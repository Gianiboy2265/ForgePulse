use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    mem::size_of,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    time::Instant,
};

use chrono::Utc;
use forge_core::{
    ForgeError, Result,
    metrics::{
        Availability, Capability, CpuMetrics, MemoryMetrics, MetricSnapshot, ProcessIdentity,
        ProcessMetrics,
    },
};
use forge_monitor::{Collector, SampleContext};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE},
        System::{
            ProcessStatus::{
                K32EnumProcesses, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
            },
            SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::{
                ALL_PROCESSOR_GROUPS, GetActiveProcessorCount, GetProcessHandleCount,
                GetProcessTimes, GetSystemTimes, OpenProcess, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ, QueryFullProcessImageNameW,
            },
        },
    },
    core::PWSTR,
};

const MAX_EXECUTABLE_PATH_UTF16: usize = 32_768;

#[derive(Debug, Clone, Copy)]
struct CpuTimes {
    idle: u64,
    kernel: u64,
    user: u64,
}

impl CpuTimes {
    const fn total(self) -> u64 {
        self.kernel.saturating_add(self.user)
    }
}

#[derive(Debug)]
struct OwnedProcessHandle(HANDLE);

impl Drop for OwnedProcessHandle {
    fn drop(&mut self) {
        // SAFETY: OpenProcess returned this owned, non-null handle. This wrapper is
        // not Clone, so Drop closes it exactly once.
        let result = unsafe { CloseHandle(self.0) };
        if let Err(error) = result {
            tracing::debug!(%error, "failed to close process handle");
        }
    }
}

#[derive(Debug)]
pub struct WindowsCollector {
    process_limit: usize,
    collect_paths: bool,
    logical_processor_count: u32,
    previous_system: Option<CpuTimes>,
    previous_process_cpu: HashMap<ProcessIdentity, u64>,
}

impl WindowsCollector {
    pub fn new(process_limit: usize, collect_paths: bool) -> Result<Self> {
        if !(16..=65_536).contains(&process_limit) {
            return Err(ForgeError::InvalidConfiguration(
                "Windows collector process limit must be between 16 and 65536".to_owned(),
            ));
        }
        // SAFETY: GetActiveProcessorCount has no pointer parameters and the group
        // constant requests the documented aggregate across processor groups.
        let logical_processor_count = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
        if logical_processor_count == 0 {
            return Err(ForgeError::Collector {
                collector: "windows".to_owned(),
                details: "Windows reported zero active processors".to_owned(),
            });
        }
        Ok(Self {
            process_limit,
            collect_paths,
            logical_processor_count,
            previous_system: None,
            previous_process_cpu: HashMap::new(),
        })
    }

