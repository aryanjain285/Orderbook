// use serde_json::json;
// use std::sync::Arc;
// use std::time::{Duration, SystemTime, UNIX_EPOCH};
// use tokio::time::interval;
// use tracing::{error, info, warn};

// use super::collectors::{LatencyStatistics, ResourceStatistics, ThroughputStatistics};

// /// InfluxDB exporter for time-series metrics
// pub struct InfluxDBExporter {
//     client: Option<influxdb2::Client>,
//     bucket: String,
//     org: String,
//     enabled: bool,
// }

// impl InfluxDBExporter {
//     pub fn new(url: &str, token: &str, bucket: String, org: String) -> Self {
//         match influxdb2::Client::new(url, token) {
//             client => {
//                 info!("InfluxDB exporter initialized for bucket: {}", bucket);
//                 Self {
//                     client: Some(client),
//                     bucket,
//                     org,
//                     enabled: true,
//                 }
//             }
//         }
//     }

//     pub fn disabled() -> Self {
//         Self {
//             client: None,
//             bucket: String::new(),
//             org: String::new(),
//             enabled: false,
//         }
//     }

//     /// Export latency statistics
//     pub async fn export_latency(&self, measurement: &str, symbol: &str, stats: &LatencyStatistics) {
//         if !self.enabled || self.client.is_none() {
//             return;
//         }

//         let timestamp = SystemTime::now()
//             .duration_since(UNIX_EPOCH)
//             .unwrap()
//             .as_nanos() as i64;

//         let micros = stats.to_micros();

//         // Create data points for each percentile
//         let points = vec![
//             format!(
//                 "{},symbol={},metric=count value={}i {}",
//                 measurement, symbol, micros.count, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=min value={} {}",
//                 measurement, symbol, micros.min, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=max value={} {}",
//                 measurement, symbol, micros.max, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=mean value={} {}",
//                 measurement, symbol, micros.mean, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=p50 value={} {}",
//                 measurement, symbol, micros.p50, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=p95 value={} {}",
//                 measurement, symbol, micros.p95, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=p99 value={} {}",
//                 measurement, symbol, micros.p99, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=p999 value={} {}",
//                 measurement, symbol, micros.p999, timestamp
//             ),
//         ];

//         if let Some(client) = &self.client {
//             for point in points {
//                 if let Err(e) = client.write(&self.bucket, &self.org, &point).await {
//                     error!("Failed to write latency metrics to InfluxDB: {}", e);
//                 }
//             }
//         }
//     }

//     /// Export throughput statistics
//     pub async fn export_throughput(
//         &self,
//         measurement: &str,
//         symbol: &str,
//         stats: &ThroughputStatistics,
//     ) {
//         if !self.enabled || self.client.is_none() {
//             return;
//         }

//         let timestamp = SystemTime::now()
//             .duration_since(UNIX_EPOCH)
//             .unwrap()
//             .as_nanos() as i64;

//         let points = vec![
//             format!(
//                 "{},symbol={},metric=operations value={}i {}",
//                 measurement, symbol, stats.operations, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=rate value={} {}",
//                 measurement, symbol, stats.rate, timestamp
//             ),
//             format!(
//                 "{},symbol={},metric=total value={}i {}",
//                 measurement, symbol, stats.total, timestamp
//             ),
//         ];

//         if let Some(client) = &self.client {
//             for point in points {
//                 if let Err(e) = client.write(&self.bucket, &self.org, &point).await {
//                     error!("Failed to write throughput metrics to InfluxDB: {}", e);
//                 }
//             }
//         }
//     }

//     /// Export resource statistics
//     pub async fn export_resources(&self, measurement: &str, stats: &ResourceStatistics) {
//         if !self.enabled || self.client.is_none() {
//             return;
//         }

//         let timestamp = SystemTime::now()
//             .duration_since(UNIX_EPOCH)
//             .unwrap()
//             .as_nanos() as i64;

//         let points = vec![
//             format!(
//                 "{},metric=cpu_usage value={} {}",
//                 measurement, stats.cpu_usage_percent, timestamp
//             ),
//             format!(
//                 "{},metric=memory_usage value={}i {}",
//                 measurement, stats.memory_usage_bytes, timestamp
//             ),
//             format!(
//                 "{},metric=memory_available value={}i {}",
//                 measurement, stats.memory_available_bytes, timestamp
//             ),
//             format!(
//                 "{},metric=file_descriptors value={}i {}",
//                 measurement, stats.open_file_descriptors, timestamp
//             ),
//             format!(
//                 "{},metric=network_connections value={}i {}",
//                 measurement, stats.network_connections, timestamp
//             ),
//         ];

//         if let Some(client) = &self.client {
//             for point in points {
//                 if let Err(e) = client.write(&self.bucket, &self.org, &point).await {
//                     error!("Failed to write resource metrics to InfluxDB: {}", e);
//                 }
//             }
//         }
//     }
// }

// /// Console exporter for development and debugging
// pub struct ConsoleExporter {
//     enabled: bool,
// }

// impl ConsoleExporter {
//     pub fn new() -> Self {
//         Self { enabled: true }
//     }

//     pub fn disabled() -> Self {
//         Self { enabled: false }
//     }

//     /// Export latency statistics to console
//     pub fn export_latency(&self, operation: &str, symbol: &str, stats: &LatencyStatistics) {
//         if !self.enabled {
//             return;
//         }

