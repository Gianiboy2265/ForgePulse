use chrono::{DateTime, Utc};
use forge_core::{
    metrics::MetricSnapshot,
    safety::{Causality, EvidenceDirection},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Incident {
    pub id: Uuid,
    pub kind: IncidentKind,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub severity: Severity,
    pub confidence: f64,
    pub causality: Causality,
    pub summary: String,
    pub evidence: Vec<Evidence>,
    pub recommended_tests: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncidentKind {
    CpuSaturation,
    BackgroundCpuSpike,
    MemoryPressure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    pub metric: String,
    pub observation: String,
    pub direction: EvidenceDirection,
    pub weight: f64,
}

#[derive(Debug, Clone)]
pub struct RuleEngine {
    cpu_warning_percent: f64,
    cpu_critical_percent: f64,
    memory_warning_percent: u32,
    memory_critical_percent: u32,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self {
            cpu_warning_percent: 90.0,
            cpu_critical_percent: 98.0,
            memory_warning_percent: 85,
            memory_critical_percent: 95,
        }
    }
}

impl RuleEngine {
    #[must_use]
    pub fn evaluate(&self, samples: &[MetricSnapshot]) -> Vec<Incident> {
        if samples.len() < 3 {
            return Vec::new();
        }
        let mut incidents = Vec::new();
        let recent = &samples[samples.len() - 3..];
        let cpu: Vec<f64> = recent
            .iter()
            .filter_map(|sample| sample.cpu.total_percent)
            .collect();
        if cpu.len() == recent.len() && cpu.iter().all(|value| *value >= self.cpu_warning_percent) {
            let peak = cpu.iter().copied().fold(0.0, f64::max);
            let severity = if peak >= self.cpu_critical_percent {
                Severity::Critical
            } else {
                Severity::Warning
            };
            incidents.push(Incident {
                id: Uuid::new_v4(),
                kind: IncidentKind::CpuSaturation,
                start_time: recent[0].captured_at,
                end_time: None,
                severity,
                confidence: weighted_confidence(&[0.55, 0.25, 0.15]),
                causality: Causality::Correlation,
                summary: format!("CPU remained above 90% for three samples (peak {peak:.1}%)"),
                evidence: vec![Evidence {
                    metric: "cpu.total_percent".to_owned(),
                    observation: format!("samples: {cpu:.1?}"),
                    direction: EvidenceDirection::Supports,
                    weight: 0.55,
                }],
                recommended_tests: vec![
                    "Compare per-process CPU attribution during a repeated workload".to_owned(),
                ],
            });
        }

        let memory_load = recent
            .iter()
            .map(|sample| sample.memory.memory_load_percent)
            .max()
            .unwrap_or_default();
        if memory_load >= self.memory_warning_percent {
            incidents.push(Incident {
                id: Uuid::new_v4(),
                kind: IncidentKind::MemoryPressure,
                start_time: recent[0].captured_at,
                end_time: None,
                severity: if memory_load >= self.memory_critical_percent {
                    Severity::Critical
                } else {
                    Severity::Warning
                },
                confidence: 0.8,
                causality: Causality::Correlation,
                summary: format!("Physical memory load reached {memory_load}%"),
                evidence: vec![Evidence {
                    metric: "memory.memory_load_percent".to_owned(),
                    observation: format!("peak load: {memory_load}%"),
                    direction: EvidenceDirection::Supports,
                    weight: 0.7,
                }],
                recommended_tests: vec![
                    "Record private-byte growth across a repeatable session".to_owned(),
                ],
            });
        }
        incidents
    }
}

#[must_use]
pub fn weighted_confidence(weights: &[f64]) -> f64 {
    (1.0 - weights
        .iter()
        .map(|weight| 1.0 - weight.clamp(0.0, 1.0))
        .product::<f64>())
    .clamp(0.0, 0.99)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_core::metrics::{CpuMetrics, MemoryMetrics};

    use super::*;

    fn sample(sequence: u64, cpu: f64, memory: u32) -> MetricSnapshot {
        MetricSnapshot {
            sequence,
            captured_at: Utc::now() + chrono::Duration::seconds(sequence as i64),
            collection_duration_us: 1,
            sampling_interval_ms: 1_000,
            dropped_samples: 0,
            cpu: CpuMetrics {
                total_percent: Some(cpu),
                logical_processor_count: 8,
            },
            memory: MemoryMetrics {
                total_physical_bytes: 100,
                available_physical_bytes: 10,
                committed_bytes: 90,
                commit_limit_bytes: 100,
                memory_load_percent: memory,
            },
            processes: Vec::new(),
            capabilities: BTreeMap::new(),
        }
    }

    #[test]
    fn sustained_cpu_is_correlation_not_confirmed_cause() {
        let incidents = RuleEngine::default().evaluate(&[
            sample(1, 95.0, 10),
            sample(2, 96.0, 10),
            sample(3, 97.0, 10),
        ]);
        assert_eq!(incidents.len(), 1);
        assert_eq!(incidents[0].kind, IncidentKind::CpuSaturation);
        assert_eq!(incidents[0].causality, Causality::Correlation);
    }
}
