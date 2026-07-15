use std::{collections::BTreeMap, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    CpuTotal,
    CpuPerProcess,
    MemorySystem,
    MemoryPerProcess,
    ProcessInventory,
    ExecutablePath,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
pub enum Availability {
    Available,
    PermissionDenied(String),
    Unsupported(String),
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricSnapshot {
    pub sequence: u64,
    pub captured_at: DateTime<Utc>,
    pub collection_duration_us: u64,
    pub sampling_interval_ms: u64,
    pub dropped_samples: u64,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub processes: Vec<ProcessMetrics>,
    pub capabilities: BTreeMap<Capability, Availability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CpuMetrics {
    /// None until a valid pair of cumulative tick samples exists.
    pub total_percent: Option<f64>,
    pub logical_processor_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryMetrics {
    pub total_physical_bytes: u64,
    pub available_physical_bytes: u64,
    pub committed_bytes: u64,
    pub commit_limit_bytes: u64,
    pub memory_load_percent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Raw Windows creation timestamp in 100-nanosecond FILETIME units.
    /// Serialized as text because FILETIME values exceed JavaScript's safe integer range.
    #[serde(with = "u64_string")]
    pub creation_time_100ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessMetrics {
    pub identity: ProcessIdentity,
    pub executable_name: String,
    pub executable_path: Option<PathBuf>,
    pub cpu_percent: Option<f64>,
    pub working_set_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    pub handle_count: Option<u32>,
    pub thread_count: Option<u32>,
    pub inaccessible_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverheadMetrics {
    pub cpu_percent: Option<f64>,
    pub resident_memory_bytes: u64,
    pub database_bytes: u64,
    pub sampling_interval_ms: u64,
    pub dropped_samples: u64,
}

impl MetricSnapshot {
    #[must_use]
    pub fn process(&self, identity: &ProcessIdentity) -> Option<&ProcessMetrics> {
        self.processes
            .iter()
            .find(|process| process.identity == *identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_reuse_has_distinct_identity() {
        let first = ProcessIdentity {
            pid: 42,
            creation_time_100ns: 100,
        };
        let reused = ProcessIdentity {
            pid: 42,
            creation_time_100ns: 200,
        };
        assert_ne!(first, reused);
    }
}

mod u64_string {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse::<u64>().map_err(D::Error::custom)
    }
}