//         let micros = stats.to_micros();
//         info!(
//             "📊 {} {} Latency | Count: {} | Min: {:.2}μs | P50: {:.2}μs | P95: {:.2}μs | P99: {:.2}μs | Max: {:.2}μs",
//             operation,
//             symbol,
//             micros.count,
//             micros.min,
//             micros.p50,
//             micros.p95,
//             micros.p99,
//             micros.max
//         );
//     }

//     /// Export throughput statistics to console
//     pub fn export_throughput(&self, operation: &str, symbol: &str, stats: &ThroughputStatistics) {
//         if !self.enabled {
//             return;
//         }

//         info!(
//             "⚡ {} {} Throughput | Operations: {} | Rate: {:.2}/sec | Total: {}",
//             operation, symbol, stats.operations, stats.rate, stats.total
//         );
//     }

//     /// Export resource statistics to console
//     pub fn export_resources(&self, stats: &ResourceStatistics) {
//         if !self.enabled {
//             return;
//         }

//         info!(
//             "💻 System Resources | CPU: {:.1}% | Memory: {} MB | FDs: {} | Connections: {}",
//             stats.cpu_usage_percent,
//             stats.memory_usage_bytes / 1024 / 1024,
//             stats.open_file_descriptors,
//             stats.network_connections
//         );
//     }
// }

// impl Default for ConsoleExporter {
//     fn default() -> Self {
//         Self::new()
//     }
// }

// /// JSON file exporter for persistent storage
// pub struct FileExporter {
//     file_path: String,
//     enabled: bool,
// }

// impl FileExporter {
//     pub fn new(file_path: String) -> Self {
//         Self {
//             file_path,
//             enabled: true,
//         }
//     }

//     pub fn disabled() -> Self {
//         Self {
//             file_path: String::new(),
//             enabled: false,
//         }
//     }

//     /// Export metrics to JSON file
//     pub async fn export_metrics(&self, metrics: &MetricsSnapshot) {
//         if !self.enabled {
//             return;
//         }

//         let json_data = match serde_json::to_string_pretty(metrics) {
//             Ok(data) => data,
//             Err(e) => {
//                 error!("Failed to serialize metrics: {}", e);
//                 return;
//             }
//         };

//         if let Err(e) = tokio::fs::write(&self.file_path, json_data).await {
//             error!("Failed to write metrics to file {}: {}", self.file_path, e);
//         }
//     }
// }

// /// Combined metrics exporter
// pub struct MetricsExporter {
//     influxdb: InfluxDBExporter,
//     console: ConsoleExporter,
//     file: FileExporter,
// }

// impl MetricsExporter {
//     pub fn new(influxdb: InfluxDBExporter, console: ConsoleExporter, file: FileExporter) -> Self {
//         Self {
//             influxdb,
//             console,
//             file,
//         }
//     }

//     /// Export all metrics to all configured exporters
//     pub async fn export_all(&self, snapshot: &MetricsSnapshot) {
//         // Export to InfluxDB
//         for (symbol, latency) in &snapshot.latency_stats {
//             self.influxdb
//                 .export_latency("orderbook_latency", symbol, latency)
//                 .await;
//         }

//         for (symbol, throughput) in &snapshot.throughput_stats {
//             self.influxdb
//                 .export_throughput("orderbook_throughput", symbol, throughput)
//                 .await;
//         }

//         self.influxdb
//             .export_resources("system_resources", &snapshot.resource_stats)
//             .await;

//         // Export to console
//         for (symbol, latency) in &snapshot.latency_stats {
//             self.console.export_latency("OrderBook", symbol, latency);
//         }

//         for (symbol, throughput) in &snapshot.throughput_stats {
//             self.console
//                 .export_throughput("OrderBook", symbol, throughput);
//         }

//         self.console.export_resources(&snapshot.resource_stats);

//         // Export to file
//         self.file.export_metrics(snapshot).await;
//     }
// }

// /// Snapshot of all metrics at a point in time

// pub struct MetricsSnapshot {
//     pub timestamp: u64,
//     pub latency_stats: std::collections::HashMap<String, LatencyStatistics>,
//     pub throughput_stats: std::collections::HashMap<String, ThroughputStatistics>,
//     pub resource_stats: ResourceStatistics,
// }

// impl MetricsSnapshot {
//     pub fn new() -> Self {
//         Self {
//             timestamp: SystemTime::now()
//                 .duration_since(UNIX_EPOCH)
//                 .unwrap()
//                 .as_secs(),
//             latency_stats: std::collections::HashMap::new(),
//             throughput_stats: std::collections::HashMap::new(),
//             resource_stats: ResourceStatistics {
//                 cpu_usage_percent: 0.0,
//                 memory_usage_bytes: 0,
//                 memory_available_bytes: 0,
//                 open_file_descriptors: 0,
//                 network_connections: 0,
//             },
//         }
//     }
// }

// impl Default for MetricsSnapshot {
//     fn default() -> Self {
//         Self::new()
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_console_exporter() {
//         let exporter = ConsoleExporter::new();
//         let stats = LatencyStatistics::default();

//         // Should not panic
//         exporter.export_latency("test", "AAPL", &stats);
//     }

//     #[test]
//     fn test_metrics_snapshot() {
//         let snapshot = MetricsSnapshot::new();
//         assert!(snapshot.timestamp > 0);
//         assert!(snapshot.latency_stats.is_empty());
//         assert!(snapshot.throughput_stats.is_empty());
//     }
// }
