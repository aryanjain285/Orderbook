mod metrics;
mod orderbook;

fn main() {
    // Initialize metrics
    let metrics = Arc::new(metrics::OrderBookMetrics::new());

    // Start metrics reporter
    let reporter = metrics::MetricsReporter::new(metrics.clone(), Duration::from_secs(5));
    tokio::spawn(async move {
        reporter.run().await;
    });

    // ... rest of your code ...
}
