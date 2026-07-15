use std::path::PathBuf;

use forge_core::{ForgeError, Result, safety::RiskClass};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApplicationProfile {
    pub id: Uuid,
    pub name: String,
    pub executable: PathBuf,
    pub enabled: bool,
    pub trigger: ProfileTrigger,
    pub actions: Vec<ProfileAction>,
    pub maximum_duration_seconds: u32,
    pub restore_on_exit: bool,
    pub testing_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileTrigger {
    pub delay_ms: u32,
    pub require_foreground: bool,
    pub require_fullscreen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", content = "parameters", rename_all = "snake_case")]
pub enum ProfileAction {
    StartSession { name: String },
    ChangeProcessPriority { priority: ProcessPriority },
    ChangeProcessAffinity { mask: usize },
    SwitchPowerPlan { scheme_guid: Uuid },
    PauseApprovedApplication { executable: PathBuf },
    MonitorLatency { host: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessPriority {
    BelowNormal,
    Normal,
    AboveNormal,
    High,
}

impl ProfileAction {
    #[must_use]
    pub const fn risk(&self) -> RiskClass {
        match self {
            Self::StartSession { .. } | Self::MonitorLatency { .. } => RiskClass::ReadOnly,
            Self::ChangeProcessPriority { .. } | Self::ChangeProcessAffinity { .. } => {
                RiskClass::Safe
            }
            Self::SwitchPowerPlan { .. } | Self::PauseApprovedApplication { .. } => {
                RiskClass::Caution
            }
        }
    }
}

impl ApplicationProfile {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 100 {
            return Err(ForgeError::InvalidInput(
                "profile name must contain 1 to 100 characters".to_owned(),
            ));
        }
        if !self.executable.is_absolute() {
            return Err(ForgeError::InvalidInput(
                "profile executable must be an absolute path".to_owned(),
            ));
        }
        if self.actions.is_empty() || self.actions.len() > 64 {
            return Err(ForgeError::InvalidInput(
                "profile must contain 1 to 64 actions".to_owned(),
            ));
        }
        if !(5..=604_800).contains(&self.maximum_duration_seconds) {
            return Err(ForgeError::InvalidInput(
                "profile maximum duration must be between 5 seconds and 7 days".to_owned(),
            ));
        }
        if self
            .actions
            .iter()
            .any(|action| action.risk() > RiskClass::ReadOnly && !self.restore_on_exit)
        {
            return Err(ForgeError::InvalidInput(
                "profiles with mutating actions must restore on exit".to_owned(),
            ));
        }
        Ok(())
    }
}
