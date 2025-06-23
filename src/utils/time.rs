use chrono::{DateTime, Utc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// High-precision timestamp for latency measurements
#[derive(Debug, Clone, Copy)]
pub struct PrecisionTimestamp {
    instant: Instant,
    system_time: SystemTime,
}

impl PrecisionTimestamp {
    /// Create a new timestamp
    pub fn now() -> Self {
        Self {
            instant: Instant::now(),
            system_time: SystemTime::now(),
        }
    }

    /// Get duration since this timestamp
    pub fn elapsed(&self) -> Duration {
        self.instant.elapsed()
    }

    /// Get duration between two timestamps
    pub fn duration_since(&self, earlier: &PrecisionTimestamp) -> Duration {
        self.instant.duration_since(earlier.instant)
    }

    /// Convert to UTC DateTime
    pub fn to_utc(&self) -> DateTime<Utc> {
        DateTime::from(self.system_time)
    }

    /// Get nanoseconds since Unix epoch
    pub fn nanos_since_epoch(&self) -> u64 {
        self.system_time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
}

/// Timer for measuring operation latency
pub struct LatencyTimer {
    start: Instant,
}

impl LatencyTimer {
    /// Start a new timer
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Stop the timer and return elapsed duration
    pub fn stop(self) -> Duration {
        self.start.elapsed()
    }

    /// Get elapsed time without stopping the timer
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Clock synchronization utilities
pub struct Clock;

impl Clock {
    /// Get current time with high precision
    pub fn now() -> PrecisionTimestamp {
        PrecisionTimestamp::now()
    }

    /// Get nanoseconds since Unix epoch
    pub fn nanos() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }

    /// Get microseconds since Unix epoch
    pub fn micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    /// Get milliseconds since Unix epoch
    pub fn millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_precision_timestamp() {
        let ts1 = PrecisionTimestamp::now();
        thread::sleep(Duration::from_millis(1));
        let ts2 = PrecisionTimestamp::now();

        assert!(ts2.duration_since(&ts1) >= Duration::from_millis(1));
        assert!(ts1.elapsed() >= Duration::from_millis(1));
    }

    #[test]
    fn test_latency_timer() {
        let timer = LatencyTimer::start();
        thread::sleep(Duration::from_millis(1));
        let elapsed = timer.stop();

        assert!(elapsed >= Duration::from_millis(1));
    }

    #[test]
    fn test_clock() {
        let nanos1 = Clock::nanos();
        thread::sleep(Duration::from_millis(1));
        let nanos2 = Clock::nanos();

        assert!(nanos2 > nanos1);
        assert!(Clock::micros() > 0);
        assert!(Clock::millis() > 0);
    }
}
