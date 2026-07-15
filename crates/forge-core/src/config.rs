use std::{fs, path::PathBuf, time::Duration};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{ForgeError, Result};

const DEFAULT_DATABASE_LIMIT_MIB: u64 = 2_048;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub monitoring: MonitoringConfig,
    pub storage: StorageConfig,
    pub privacy: PrivacyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MonitoringConfig {
    pub idle_interval_ms: u64,
    pub normal_interval_ms: u64,
    pub incident_interval_ms: u64,
    pub gaming_interval_ms: u64,
    pub benchmark_interval_ms: u64,
    pub minimum_interval_ms: u64,
    pub max_cpu_overhead_percent: f64,
    pub process_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    pub database_path: PathBuf,
    pub database_limit_mib: u64,
    pub retention_days: u32,
    pub chunk_sample_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct PrivacyConfig {
    pub telemetry_enabled: bool,
    pub collect_command_lines: bool,
    pub collect_executable_paths: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            idle_interval_ms: 5_000,
            normal_interval_ms: 2_000,
            incident_interval_ms: 500,
            gaming_interval_ms: 250,
            benchmark_interval_ms: 100,
            minimum_interval_ms: 100,
            max_cpu_overhead_percent: 1.0,
            process_limit: 2_048,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        let database_path = runtime_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("forgepulse.db");
        Self {
            database_path,
            database_limit_mib: DEFAULT_DATABASE_LIMIT_MIB,
            retention_days: 30,
            chunk_sample_count: 30,
        }
    }
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            telemetry_enabled: false,
            collect_command_lines: false,
            collect_executable_paths: true,
        }
    }
}

impl AppConfig {
    pub fn load_or_create() -> Result<Self> {
        let path = config_path()?;
        if path.exists() {
            let bytes = fs::read(&path).map_err(|source| ForgeError::io(&path, source))?;
            let config: Self = serde_json::from_slice(&bytes)?;
            config.validate()?;
            return Ok(config);
        }

        let config = Self::default();
        config.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ForgeError::io(parent, source))?;
        }
        let bytes = serde_json::to_vec_pretty(&config)?;
        fs::write(&path, bytes).map_err(|source| ForgeError::io(&path, source))?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        let monitoring = &self.monitoring;
        let intervals = [
            monitoring.idle_interval_ms,
            monitoring.normal_interval_ms,
            monitoring.incident_interval_ms,
            monitoring.gaming_interval_ms,
            monitoring.benchmark_interval_ms,
        ];
        if intervals
            .into_iter()
            .any(|value| value < monitoring.minimum_interval_ms)
        {
            return Err(ForgeError::InvalidConfiguration(
                "sampling intervals must be at least minimum_interval_ms".to_owned(),
            ));
        }
        if !(0.1..=10.0).contains(&monitoring.max_cpu_overhead_percent) {
            return Err(ForgeError::InvalidConfiguration(
                "max_cpu_overhead_percent must be between 0.1 and 10".to_owned(),
            ));
        }
        if !(16..=65_536).contains(&monitoring.process_limit) {
            return Err(ForgeError::InvalidConfiguration(
                "process_limit must be between 16 and 65536".to_owned(),
            ));
        }
        if self.storage.database_limit_mib < 128 {
            return Err(ForgeError::InvalidConfiguration(
                "database_limit_mib must be at least 128".to_owned(),
            ));
        }
        if !(1..=10_000).contains(&self.storage.retention_days) {
            return Err(ForgeError::InvalidConfiguration(
                "retention_days must be between 1 and 10000".to_owned(),
            ));
        }
        if !(2..=3_600).contains(&self.storage.chunk_sample_count) {
            return Err(ForgeError::InvalidConfiguration(
                "chunk_sample_count must be between 2 and 3600".to_owned(),
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn normal_interval(&self) -> Duration {
        Duration::from_millis(self.monitoring.normal_interval_ms)
    }
}

pub fn runtime_dir() -> Result<PathBuf> {
    ProjectDirs::from("dev", "ForgePulse", "ForgePulse")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .ok_or_else(|| {
            ForgeError::InvalidConfiguration(
                "Windows local application data is unavailable".to_owned(),
            )
        })
}

pub fn config_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("config.json"))
}

pub fn ipc_key_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("ipc.key"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() -> Result<()> {
        AppConfig::default().validate()
    }

    #[test]
    fn rejects_too_fast_sampling() {
        let mut config = AppConfig::default();
        config.monitoring.normal_interval_ms = 10;
        assert!(config.validate().is_err());
    }
}
