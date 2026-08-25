use std::time::Duration;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LatencyStats {
    pub samples: usize,
    pub min_us: f64,
    pub max_us: f64,
    pub mean_us: f64,
    pub median_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub std_dev_us: f64,
    pub ops_per_sec: f64,
    pub primitives_per_sec: f64,
}

impl LatencyStats {
    pub fn from_durations(mut durations: Vec<Duration>, primitives_per_run: usize) -> Self {
        if durations.is_empty() {
            return Self::empty();
        }

        durations.sort();

        let count = durations.len();
        let us_values: Vec<f64> = durations
            .iter()
            .map(|d| d.as_secs_f64() * 1_000_000.0)
            .collect();

        let min_us = us_values[0];
        let max_us = us_values[count - 1];
        let sum_us: f64 = us_values.iter().sum();
        let mean_us = sum_us / (count as f64);

        let median_us = if count % 2 == 0 {
            (us_values[count / 2 - 1] + us_values[count / 2]) / 2.0
        } else {
            us_values[count / 2]
        };

        let p95_idx = ((count as f64) * 0.95).floor() as usize;
        let p95_us = us_values[p95_idx.min(count - 1)];

        let p99_idx = ((count as f64) * 0.99).floor() as usize;
        let p99_us = us_values[p99_idx.min(count - 1)];

        let variance: f64 = us_values
            .iter()
            .map(|&x| (x - mean_us).powi(2))
            .sum::<f64>()
            / (count as f64);
        let std_dev_us = variance.sqrt();

        let mean_sec = mean_us / 1_000_000.0;
        let ops_per_sec = if mean_sec > 0.0 { 1.0 / mean_sec } else { 0.0 };
        let primitives_per_sec = ops_per_sec * (primitives_per_run as f64);

        Self {
            samples: count,
            min_us,
            max_us,
            mean_us,
            median_us,
            p95_us,
            p99_us,
            std_dev_us,
            ops_per_sec,
            primitives_per_sec,
        }
    }

    pub fn empty() -> Self {
        Self {
            samples: 0,
            min_us: 0.0,
            max_us: 0.0,
            mean_us: 0.0,
            median_us: 0.0,
            p95_us: 0.0,
            p99_us: 0.0,
            std_dev_us: 0.0,
            ops_per_sec: 0.0,
            primitives_per_sec: 0.0,
        }
    }
}