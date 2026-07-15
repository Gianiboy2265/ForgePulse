use std::time::Duration;

use forge_core::{Result, config::MonitoringConfig, metrics::MetricSnapshot};
use serde::{Deserialize, Serialize};

pub trait Collector: Send {
    fn name(&self) -> &'static str;
    fn collect(&mut self, context: SampleContext) -> Result<MetricSnapshot>;
}

#[derive(Debug, Clone, Copy)]
pub struct SampleContext {
    pub sequence: u64,
    pub interval: Duration,
    pub dropped_samples: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SamplingMode {
    Idle,
    Normal,
    Incident,
    Gaming,
    Benchmark,
}

#[derive(Debug, Clone, Copy)]
pub struct SamplingSignals {
    pub idle: bool,
    pub incident_active: bool,
    pub game_active: bool,
    pub benchmark_active: bool,
    pub measured_overhead_percent: f64,
}

#[derive(Debug, Clone)]
pub struct AdaptiveSampler {
    config: MonitoringConfig,
    mode: SamplingMode,
    pressure_backoff: u32,
}

impl AdaptiveSampler {
    #[must_use]
    pub fn new(config: MonitoringConfig) -> Self {
        Self {
            config,
            mode: SamplingMode::Normal,
            pressure_backoff: 0,
        }
    }

    pub fn update(&mut self, signals: SamplingSignals) -> Duration {
        self.mode = if signals.benchmark_active {
            SamplingMode::Benchmark
        } else if signals.game_active {
            SamplingMode::Gaming
        } else if signals.incident_active {
            SamplingMode::Incident
        } else if signals.idle {
            SamplingMode::Idle
        } else {
            SamplingMode::Normal
        };

        if signals.measured_overhead_percent > self.config.max_cpu_overhead_percent {
            self.pressure_backoff = self.pressure_backoff.saturating_add(1).min(5);
        } else {
            self.pressure_backoff = self.pressure_backoff.saturating_sub(1);
        }

        let base_ms = match self.mode {
            SamplingMode::Idle => self.config.idle_interval_ms,
            SamplingMode::Normal => self.config.normal_interval_ms,
            SamplingMode::Incident => self.config.incident_interval_ms,
            SamplingMode::Gaming => self.config.gaming_interval_ms,
            SamplingMode::Benchmark => self.config.benchmark_interval_ms,
        };
        let multiplier = 1_u64 << self.pressure_backoff;
        Duration::from_millis(
            base_ms
                .saturating_mul(multiplier)
                .max(self.config.minimum_interval_ms),
        )
    }

    #[must_use]
    pub const fn mode(&self) -> SamplingMode {
        self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_has_precedence() {
        let mut sampler = AdaptiveSampler::new(MonitoringConfig::default());
        let duration = sampler.update(SamplingSignals {
            idle: true,
            incident_active: true,
            game_active: true,
            benchmark_active: true,
            measured_overhead_percent: 0.0,
        });
        assert_eq!(sampler.mode(), SamplingMode::Benchmark);
        assert_eq!(duration, Duration::from_millis(100));
    }

    #[test]
    fn excessive_overhead_backs_off() {
        let config = MonitoringConfig::default();
        let mut sampler = AdaptiveSampler::new(config.clone());
        let duration = sampler.update(SamplingSignals {
            idle: false,
            incident_active: false,
            game_active: false,
            benchmark_active: false,
            measured_overhead_percent: config.max_cpu_overhead_percent + 1.0,
        });
        assert_eq!(
            duration,
            Duration::from_millis(config.normal_interval_ms * 2)
        );
    }
}
