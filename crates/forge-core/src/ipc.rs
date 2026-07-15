use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::metrics::{MetricSnapshot, ProcessMetrics};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "command", content = "arguments", rename_all = "snake_case")]
pub enum RequestCommand {
    Ping,
    Status,
    Snapshot,
    Processes { limit: u16 },
    Doctor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthenticatedRequest {
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub timestamp_ms: i64,
    pub nonce: String,
    pub command: RequestCommand,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UnsignedRequest<'a> {
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub timestamp_ms: i64,
    pub nonce: &'a str,
    pub command: &'a RequestCommand,
}

impl AuthenticatedRequest {
    #[must_use]
    pub fn unsigned(&self) -> UnsignedRequest<'_> {
        UnsignedRequest {
            protocol_version: self.protocol_version,
            request_id: self.request_id,
            timestamp_ms: self.timestamp_ms,
            nonce: &self.nonce,
            command: &self.command,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "response", content = "data", rename_all = "snake_case")]
pub enum ResponsePayload {
    Pong { service_time: DateTime<Utc> },
    Status(ServiceStatus),
    Snapshot(MetricSnapshot),
    Processes(Vec<ProcessMetrics>),
    Doctor(DoctorReport),
    Error(IpcError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceStatus {
    pub service_version: String,
    pub started_at: DateTime<Utc>,
    pub latest_sample_at: Option<DateTime<Utc>>,
    pub samples_collected: u64,
    pub dropped_samples: u64,
    pub sampling_interval_ms: u64,
    pub database_bytes: u64,
    pub active_experiments: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorReport {
    pub healthy: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}
