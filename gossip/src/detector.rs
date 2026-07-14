use std::time::{Duration, Instant};

const MAX_SAMPLES: usize = 100;
const MIN_SAMPLES: usize = 5;
const MIN_STD_DEV: f64 = 0.001; // 1 ms minimum standard deviation to avoid div/0

/// A Phi-Accrual Failure Detector implementation.
/// `#[derive(Debug, Clone)]` automatically generates code to allow printing this struct (Debug)
/// and duplicating it in memory (Clone).
#[derive(Debug, Clone)]
pub struct PhiAccrualDetector {
    // A Vec (growable array) holding the durations between consecutive heartbeats.
    intervals: Vec<Duration>,
    
    // `Option` in Rust means this value can either be `Some(Instant)` or `None`.
    // It's Rust's safe alternative to null pointers.
    last_arrival: Option<Instant>,
}

impl PhiAccrualDetector {
    pub fn new() -> Self {
        Self {
            // Pre-allocate space for MAX_SAMPLES to avoid memory re-allocations
            intervals: Vec::with_capacity(MAX_SAMPLES),
            last_arrival: None,
        }
    }

    /// Records a heartbeat arrival at the given time.
    pub fn heartbeat_received(&mut self, now: Instant) {
        if let Some(last) = self.last_arrival {
            if now > last {
                let interval = now.duration_since(last);
                if self.intervals.len() == MAX_SAMPLES {
                    self.intervals.remove(0); // Slide window
                }
                self.intervals.push(interval);
            }
        }
        self.last_arrival = Some(now);
    }

    /// Calculates the current Phi value. Higher value = higher suspicion.
    pub fn phi(&self, now: Instant) -> f64 {
        if self.intervals.len() < MIN_SAMPLES {
            // Not enough samples, return 0.0 (trust the node)
            return 0.0;
        }

        // `let Some(last) = ... else` is a clean way to exit early if `last_arrival` is `None`.
        // If it is `Some`, the inner value is extracted into the variable `last`.
        let Some(last) = self.last_arrival else {
            return 0.0;
        };

        if now < last {
            return 0.0;
        }

        // `.as_secs_f64()` converts the strongly-typed `Duration` into a raw 64-bit float representing seconds.
        let elapsed = now.duration_since(last).as_secs_f64();
        
        let mut sum = 0.0;
        for i in &self.intervals {
            sum += i.as_secs_f64();
        }
        let mean = sum / self.intervals.len() as f64;

        let mut variance_sum = 0.0;
        for i in &self.intervals {
            let diff = i.as_secs_f64() - mean;
            variance_sum += diff * diff;
        }
        let variance = variance_sum / self.intervals.len() as f64;
        let std_dev = variance.sqrt().max(MIN_STD_DEV);

        // Calculate probability of interval being larger than `elapsed`
        let y = (elapsed - mean) / std_dev;
        let p_later = 1.0 - normal_cdf(y);

        // Phi = -log10(p_later)
        let p_later = p_later.max(1e-100); // Prevent log(0) or negative
        -p_later.log10()
    }
}

/// Approximate the Cumulative Distribution Function (CDF) for standard normal distribution.
/// Using Abramowitz and Stegun formula 7.1.26
fn normal_cdf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x_abs = x.abs() / std::f64::consts::SQRT_2;

    let p = 0.3275911;
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;

    let t = 1.0 / (1.0 + p * x_abs);
    let erf = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x_abs * x_abs).exp();
    
    0.5 * (1.0 + sign * erf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phi_accrual_basic() {
        let mut detector = PhiAccrualDetector::new();
        let mut now = Instant::now();
        
        // Feed 10 heartbeats 200ms apart to build a stable baseline
        for _ in 0..10 {
            detector.heartbeat_received(now);
            now += Duration::from_millis(200);
        }

        // Check phi exactly at the expected 200ms mark
        let phi_normal = detector.phi(now);
        assert!(phi_normal < 1.0, "Phi should be low for expected arrival time (was {})", phi_normal);

        // Check phi after a massive delay (e.g. 2000ms instead of 200ms)
        let phi_delayed = detector.phi(now + Duration::from_millis(2000));
        assert!(phi_delayed > 8.0, "Phi should be very high for massive delay (was {})", phi_delayed);
    }
}
