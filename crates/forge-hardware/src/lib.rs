use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareDevice {
    pub id: Uuid,
    pub stable_instance_id: String,
    pub class: DeviceClass,
    pub vendor: Option<String>,
    pub model: String,
    pub driver_version: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    Cpu,
    Gpu,
    Memory,
    Storage,
    Network,
    Display,
    Usb,
    Audio,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareChange {
    pub detected_at: DateTime<Utc>,
    pub device_id: Uuid,
    pub kind: ChangeKind,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    DriverChanged,
    ConfigurationChanged,
}
