use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{error, info};

pub mod collectors;
pub mod exporters;

/// Metrics collector for order book operations
#[derive(Debug)]
pub struct OrderBookMetrics {
    // Latency tracking
    add_order_latency: LatencyTracker,
    cancel_order_latency: LatencyTracker,
    modify_order_latency: LatencyTracker,
    match_order_latency: LatencyTracker,

    // Throughput counters
    orders_added: AtomicU64,
    orders_cancelled: AtomicU64,
    orders_modified: AtomicU64,
    trades_executed: AtomicU64,

    // Book state
    total_orders: AtomicU64,
    bid_levels: AtomicU64,
    ask_levels: AtomicU64,

    // Volume tracking
    total_volume: AtomicU64,
    total_notional: AtomicU64,
}

impl OrderBookMetrics {
    pub fn new() -> Self {
        // Register metric descriptions
        describe_counter!("orderbook_orders_total", "Total number of orders processed");
        describe_counter!("orderbook_trades_total", "Total number of trades executed");
        describe_histogram!(
            "orderbook_operation_duration_seconds",
            "Duration of order book operations"
        );
        describe_gauge!(
            "orderbook_levels_total",
            "Number of price levels in the book"
        );
        describe_gauge!(
            "orderbook_orders_current",
            "Current number of orders in the book"
        );
        describe_gauge!("orderbook_spread_ticks", "Current bid-ask spread in ticks");

        Self {
            add_order_latency: LatencyTracker::new("add_order"),
            cancel_order_latency: LatencyTracker::new("cancel_order"),
            modify_order_latency: LatencyTracker::new("modify_order"),
            match_order_latency: LatencyTracker::new("match_order"),
            orders_added: AtomicU64::new(0),
            orders_cancelled: AtomicU64::new(0),
            orders_modified: AtomicU64::new(0),
            trades_executed: AtomicU64::new(0),
            total_orders: AtomicU64::new(0),
            bid_levels: AtomicU64::new(0),
            ask_levels: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
            total_notional: AtomicU64::new(0),
        }
    }

    // Latency measurement methods
    pub fn time_add_order<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.add_order_latency.time(f)
    }

    pub fn time_cancel_order<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.cancel_order_latency.time(f)
    }

    pub fn time_modify_order<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.modify_order_latency.time(f)
    }

    pub fn time_match_order<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.match_order_latency.time(f)
    }

    // Counter methods
    pub fn increment_orders_added(&self) {
        self.orders_added.fetch_add(1, Ordering::Relaxed);
        counter!("orderbook_orders_total", "operation" => "add").increment(1);
    }

    pub fn increment_orders_cancelled(&self) {
        self.orders_cancelled.fetch_add(1, Ordering::Relaxed);
        counter!("orderbook_orders_total", "operation" => "cancel").increment(1);
    }

    pub fn increment_orders_modified(&self) {
        self.orders_modified.fetch_add(1, Ordering::Relaxed);
        counter!("orderbook_orders_total", "operation" => "modify").increment(1);
    }

    pub fn increment_trades_executed(&self, quantity: u64, notional: u64) {
        self.trades_executed.fetch_add(1, Ordering::Relaxed);
        self.total_volume.fetch_add(quantity, Ordering::Relaxed);
        self.total_notional.fetch_add(notional, Ordering::Relaxed);

        counter!("orderbook_trades_total").increment(1);
        counter!("orderbook_volume_total").increment(quantity);
        counter!("orderbook_notional_total").increment(notional);
    }

    // Gauge methods
    pub fn set_total_orders(&self, count: u64) {
        self.total_orders.store(count, Ordering::Relaxed);
        gauge!("orderbook_orders_current").set(count as f64);
    }

    pub fn set_bid_levels(&self, count: u64) {
        self.bid_levels.store(count, Ordering::Relaxed);
        gauge!("orderbook_levels_total", "side" => "bid").set(count as f64);
    }

    pub fn set_ask_levels(&self, count: u64) {
        self.ask_levels.store(count, Ordering::Relaxed);
        gauge!("orderbook_levels_total", "side" => "ask").set(count as f64);
    }

    pub fn set_spread(&self, spread_ticks: u64) {
        gauge!("orderbook_spread_ticks").set(spread_ticks as f64);
    }

    pub fn set_best_bid(&self, price: u64) {
        gauge!("orderbook_best_bid").set(price as f64);
    }

    pub fn set_best_ask(&self, price: u64) {
        gauge!("orderbook_best_ask").set(price as f64);
    }

    // Getters for current values
    pub fn get_orders_added(&self) -> u64 {
        self.orders_added.load(Ordering::Relaxed)
    }

    pub fn get_orders_cancelled(&self) -> u64 {
        self.orders_cancelled.load(Ordering::Relaxed)
    }

    pub fn get_orders_modified(&self) -> u64 {
        self.orders_modified.load(Ordering::Relaxed)
    }

    pub fn get_trades_executed(&self) -> u64 {
        self.trades_executed.load(Ordering::Relaxed)
    }

    pub fn get_total_volume(&self) -> u64 {
        self.total_volume.load(Ordering::Relaxed)
    }

    pub fn get_total_notional(&self) -> u64 {
        self.total_notional.load(Ordering::Relaxed)
    }

    pub fn get_latency_stats(&self) -> LatencyStats {
        LatencyStats {
            add_order: self.add_order_latency.get_stats(),
            cancel_order: self.cancel_order_latency.get_stats(),
            modify_order: self.modify_order_latency.get_stats(),
            match_order: self.match_order_latency.get_stats(),
        }
    }
}

