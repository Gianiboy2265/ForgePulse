use chrono::{DateTime, Utc};
use forge_core::metrics::ProcessIdentity;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEvent {
    pub id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub source: EventSource,
    pub kind: EventKind,
    pub summary: String,
    pub process: Option<ProcessIdentity>,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Collector,
    WindowsEventLog,
    ServiceControlManager,
    User,
    Benchmark,
    Experiment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    ProcessStarted,
    ProcessExited,
    ServiceStarted,
    ServiceStopped,
    Crash,
    ForegroundChanged,
    SessionChanged,
    PowerChanged,
    HardwareChanged,
    Marker,
    ConfigurationChanged,
}

impl TimelineEvent {
    #[must_use]
    pub fn marker(summary: impl Into<String>, occurred_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            occurred_at,
            source: EventSource::User,
            kind: EventKind::Marker,
            summary: summary.into(),
            process: None,
            properties: serde_json::Value::Null,
        }
    }
}
