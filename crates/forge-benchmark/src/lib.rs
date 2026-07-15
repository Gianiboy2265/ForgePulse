use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SampleStatistics {
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub standard_deviation: f64,
    pub p01: f64,
    pub p99: f64,
    pub confidence_interval_95: (f64, f64),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonVerdict {
    MeasurableImprovement,
    LikelyImprovement,
    Inconclusive,
    LikelyRegression,
    MeasurableRegression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Comparison {
    pub control: SampleStatistics,
    pub test: SampleStatistics,
    pub absolute_change: f64,
    pub relative_change_percent: Option<f64>,
    pub cohens_d: Option<f64>,
    pub confidence: f64,
    pub verdict: ComparisonVerdict,
    pub minimum_sample_size_met: bool,
}

impl SampleStatistics {
    pub fn calculate(samples: &[f64]) -> Result<Self> {
        let mut values: Vec<f64> = samples
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect();
        if values.is_empty() {
            return Err(ForgeError::InvalidInput(
                "statistics require at least one finite sample".to_owned(),
            ));
        }
        values.sort_by(f64::total_cmp);
        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;
        let variance = if count > 1 {
            values
                .iter()
                .map(|value| (value - mean).powi(2))
                .sum::<f64>()
                / (count - 1) as f64
        } else {
            0.0
        };
        let standard_deviation = variance.sqrt();
        let margin = if count > 1 {
            1.96 * standard_deviation / (count as f64).sqrt()
        } else {
            0.0
        };
        Ok(Self {
            count,
            mean,
            median: percentile_sorted(&values, 0.5),
            minimum: values[0],
            maximum: values[count - 1],
            standard_deviation,
            p01: percentile_sorted(&values, 0.01),
            p99: percentile_sorted(&values, 0.99),
            confidence_interval_95: (mean - margin, mean + margin),
        })
    }
}

pub fn compare(
    control_samples: &[f64],
    test_samples: &[f64],
    lower_is_better: bool,
    minimum_sample_size: usize,
) -> Result<Comparison> {
    let control = SampleStatistics::calculate(control_samples)?;
    let test = SampleStatistics::calculate(test_samples)?;
    let absolute_change = test.mean - control.mean;
    let relative_change_percent = if control.mean.abs() <= f64::EPSILON {
        None
    } else {
        Some((absolute_change / control.mean) * 100.0)
    };
    let pooled_variance = if control.count + test.count > 2 {
        (((control.count - 1) as f64 * control.standard_deviation.powi(2))
            + ((test.count - 1) as f64 * test.standard_deviation.powi(2)))
            / (control.count + test.count - 2) as f64
    } else {
        0.0
    };
    let cohens_d =
        (pooled_variance > f64::EPSILON).then(|| absolute_change / pooled_variance.sqrt());
    let standard_error = (control.standard_deviation.powi(2) / control.count as f64
        + test.standard_deviation.powi(2) / test.count as f64)
        .sqrt();
    let z_score = if standard_error > f64::EPSILON {
        absolute_change.abs() / standard_error
    } else if absolute_change.abs() > f64::EPSILON {
        8.0
    } else {
        0.0
    };
    let confidence = normal_two_sided_confidence(z_score);
    let minimum_sample_size_met =
        control.count >= minimum_sample_size && test.count >= minimum_sample_size;
    let beneficial = if lower_is_better {
        absolute_change < 0.0
    } else {
        absolute_change > 0.0
    };
    let verdict = if !minimum_sample_size_met || confidence < 0.75 {
        ComparisonVerdict::Inconclusive
    } else if confidence >= 0.95 && beneficial {
        ComparisonVerdict::MeasurableImprovement
    } else if beneficial {
        ComparisonVerdict::LikelyImprovement
    } else if confidence >= 0.95 {
        ComparisonVerdict::MeasurableRegression
    } else {
        ComparisonVerdict::LikelyRegression
    };
    Ok(Comparison {
        control,
        test,
        absolute_change,
        relative_change_percent,
        cohens_d,
        confidence,
        verdict,
        minimum_sample_size_met,
    })
}

fn percentile_sorted(values: &[f64], percentile: f64) -> f64 {
    if values.len() == 1 {
        return values[0];
    }
    let position = percentile * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    values[lower] + (values[upper] - values[lower]) * fraction
}

// Abramowitz-Stegun 7.1.26 approximation. Adequate for a deterministic display
// confidence; benchmark reports retain the raw samples and assumptions.
fn normal_two_sided_confidence(z: f64) -> f64 {
    let x = z.abs() / 2.0_f64.sqrt();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let polynomial =
        (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t;
    (1.0 - polynomial * (-x * x).exp()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_are_stable() -> Result<()> {
        let stats = SampleStatistics::calculate(&[1.0, 2.0, 3.0, 4.0])?;
        assert_eq!(stats.mean, 2.5);
        assert_eq!(stats.median, 2.5);
        Ok(())
    }

    #[test]
    fn recognizes_clear_improvement() -> Result<()> {
        let control = [10.0, 10.1, 9.9, 10.0, 10.1, 9.9];
        let test = [8.0, 8.1, 7.9, 8.0, 8.1, 7.9];
        let result = compare(&control, &test, true, 6)?;
        assert_eq!(result.verdict, ComparisonVerdict::MeasurableImprovement);
        Ok(())
    }
}