impl Default for OrderBookMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Latency tracker for individual operations
#[derive(Debug)]
struct LatencyTracker {
    operation: String,
    samples: AtomicU64,
    total_nanos: AtomicU64,
    min_nanos: AtomicU64,
    max_nanos: AtomicU64,
}

impl LatencyTracker {
    fn new(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            samples: AtomicU64::new(0),
            total_nanos: AtomicU64::new(0),
            min_nanos: AtomicU64::new(u64::MAX),
            max_nanos: AtomicU64::new(0),
        }
    }

    fn time<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();

        self.record_latency(duration);
        result
    }

    fn record_latency(&self, duration: Duration) {
        let nanos = duration.as_nanos() as u64;

        self.samples.fetch_add(1, Ordering::Relaxed);
        self.total_nanos.fetch_add(nanos, Ordering::Relaxed);

        // Update min (with CAS loop)
        let mut current_min = self.min_nanos.load(Ordering::Relaxed);
        while nanos < current_min {
            match self.min_nanos.compare_exchange_weak(
                current_min,
                nanos,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_min) => current_min = new_min,
            }
        }

        // Update max (with CAS loop)
        let mut current_max = self.max_nanos.load(Ordering::Relaxed);
        while nanos > current_max {
            match self.max_nanos.compare_exchange_weak(
                current_max,
                nanos,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_max) => current_max = new_max,
            }
        }

        // Record in metrics system
        histogram!("orderbook_operation_duration_seconds", duration.as_secs_f64(), "operation" => self.operation.clone());
    }

    fn get_stats(&self) -> OperationLatencyStats {
        let samples = self.samples.load(Ordering::Relaxed);
        let total = self.total_nanos.load(Ordering::Relaxed);
        let min = self.min_nanos.load(Ordering::Relaxed);
        let max = self.max_nanos.load(Ordering::Relaxed);

        let avg = if samples > 0 { total / samples } else { 0 };

        OperationLatencyStats {
            operation: self.operation.clone(),
            samples,
            avg_nanos: avg,
            min_nanos: if min == u64::MAX { 0 } else { min },
            max_nanos: max,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub add_order: OperationLatencyStats,
    pub cancel_order: OperationLatencyStats,
    pub modify_order: OperationLatencyStats,
    pub match_order: OperationLatencyStats,
}

#[derive(Debug, Clone)]
pub struct OperationLatencyStats {
    pub operation: String,
    pub samples: u64,
    pub avg_nanos: u64,
    pub min_nanos: u64,
    pub max_nanos: u64,
}

impl OperationLatencyStats {
    pub fn avg_micros(&self) -> f64 {
        self.avg_nanos as f64 / 1_000.0
    }

    pub fn min_micros(&self) -> f64 {
        self.min_nanos as f64 / 1_000.0
    }

    pub fn max_micros(&self) -> f64 {
        self.max_nanos as f64 / 1_000.0
    }
}

/// Background metrics reporter
pub struct MetricsReporter {
    metrics: Arc<OrderBookMetrics>,
    interval: Duration,
}

impl MetricsReporter {
    pub fn new(metrics: Arc<OrderBookMetrics>, interval: Duration) -> Self {
        Self { metrics, interval }
    }

    pub async fn run(&self) {
        let mut interval = interval(self.interval);

        loop {
            interval.tick().await;

            let stats = self.metrics.get_latency_stats();

            info!(
              "OrderBook Metrics - Orders: +{} -{} ~{} | Trades: {} | Latency (μs): add={:.2} cancel={:.2} modify={:.2} match={:.2}",
              self.metrics.get_orders_added(),
              self.metrics.get_orders_cancelled(),
              self.metrics.get_orders_modified(),
              self.metrics.get_trades_executed(),
              stats.add_order.avg_micros(),
              stats.cancel_order.avg_micros(),
              stats.modify_order.avg_micros(),
              stats.match_order.avg_micros()  // FIXED: Added missing argument
          );
        }
    }
}
