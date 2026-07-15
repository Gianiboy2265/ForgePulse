use forge_core::{ForgeError, Result, safety::RiskClass};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationRule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
    pub require_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "trigger", content = "parameters", rename_all = "snake_case")]
pub enum Trigger {
    ProcessStarts { executable_name: String },
    ProcessExits { executable_name: String },
    CpuExceeds { percent: f64, duration_seconds: u32 },
    MemoryExceeds { percent: f64, duration_seconds: u32 },
    SystemIdle { duration_seconds: u32 },
    AcPowerChanged { connected: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "condition", content = "parameters", rename_all = "snake_case")]
pub enum Condition {
    ApplicationForeground { executable_name: String },
    ProcessExists { executable_name: String },
    CpuBelow { percent: f64 },
    MemoryBelow { percent: f64 },
    OnAcPower { expected: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", content = "parameters", rename_all = "snake_case")]
pub enum Action {
    StartSession { name: String },
    ShowNotification { message: String },
    LogMarker { message: String },
    ActivateProfile { profile_id: Uuid },
    RestoreProfile { profile_id: Uuid },
    Delay { milliseconds: u32 },
}

#[derive(Debug, Clone, Default)]
pub struct EvaluationContext {
    pub foreground_executable: Option<String>,
    pub running_executables: Vec<String>,
    pub cpu_percent: Option<f64>,
    pub memory_percent: Option<f64>,
    pub on_ac_power: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DryRunPlan {
    pub rule_id: Uuid,
    pub would_run: bool,
    pub failed_conditions: Vec<String>,
    pub actions: Vec<Action>,
    pub highest_risk: RiskClass,
}

impl AutomationRule {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 100 {
            return Err(ForgeError::InvalidInput(
                "automation name must contain 1 to 100 characters".to_owned(),
            ));
        }
        if self.actions.is_empty() || self.actions.len() > 64 {
            return Err(ForgeError::InvalidInput(
                "automation must contain 1 to 64 actions".to_owned(),
            ));
        }
        validate_percentage_trigger(&self.trigger)?;
        Ok(())
    }

    #[must_use]
    pub fn dry_run(&self, context: &EvaluationContext) -> DryRunPlan {
        let failed_conditions: Vec<String> = self
            .conditions
            .iter()
            .filter(|condition| !condition_matches(condition, context))
            .map(|condition| format!("{condition:?}"))
            .collect();
        DryRunPlan {
            rule_id: self.id,
            would_run: self.enabled && failed_conditions.is_empty(),
            failed_conditions,
            actions: self.actions.clone(),
            highest_risk: self
                .actions
                .iter()
                .map(action_risk)
                .max()
                .unwrap_or(RiskClass::ReadOnly),
        }
    }
}

fn validate_percentage_trigger(trigger: &Trigger) -> Result<()> {
    let percentage = match trigger {
        Trigger::CpuExceeds { percent, .. } | Trigger::MemoryExceeds { percent, .. } => {
            Some(*percent)
        }
        _ => None,
    };
    if percentage.is_some_and(|value| !(0.0..=100.0).contains(&value)) {
        return Err(ForgeError::InvalidInput(
            "automation percentage must be between 0 and 100".to_owned(),
        ));
    }
    Ok(())
}

fn condition_matches(condition: &Condition, context: &EvaluationContext) -> bool {
    match condition {
        Condition::ApplicationForeground { executable_name } => context
            .foreground_executable
            .as_ref()
            .is_some_and(|current| current.eq_ignore_ascii_case(executable_name)),
        Condition::ProcessExists { executable_name } => context
            .running_executables
            .iter()
            .any(|current| current.eq_ignore_ascii_case(executable_name)),
        Condition::CpuBelow { percent } => {
            context.cpu_percent.is_some_and(|value| value < *percent)
        }
        Condition::MemoryBelow { percent } => {
            context.memory_percent.is_some_and(|value| value < *percent)
        }
        Condition::OnAcPower { expected } => context.on_ac_power == Some(*expected),
    }
}

const fn action_risk(action: &Action) -> RiskClass {
    match action {
        Action::StartSession { .. }
        | Action::ShowNotification { .. }
        | Action::LogMarker { .. }
        | Action::Delay { .. } => RiskClass::ReadOnly,
        Action::ActivateProfile { .. } | Action::RestoreProfile { .. } => RiskClass::Caution,
    }
}
