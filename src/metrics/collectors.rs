use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

/// Collects and aggregates latency statistics
#[derive(Debug)]
pub struct LatencyCollector {
    samples: Vec<Duration>,
    last_collection: Instant,
    collection_interval: Duration,
}

impl LatencyCollector {
    pub fn new(collection_interval: Duration) -> Self {
        Self {
            samples: Vec::new(),
            last_collection: Instant::now(),
            collection_interval,
        }
    }

    /// Add a latency sample
    pub fn record(&mut self, latency: Duration) {
        self.samples.push(latency);
    }

    /// Collect and reset statistics if interval has passed
    pub fn collect(&mut self) -> Option<LatencyStatistics> {
        if self.last_collection.elapsed() >= self.collection_interval {
            let stats = self.calculate_stats();
            self.samples.clear();
            self.last_collection = Instant::now();
            Some(stats)
        } else {
            None
        }
    }

    fn calculate_stats(&self) -> LatencyStatistics {
        if self.samples.is_empty() {
            return LatencyStatistics::default();
        }

        let mut sorted_samples = self.samples.clone();
        sorted_samples.sort();

        let len = sorted_samples.len();
        let min = sorted_samples[0];
        let max = sorted_samples[len - 1];
        let p50 = sorted_samples[len / 2];
        let p95 = sorted_samples[(len as f64 * 0.95) as usize];
        let p99 = sorted_samples[(len as f64 * 0.99) as usize];
        let p999 = sorted_samples[(len as f64 * 0.999) as usize];

        let total: Duration = sorted_samples.iter().sum();
        let mean = total / len as u32;

        LatencyStatistics {
            count: len as u64,
            min,
            max,
            mean,
            p50,
            p95,
            p99,
            p999,
        }
    }
}

/// Throughput collector for counting operations per second
#[derive(Debug)]
pub struct ThroughputCollector {
    counter: AtomicU64,
    last_collection: Instant,
    collection_interval: Duration,
    last_count: u64,
}

impl ThroughputCollector {
    pub fn new(collection_interval: Duration) -> Self {
        Self {
            counter: AtomicU64::new(0),
            last_collection: Instant::now(),
            collection_interval,
            last_count: 0,
        }
    }

    /// Increment the counter
    pub fn increment(&self) {
        self.counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment by a specific amount
    pub fn add(&self, value: u64) {
        self.counter.fetch_add(value, Ordering::Relaxed);
    }

    /// Collect throughput statistics
    pub fn collect(&mut self) -> Option<ThroughputStatistics> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_collection);

        if elapsed >= self.collection_interval {
            let current_count = self.counter.load(Ordering::Relaxed);
            let operations = current_count - self.last_count;
            let rate = operations as f64 / elapsed.as_secs_f64();

            self.last_collection = now;
            self.last_count = current_count;

            Some(ThroughputStatistics {
                operations,
                rate,
                total: current_count,
                interval: elapsed,
            })
        } else {
            None
        }
    }

    /// Get current total count
    pub fn total(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }
}

/// System resource collector
#[derive(Debug)]
pub struct ResourceCollector {
    last_collection: Instant,
    collection_interval: Duration,
}

impl ResourceCollector {
    pub fn new(collection_interval: Duration) -> Self {
        Self {
            last_collection: Instant::now(),
            collection_interval,
        }
    }

    /// Collect system resource statistics
    pub fn collect(&mut self) -> Option<ResourceStatistics> {
        if self.last_collection.elapsed() >= self.collection_interval {
            self.last_collection = Instant::now();
            Some(self.get_resource_stats())
        } else {
            None
        }
    }

    fn get_resource_stats(&self) -> ResourceStatistics {
        // In a real implementation, you would use system APIs
        // For now, we'll return placeholder values
        ResourceStatistics {
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
            memory_available_bytes: 0,
            open_file_descriptors: 0,
            network_connections: 0,
        }
    }
}

/// Aggregated latency statistics
#[derive(Debug, Clone, Default)]
pub struct LatencyStatistics {
    pub count: u64,
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub p999: Duration,
}

impl LatencyStatistics {
    /// Convert to microseconds for easier reading
    pub fn to_micros(&self) -> LatencyMicros {
        LatencyMicros {
            count: self.count,
            min: self.min.as_micros() as f64,
            max: self.max.as_micros() as f64,
            mean: self.mean.as_micros() as f64,
            p50: self.p50.as_micros() as f64,
            p95: self.p95.as_micros() as f64,
            p99: self.p99.as_micros() as f64,
            p999: self.p999.as_micros() as f64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LatencyMicros {
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p999: f64,
}

/// Throughput statistics
#[derive(Debug, Clone)]
pub struct ThroughputStatistics {
    pub operations: u64,
    pub rate: f64,
    pub total: u64,
    pub interval: Duration,
}

/// System resource statistics
#[derive(Debug, Clone)]
pub struct ResourceStatistics {
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_available_bytes: u64,
    pub open_file_descriptors: u32,
    pub network_connections: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_latency_collector() {
        let mut collector = LatencyCollector::new(Duration::from_millis(100));

        collector.record(Duration::from_micros(100));
        collector.record(Duration::from_micros(200));
        collector.record(Duration::from_micros(300));

        // Should not collect yet
        assert!(collector.collect().is_none());

        thread::sleep(Duration::from_millis(101));

        // Should collect now
        let stats = collector.collect().unwrap();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min, Duration::from_micros(100));
        assert_eq!(stats.max, Duration::from_micros(300));
    }

    #[test]
    fn test_throughput_collector() {
        let mut collector = ThroughputCollector::new(Duration::from_millis(100));

        collector.increment();
        collector.add(5);

        assert_eq!(collector.total(), 6);

        // Should not collect yet
        assert!(collector.collect().is_none());

        thread::sleep(Duration::from_millis(101));

        // Should collect now
        let stats = collector.collect().unwrap();
        assert_eq!(stats.operations, 6);
        assert!(stats.rate > 0.0);
    }
}