    fn collect_inner(&mut self, context: SampleContext) -> Result<MetricSnapshot> {
        let started = Instant::now();
        let captured_at = Utc::now();
        let current_system = system_cpu_times()?;
        let memory = system_memory()?;
        let system_delta = self.previous_system.and_then(|previous| {
            let total = current_system.total().checked_sub(previous.total())?;
            let idle = current_system.idle.checked_sub(previous.idle)?;
            (total > 0).then_some((total, idle))
        });
        let total_percent = system_delta.map(|(total, idle)| {
            let busy = total.saturating_sub(idle);
            (busy as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
        });

        let (processes, inaccessible_count) = self.collect_processes(system_delta.map(|v| v.0))?;
        self.previous_system = Some(current_system);

        let mut capabilities = BTreeMap::new();
        capabilities.insert(Capability::CpuTotal, Availability::Available);
        capabilities.insert(Capability::MemorySystem, Availability::Available);
        capabilities.insert(
            Capability::ProcessInventory,
            if inaccessible_count == 0 {
                Availability::Available
            } else {
                Availability::PermissionDenied(format!(
                    "{inaccessible_count} protected or exited processes were not queryable"
                ))
            },
        );
        capabilities.insert(Capability::CpuPerProcess, Availability::Available);
        let missing_process_memory = processes
            .iter()
            .filter(|process| process.working_set_bytes.is_none())
            .count();
        capabilities.insert(
            Capability::MemoryPerProcess,
            if missing_process_memory == 0 {
                Availability::Available
            } else {
                Availability::PermissionDenied(format!(
                    "memory counters were unavailable for {missing_process_memory} processes"
                ))
            },
        );
        capabilities.insert(
            Capability::ExecutablePath,
            if self.collect_paths {
                if inaccessible_count == 0 {
                    Availability::Available
                } else {
                    Availability::PermissionDenied(format!(
                        "{inaccessible_count} protected or exited processes were not queryable"
                    ))
                }
            } else {
                Availability::Unsupported("disabled by privacy configuration".to_owned())
            },
        );

        Ok(MetricSnapshot {
            sequence: context.sequence,
            captured_at,
            collection_duration_us: u64::try_from(started.elapsed().as_micros())
                .unwrap_or(u64::MAX),
            sampling_interval_ms: u64::try_from(context.interval.as_millis()).unwrap_or(u64::MAX),
            dropped_samples: context.dropped_samples,
            cpu: CpuMetrics {
                total_percent,
                logical_processor_count: self.logical_processor_count,
            },
            memory,
            processes,
            capabilities,
        })
    }

    fn collect_processes(
        &mut self,
        system_delta: Option<u64>,
    ) -> Result<(Vec<ProcessMetrics>, usize)> {
        let pids = enumerate_process_ids(self.process_limit)?;
        let mut next_cpu_state = HashMap::with_capacity(pids.len());
        let mut processes = Vec::with_capacity(pids.len());
        let mut inaccessible = 0_usize;
        for pid in pids {
            match query_process(pid, self.collect_paths) {
                Ok((mut process, cumulative_cpu)) => {
                    let identity = process.identity.clone();
                    process.cpu_percent = system_delta.and_then(|total_delta| {
                        self.previous_process_cpu
                            .get(&identity)
                            .and_then(|previous| cumulative_cpu.checked_sub(*previous))
                            .map(|delta| {
                                (delta as f64 / total_delta as f64 * 100.0).clamp(0.0, 100.0)
                            })
                    });
                    next_cpu_state.insert(identity, cumulative_cpu);
                    processes.push(process);
                }
                Err(error) => {
                    inaccessible = inaccessible.saturating_add(1);
                    tracing::trace!(pid, %error, "process became inaccessible during collection");
                }
            }
        }
        self.previous_process_cpu = next_cpu_state;
        processes.sort_by(|left, right| {
            right
                .cpu_percent
                .unwrap_or_default()
                .total_cmp(&left.cpu_percent.unwrap_or_default())
                .then_with(|| {
                    right
                        .working_set_bytes
                        .unwrap_or_default()
                        .cmp(&left.working_set_bytes.unwrap_or_default())
                })
        });
        Ok((processes, inaccessible))
    }
}

impl Collector for WindowsCollector {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn collect(&mut self, context: SampleContext) -> Result<MetricSnapshot> {
        self.collect_inner(context)
    }
}

fn system_cpu_times() -> Result<CpuTimes> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: all pointers refer to initialized, writable FILETIME values for the
    // full call duration. GetSystemTimes writes exactly those three structures.
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.map_err(
        |error| ForgeError::Collector {
            collector: "cpu".to_owned(),
            details: error.to_string(),
        },
    )?;
    Ok(CpuTimes {
        idle: filetime_ticks(idle),
        kernel: filetime_ticks(kernel),
        user: filetime_ticks(user),
    })
}

fn system_memory() -> Result<MemoryMetrics> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).map_err(|_| {
            ForgeError::Invariant("MEMORYSTATUSEX size did not fit in u32".to_owned())
        })?,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: status is initialized with the required dwLength and remains writable
    // for the call. GlobalMemoryStatusEx fills this fixed-size structure.
    unsafe { GlobalMemoryStatusEx(&mut status) }.map_err(|error| ForgeError::Collector {
        collector: "memory".to_owned(),
        details: error.to_string(),
    })?;
    Ok(MemoryMetrics {
        total_physical_bytes: status.ullTotalPhys,
        available_physical_bytes: status.ullAvailPhys,
        committed_bytes: status
            .ullTotalPageFile
            .saturating_sub(status.ullAvailPageFile),
        commit_limit_bytes: status.ullTotalPageFile,
        memory_load_percent: status.dwMemoryLoad,
    })
}

fn enumerate_process_ids(limit: usize) -> Result<Vec<u32>> {
    let mut pids = vec![0_u32; limit];
    let byte_capacity =
        u32::try_from(pids.len().saturating_mul(size_of::<u32>())).map_err(|_| {
            ForgeError::InvalidConfiguration("process enumeration buffer is too large".to_owned())
        })?;
    let mut bytes_written = 0_u32;
    // SAFETY: pids is a contiguous writable u32 array with byte_capacity bytes,
    // and bytes_written is a valid output pointer. The API cannot exceed cb.
    unsafe { K32EnumProcesses(pids.as_mut_ptr(), byte_capacity, &mut bytes_written) }
        .ok()
        .map_err(|error| ForgeError::Collector {
            collector: "processes".to_owned(),
            details: error.to_string(),
        })?;
    let count = usize::try_from(bytes_written)
        .map_err(|_| ForgeError::Invariant("process byte count did not fit usize".to_owned()))?
        / size_of::<u32>();
    pids.truncate(count.min(pids.len()));
    pids.retain(|pid| *pid != 0);
    Ok(pids)
}

