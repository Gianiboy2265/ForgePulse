use std::{
    ffi::c_void,
    mem::size_of,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use chrono::{DateTime, Utc};
use forge_core::{
    DEFAULT_PIPE_NAME, ForgeError, Result,
    config::{AppConfig, ipc_key_path},
    ipc::{
        AuthenticatedRequest, CheckStatus, DoctorCheck, DoctorReport, IpcError, RequestCommand,
        ResponsePayload, ServiceStatus,
    },
    metrics::MetricSnapshot,
};
use forge_monitor::{AdaptiveSampler, Collector, SampleContext, SamplingSignals};
use forge_security::{AuthenticationGuard, SecretKey, read_frame, write_frame};
use forge_storage::Storage;
use forge_windows::WindowsCollector;
use parking_lot::RwLock;
use tokio::{
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::watch,
    time::MissedTickBehavior,
};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
    },
    core::w,
};

// The pipe is local-only and every request still requires the per-installation HMAC key. Granting
// authenticated local users read/write access lets an unelevated dashboard reach a service or
// console host running at a different integrity level. Restricted and packaged tokens need a
// matching ACE for Windows' second restricted-token access check; those ACEs do not independently
// satisfy the first authenticated-user check. The low-integrity label supports sandboxed desktop
// hosts, while HMAC prevents callers without the protected key from issuing commands.
const IPC_SECURITY_DESCRIPTOR: windows::core::PCWSTR =
    w!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)(A;;GRGW;;;RC)(A;;GRGW;;;AC)S:(ML;;NW;;;LW)");

#[derive(Debug)]
struct OwnedLocalSecurityDescriptor(PSECURITY_DESCRIPTOR);

impl Drop for OwnedLocalSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW allocated this exact
            // pointer with LocalAlloc, ownership has not been transferred, and this Drop runs once.
            let _released = unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
        }
    }
}

#[derive(Debug)]
pub struct ServiceRuntime {
    started_at: DateTime<Utc>,
    latest: RwLock<Option<MetricSnapshot>>,
    samples_collected: AtomicU64,
    dropped_samples: AtomicU64,
    sampling_interval_ms: AtomicU64,
    active_experiments: AtomicU64,
    database_path: PathBuf,
    storage: Storage,
}

impl ServiceRuntime {
    #[must_use]
    pub fn new(storage: Storage, database_path: PathBuf, initial_interval_ms: u64) -> Self {
        Self {
            started_at: Utc::now(),
            latest: RwLock::new(None),
            samples_collected: AtomicU64::new(0),
            dropped_samples: AtomicU64::new(0),
            sampling_interval_ms: AtomicU64::new(initial_interval_ms),
            active_experiments: AtomicU64::new(0),
            database_path,
            storage,
        }
    }

    fn status(&self) -> ServiceStatus {
        let latest = self.latest.read();
        ServiceStatus {
            service_version: env!("CARGO_PKG_VERSION").to_owned(),
            started_at: self.started_at,
            latest_sample_at: latest.as_ref().map(|sample| sample.captured_at),
            samples_collected: self.samples_collected.load(Ordering::Relaxed),
            dropped_samples: self.dropped_samples.load(Ordering::Relaxed),
            sampling_interval_ms: self.sampling_interval_ms.load(Ordering::Relaxed),
            database_bytes: database_size(&self.database_path),
            active_experiments: u32::try_from(self.active_experiments.load(Ordering::Relaxed))
                .unwrap_or(u32::MAX),
        }
    }

    fn latest_snapshot(&self) -> Option<MetricSnapshot> {
        self.latest.read().clone()
    }

    async fn doctor(&self) -> DoctorReport {
        let integrity = self.storage.quick_check().await;
        let rollback = self.storage.pending_rollback_count().await;
        let collector_ready = self.latest.read().is_some();
        let checks = vec![
            match integrity {
                Ok(status) => DoctorCheck {
                    name: "database_integrity".to_owned(),
                    status: if status.healthy {
                        CheckStatus::Pass
                    } else {
                        CheckStatus::Fail
                    },
                    details: status.details,
                },
                Err(error) => failed_check("database_integrity", error),
            },
            match rollback {
                Ok(0) => DoctorCheck {
                    name: "rollback_records".to_owned(),
                    status: CheckStatus::Pass,
                    details: "no pending rollback records".to_owned(),
                },
                Ok(count) => DoctorCheck {
                    name: "rollback_records".to_owned(),
                    status: CheckStatus::Warning,
                    details: format!("{count} rollback records require recovery"),
                },
                Err(error) => failed_check("rollback_records", error),
            },
            DoctorCheck {
                name: "collector".to_owned(),
                status: if collector_ready {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Warning
                },
                details: if collector_ready {
                    "at least one real Windows sample is available".to_owned()
                } else {
                    "the collector has not produced its first sample".to_owned()
                },
            },
        ];
        DoctorReport {
            healthy: checks.iter().all(|check| check.status != CheckStatus::Fail),
            checks,
        }
    }
}

