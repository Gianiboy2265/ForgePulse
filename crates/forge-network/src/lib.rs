use std::net::IpAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatencyTest {
    pub target: DiagnosticTarget,
    pub attempts: u16,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DiagnosticTarget {
    DefaultGateway,
    Hostname(String),
    Address(IpAddr),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LatencySummary {
    pub sent: usize,
    pub received: usize,
    pub loss_percent: f64,
    pub minimum_ms: Option<f64>,
    pub median_ms: Option<f64>,
    pub maximum_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
}

impl LatencyTest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(3..=1_000).contains(&self.attempts) {
            return Err("attempts must be between 3 and 1000");
        }
        if !(50..=60_000).contains(&self.timeout_ms) {
            return Err("timeout_ms must be between 50 and 60000");
        }
        if matches!(&self.target, DiagnosticTarget::Hostname(name) if name.is_empty() || name.len() > 253)
        {
            return Err("hostname length is invalid");
        }
        Ok(())
    }
}

#[must_use]
pub fn summarize_latency(attempts: &[Option<f64>]) -> LatencySummary {
    let mut received: Vec<f64> = attempts
        .iter()
        .filter_map(|value| value.filter(|latency| latency.is_finite() && *latency >= 0.0))
        .collect();
    received.sort_by(f64::total_cmp);
    let sent = attempts.len();
    let received_count = received.len();
    let loss_percent = if sent == 0 {
        0.0
    } else {
        ((sent - received_count) as f64 / sent as f64) * 100.0
    };
    let median_ms = if received.is_empty() {
        None
    } else {
        Some(received[received.len() / 2])
    };
    let jitter_ms = if received.len() < 2 {
        None
    } else {
        let total: f64 = received
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .sum();
        Some(total / (received.len() - 1) as f64)
    };
    LatencySummary {
        sent,
        received: received_count,
        loss_percent,
        minimum_ms: received.first().copied(),
        median_ms,
        maximum_ms: received.last().copied(),
        jitter_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_summary_preserves_loss() {
        let result = summarize_latency(&[Some(5.0), None, Some(7.0)]);
        assert_eq!(result.received, 2);
        assert!((result.loss_percent - 100.0 / 3.0).abs() < 0.001);
    }
}