fn open_process(pid: u32, with_memory: bool) -> Result<OwnedProcessHandle> {
    let rights = if with_memory {
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ
    } else {
        PROCESS_QUERY_LIMITED_INFORMATION
    };
    // SAFETY: pid is data, no pointer parameters are involved, and the returned
    // owned handle is immediately wrapped for deterministic CloseHandle.
    let handle =
        unsafe { OpenProcess(rights, false, pid) }.map_err(|error| ForgeError::Collector {
            collector: "processes".to_owned(),
            details: format!("OpenProcess({pid}) failed: {error}"),
        })?;
    Ok(OwnedProcessHandle(handle))
}

fn query_process(pid: u32, collect_path: bool) -> Result<(ProcessMetrics, u64)> {
    let handle = open_process(pid, true).or_else(|_| open_process(pid, false))?;
    let (creation_time, cumulative_cpu) = query_process_times(&handle)?;
    let path = if collect_path {
        query_process_path(&handle).ok().flatten()
    } else {
        None
    };
    let executable_name = path
        .as_ref()
        .and_then(|value| value.file_name())
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("PID {pid}"));
    let memory = query_process_memory(&handle).ok();
    let (working_set_bytes, private_bytes) = memory
        .map_or((None, None), |(working_set, private)| {
            (Some(working_set), Some(private))
        });
    let handle_count = query_handle_count(&handle).ok();
    Ok((
        ProcessMetrics {
            identity: ProcessIdentity {
                pid,
                creation_time_100ns: creation_time,
            },
            executable_name,
            executable_path: path,
            cpu_percent: None,
            working_set_bytes,
            private_bytes,
            handle_count,
            thread_count: None,
            inaccessible_reason: memory
                .is_none()
                .then(|| "per-process memory counters were not accessible".to_owned()),
        },
        cumulative_cpu,
    ))
}

fn query_process_times(handle: &OwnedProcessHandle) -> Result<(u64, u64)> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: handle stays valid through the call and all four FILETIME pointers
    // refer to initialized writable structures.
    unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|error| ForgeError::Collector {
            collector: "processes".to_owned(),
            details: format!("GetProcessTimes failed: {error}"),
        })?;
    Ok((
        filetime_ticks(creation),
        filetime_ticks(kernel).saturating_add(filetime_ticks(user)),
    ))
}

fn query_process_path(handle: &OwnedProcessHandle) -> Result<Option<PathBuf>> {
    let mut buffer = vec![0_u16; MAX_EXECUTABLE_PATH_UTF16];
    let mut length = u32::try_from(buffer.len())
        .map_err(|_| ForgeError::Invariant("path buffer was too large".to_owned()))?;
    // SAFETY: buffer provides `length` writable UTF-16 code units, handle is valid,
    // and length remains writable so Windows can return the actual character count.
    unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .map_err(|error| ForgeError::Collector {
        collector: "process_path".to_owned(),
        details: error.to_string(),
    })?;
    let length = usize::try_from(length)
        .map_err(|_| ForgeError::Invariant("path length did not fit usize".to_owned()))?;
    if length == 0 || length > buffer.len() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(OsString::from_wide(&buffer[..length]))))
}

fn query_process_memory(handle: &OwnedProcessHandle) -> Result<(u64, u64)> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
    let size = u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX>())
        .map_err(|_| ForgeError::Invariant("process memory structure was too large".to_owned()))?;
    // SAFETY: counters is a writable PROCESS_MEMORY_COUNTERS_EX whose initial base
    // layout is PROCESS_MEMORY_COUNTERS, and `size` advertises the extended size.
    unsafe { K32GetProcessMemoryInfo(handle.0, std::ptr::from_mut(&mut counters).cast(), size) }
        .ok()
        .map_err(|error| ForgeError::Collector {
            collector: "process_memory".to_owned(),
            details: error.to_string(),
        })?;
    let working_set = u64::try_from(counters.WorkingSetSize)
        .map_err(|_| ForgeError::Invariant("working-set size did not fit u64".to_owned()))?;
    let private_bytes = u64::try_from(counters.PrivateUsage)
        .map_err(|_| ForgeError::Invariant("private byte count did not fit u64".to_owned()))?;
    Ok((working_set, private_bytes))
}

fn query_handle_count(handle: &OwnedProcessHandle) -> Result<u32> {
    let mut count = 0_u32;
    // SAFETY: handle is valid and count is a writable u32 output value.
    unsafe { GetProcessHandleCount(handle.0, &mut count) }.map_err(|error| {
        ForgeError::Collector {
            collector: "process_handles".to_owned(),
            details: error.to_string(),
        }
    })?;
    Ok(count)
}

fn filetime_ticks(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}