pub async fn run(config: AppConfig, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    config.validate()?;
    let storage = Storage::open(&config.storage.database_path).await?;
    let runtime = Arc::new(ServiceRuntime::new(
        storage,
        config.storage.database_path.clone(),
        config.monitoring.normal_interval_ms,
    ));
    let key = SecretKey::load_or_create(&ipc_key_path()?)?;
    let auth = Arc::new(AuthenticationGuard::new(key));

    let sampler_runtime = Arc::clone(&runtime);
    let sampler_config = config.clone();
    let sampler_shutdown = shutdown.clone();
    let sampler = tokio::spawn(async move {
        sampling_loop(sampler_runtime, sampler_config, sampler_shutdown).await
    });

    let ipc_result = ipc_loop(Arc::clone(&runtime), auth, &mut shutdown).await;
    if ipc_result.is_err() {
        sampler.abort();
        return ipc_result;
    }
    let sampler_result = sampler
        .await
        .map_err(|error| ForgeError::Invariant(format!("sampler task failed: {error}")))?;
    sampler_result?;
    ipc_result
}

async fn sampling_loop(
    runtime: Arc<ServiceRuntime>,
    config: AppConfig,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut collector = WindowsCollector::new(
        config.monitoring.process_limit,
        config.privacy.collect_executable_paths,
    )?;
    let mut adaptive = AdaptiveSampler::new(config.monitoring.clone());
    let mut interval_duration = config.normal_interval();
    let mut timer = tokio::time::interval_at(
        tokio::time::Instant::now() + interval_duration,
        interval_duration,
    );
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut sequence = 0_u64;
    let mut buffer = Vec::with_capacity(config.storage.chunk_sample_count);

    loop {
        tokio::select! {
            _ = timer.tick() => {
                sequence = sequence.saturating_add(1);
                let context = SampleContext {
                    sequence,
                    interval: interval_duration,
                    dropped_samples: runtime.dropped_samples.load(Ordering::Relaxed),
                };
                match collector.collect(context) {
                    Ok(sample) => {
                        let collection_overhead = if interval_duration.is_zero() {
                            0.0
                        } else {
                            sample.collection_duration_us as f64
                                / interval_duration.as_micros() as f64
                                * 100.0
                        };
                        let next_interval = adaptive.update(SamplingSignals {
                            idle: false,
                            incident_active: sample.cpu.total_percent.is_some_and(|value| value >= 90.0)
                                || sample.memory.memory_load_percent >= 90,
                            game_active: false,
                            benchmark_active: false,
                            measured_overhead_percent: collection_overhead,
                        });
                        if next_interval != interval_duration {
                            interval_duration = next_interval;
                            runtime.sampling_interval_ms.store(
                                u64::try_from(interval_duration.as_millis()).unwrap_or(u64::MAX),
                                Ordering::Relaxed,
                            );
                            timer = tokio::time::interval_at(
                                tokio::time::Instant::now() + interval_duration,
                                interval_duration,
                            );
                            timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
                        }
                        *runtime.latest.write() = Some(sample.clone());
                        runtime.samples_collected.fetch_add(1, Ordering::Relaxed);
                        buffer.push(sample);
                        if buffer.len() >= config.storage.chunk_sample_count {
                            if let Err(error) = persist_metric_buffer(&runtime, &config, &buffer).await {
                                runtime.dropped_samples.fetch_add(
                                    u64::try_from(buffer.len()).unwrap_or(u64::MAX),
                                    Ordering::Relaxed,
                                );
                                tracing::error!(%error, "failed to persist metric chunk");
                            }
                            buffer.clear();
                        }
                    }
                    Err(error) => {
                        runtime.dropped_samples.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(collector = collector.name(), %error, "sample collection failed");
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
    if !buffer.is_empty() {
        persist_metric_buffer(&runtime, &config, &buffer).await?;
    }
    Ok(())
}

async fn persist_metric_buffer(
    runtime: &ServiceRuntime,
    config: &AppConfig,
    buffer: &[MetricSnapshot],
) -> Result<()> {
    runtime.storage.insert_metric_chunk(buffer).await?;
    let retention = chrono::Duration::days(i64::from(config.storage.retention_days));
    let cutoff = (Utc::now() - retention).timestamp_millis();
    runtime.storage.enforce_retention(cutoff, 128).await?;

    let size_limit_bytes = config
        .storage
        .database_limit_mib
        .saturating_mul(1024 * 1024);
    if database_size(&runtime.database_path) > size_limit_bytes {
        let deleted = runtime.storage.delete_oldest_metric_chunks(128).await?;
        tracing::warn!(
            deleted,
            size_limit_bytes,
            "database size limit triggered metric eviction"
        );
    }
    Ok(())
}

async fn ipc_loop(
    runtime: Arc<ServiceRuntime>,
    auth: Arc<AuthenticationGuard>,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()> {
    let mut first_instance = true;
    loop {
        let server = create_ipc_server(first_instance)?;
        first_instance = false;
        tokio::select! {
            connected = server.connect() => {
                connected.map_err(|error| ForgeError::Protocol(format!("pipe connection failed: {error}")))?;
                let client_runtime = Arc::clone(&runtime);
                let client_auth = Arc::clone(&auth);
                tokio::spawn(async move {
                    if let Err(error) = handle_client(server, client_runtime, client_auth).await {
                        tracing::debug!(%error, "IPC client request failed");
                    }
                });
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn create_ipc_server(first_instance: bool) -> Result<NamedPipeServer> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: IPC_SECURITY_DESCRIPTOR is a static, null-terminated SDDL string. `descriptor` is a
    // valid out pointer and the returned LocalAlloc allocation is immediately owned by the guard.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            IPC_SECURITY_DESCRIPTOR,
            SDDL_REVISION_1,
            &raw mut descriptor,
            None,
        )
    }
    .map_err(|error| {
        ForgeError::ServiceUnavailable(format!("creating IPC security descriptor failed: {error}"))
    })?;
    let descriptor = OwnedLocalSecurityDescriptor(descriptor);
    let attribute_size = u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).map_err(|error| {
        ForgeError::InvalidConfiguration(format!(
            "SECURITY_ATTRIBUTES size does not fit a Win32 DWORD: {error}"
        ))
    })?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: attribute_size,
        lpSecurityDescriptor: descriptor.0.0,
        bInheritHandle: false.into(),
    };
    let mut options = ServerOptions::new();
    options.first_pipe_instance(first_instance);
    // SAFETY: `attributes` has the Win32 SECURITY_ATTRIBUTES layout and points to the live guarded
    // descriptor above. CreateNamedPipeW consumes both only during this call and does not retain
    // either pointer; the pipe handle owns its copied security descriptor after creation.
    unsafe {
        options.create_with_security_attributes_raw(
            DEFAULT_PIPE_NAME,
            std::ptr::from_mut(&mut attributes).cast::<c_void>(),
        )
    }
    .map_err(|error| ForgeError::ServiceUnavailable(format!("creating IPC pipe failed: {error}")))
}

async fn handle_client(
    mut pipe: NamedPipeServer,
    runtime: Arc<ServiceRuntime>,
    auth: Arc<AuthenticationGuard>,
) -> Result<()> {
    let request: AuthenticatedRequest = read_frame(&mut pipe).await?;
    if let Err(error) = auth.verify(&request) {
        let response = ResponsePayload::Error(IpcError {
            code: "authentication_failed".to_owned(),
            message: "request authentication failed".to_owned(),
            retryable: false,
        });
        write_frame(&mut pipe, &response).await?;
        return Err(error);
    }
    let response = dispatch(&runtime, request.command).await;
    write_frame(&mut pipe, &response).await
}

async fn dispatch(runtime: &ServiceRuntime, command: RequestCommand) -> ResponsePayload {
    match command {
        RequestCommand::Ping => ResponsePayload::Pong {
            service_time: Utc::now(),
        },
        RequestCommand::Status => ResponsePayload::Status(runtime.status()),
        RequestCommand::Snapshot => runtime.latest_snapshot().map_or_else(
            || {
                ResponsePayload::Error(IpcError {
                    code: "sample_pending".to_owned(),
                    message: "the first collector sample is not ready".to_owned(),
                    retryable: true,
                })
            },
            ResponsePayload::Snapshot,
        ),
        RequestCommand::Processes { limit } => runtime.latest_snapshot().map_or_else(
            || {
                ResponsePayload::Error(IpcError {
                    code: "sample_pending".to_owned(),
                    message: "the first collector sample is not ready".to_owned(),
                    retryable: true,
                })
            },
            |sample| {
                ResponsePayload::Processes(
                    sample
                        .processes
                        .into_iter()
                        .take(usize::from(limit))
                        .collect(),
                )
            },
        ),
        RequestCommand::Doctor => ResponsePayload::Doctor(runtime.doctor().await),
    }
}

fn failed_check(name: &str, error: ForgeError) -> DoctorCheck {
    DoctorCheck {
        name: name.to_owned(),
        status: CheckStatus::Fail,
        details: error.to_string(),
    }
}

fn database_size(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |metadata| metadata.len())
        + std::fs::metadata(path.with_extension("db-wal")).map_or(0, |metadata| metadata.len())
}
