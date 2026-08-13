use std::time::Duration;

/// Collects frame durations and reports percentiles.
///
/// Stores every sample rather than bucketing. A spike run is short and this
/// keeps the percentile math exact.
#[derive(Default)]
pub struct FrameTimer {
    samples: Vec<u128>,
}

impl FrameTimer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, d: Duration) {
        self.samples.push(d.as_micros());
    }

    pub fn report(&self) -> String {
        if self.samples.is_empty() {
            return "n=0".to_string();
        }
        let mut s = self.samples.clone();
        s.sort_unstable();
        let n = s.len();
        let mean = s.iter().sum::<u128>() / n as u128;
        let pick = |q: f64| s[((n as f64 * q) as usize).min(n - 1)];
        // Which frame was the worst matters as much as how bad it was: frame 0
        // is the startup paint and says nothing about scrolling.
        let max_at = self
            .samples
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| **v)
            .map_or(0, |(i, _)| i);
        format!(
            "n={} mean={}us p50={}us p99={}us max={}us max_at_frame={} first={}us",
            n,
            mean,
            pick(0.50),
            pick(0.99),
            s[n - 1],
            max_at,
            self.samples[0]
        )
    }
}
