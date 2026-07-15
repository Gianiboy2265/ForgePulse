use chrono::{DateTime, Utc};
use forge_core::{
    ForgeError, Result,
    safety::{RiskClass, RollbackState},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentSpec {
    pub id: Uuid,
    pub title: String,
    pub hypothesis: String,
    pub target_metric: String,
    pub control_configuration: serde_json::Value,
    pub test_configuration: serde_json::Value,
    pub preparation_steps: Vec<String>,
    pub duration_seconds: u32,
    pub minimum_sample_size: u32,
    pub rollback_action: Action,
    pub success_criteria: String,
    pub failure_criteria: String,
    pub risk: RiskClass,
    pub expected_disadvantages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", content = "parameters", rename_all = "snake_case")]
pub enum Action {
    RestoreProcessPriority {
        pid: u32,
        creation_time_100ns: u64,
        priority: u32,
    },
    RestoreProcessAffinity {
        pid: u32,
        creation_time_100ns: u64,
        mask: usize,
    },
    RestorePowerPlan {
        scheme_guid: Uuid,
    },
    RestoreOpaqueState {
        kind: String,
        state: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Preparing,
    Control,
    Applying,
    Test,
    RollingBack,
    VerifyingRollback,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentRun {
    pub id: Uuid,
    pub experiment_id: Uuid,
    pub state: RunState,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackRecord {
    pub id: Uuid,
    pub experiment_run_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub state: RollbackState,
    pub action: Action,
    pub original_state_hash: String,
    pub verification_details: Option<String>,
}

impl ExperimentSpec {
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty() || self.title.len() > 160 {
            return Err(ForgeError::InvalidInput(
                "experiment title must contain 1 to 160 characters".to_owned(),
            ));
        }
        if self.hypothesis.trim().is_empty() {
            return Err(ForgeError::InvalidInput(
                "experiment hypothesis is required".to_owned(),
            ));
        }
        if !(5..=86_400).contains(&self.duration_seconds) {
            return Err(ForgeError::InvalidInput(
                "duration must be between 5 seconds and 24 hours".to_owned(),
            ));
        }
        if !(2..=10_000).contains(&self.minimum_sample_size) {
            return Err(ForgeError::InvalidInput(
                "minimum sample size must be between 2 and 10000".to_owned(),
            ));
        }
        if self.risk == RiskClass::Blocked {
            return Err(ForgeError::PermissionDenied {
                operation: "creating experiment".to_owned(),
                details: "blocked actions cannot be represented as executable experiments"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

impl ExperimentRun {
    #[must_use]
    pub fn new(experiment_id: Uuid, now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            experiment_id,
            state: RunState::Preparing,
            started_at: now,
            updated_at: now,
            error: None,
        }
    }

    pub fn transition(&mut self, next: RunState, now: DateTime<Utc>) -> Result<()> {
        let valid = matches!(
            (self.state, next),
            (RunState::Preparing, RunState::Control)
                | (RunState::Control, RunState::Applying)
                | (RunState::Applying, RunState::Test)
                | (RunState::Preparing, RunState::RollingBack)
                | (RunState::Control, RunState::RollingBack)
                | (RunState::Applying, RunState::RollingBack)
                | (RunState::Test, RunState::RollingBack)
                | (RunState::RollingBack, RunState::VerifyingRollback)
                | (RunState::VerifyingRollback, RunState::Completed)
                | (RunState::VerifyingRollback, RunState::Cancelled)
                | (RunState::VerifyingRollback, RunState::Failed)
                | (RunState::Preparing, RunState::Cancelled)
        );
        if !valid {
            return Err(ForgeError::Invariant(format!(
                "invalid experiment transition {:?} -> {next:?}",
                self.state
            )));
        }
        self.state = next;
        self.updated_at = now;
        Ok(())
    }
}

impl RollbackRecord {
    #[must_use]
    pub fn pending(experiment_run_id: Uuid, action: Action, state_hash: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            experiment_run_id,
            created_at: now,
            updated_at: now,
            state: RollbackState::Pending,
            action,
            original_state_hash: state_hash,
            verification_details: None,
        }
    }

    #[must_use]
    pub const fn needs_recovery(&self) -> bool {
        matches!(
            self.state,
            RollbackState::Pending | RollbackState::Armed | RollbackState::Restoring
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_skipping_rollback() {
        let now = Utc::now();
        let mut run = ExperimentRun::new(Uuid::new_v4(), now);
        assert!(run.transition(RunState::Completed, now).is_err());
    }

    #[test]
    fn pending_record_is_recoverable() {
        let record = RollbackRecord::pending(
            Uuid::new_v4(),
            Action::RestoreOpaqueState {
                kind: "test".to_owned(),
                state: serde_json::Value::Null,
            },
            "hash".to_owned(),
        );
        assert!(record.needs_recovery());
    }
}
