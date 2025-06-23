//! High-Performance Trading Server
//!
//! A demonstration trading server that showcases the order book engine
//! with real-time metrics and monitoring capabilities.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};
use tracing_subscriber;

use orderbook_trading_engine::{
    metrics::MetricsReporter, orderbook::types::*, OrderBook, OrderBookMetrics,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!("Starting High-Performance Trading Server...");

    // Create order books for multiple symbols
    let symbols = vec!["AAPL", "GOOGL", "MSFT", "TSLA", "AMZN"];
    let mut order_books = std::collections::HashMap::new();
    let mut metrics_map = std::collections::HashMap::new();

    for symbol in &symbols {
        let book = Arc::new(OrderBook::new(symbol.to_string()));
        let metrics = Arc::new(OrderBookMetrics::new());

        order_books.insert(symbol.to_string(), book);
        metrics_map.insert(symbol.to_string(), metrics);

        info!("Created order book for symbol: {}", symbol);
    }

    // Start metrics reporting
    let mut metric_reporters = Vec::new();
    for (symbol, metrics) in &metrics_map {
        let reporter = MetricsReporter::new(Arc::clone(metrics), Duration::from_secs(5));

        let symbol_clone = symbol.clone();
        tokio::spawn(async move {
            info!("Starting metrics reporter for {}", symbol_clone);
            reporter.run().await;
        });

        metric_reporters.push(reporter);
    }

    // Start market data simulation
    for (symbol, book) in &order_books {
        let book_clone = Arc::clone(book);
        let symbol_clone = symbol.clone();
        let metrics_clone = Arc::clone(&metrics_map[symbol]);

        tokio::spawn(async move {
            simulate_market_activity(book_clone, symbol_clone, metrics_clone).await;
        });
    }

    // Start server statistics reporting
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            let mut total_orders = 0;
            let mut total_trades = 0;

            for (symbol, book) in &order_books {
                let stats = book.get_stats();
                total_orders += stats.total_orders;
                total_trades += stats.total_trades;

                info!(
                    "📊 {} | Orders: {} | Bid: {:?} | Ask: {:?} | Spread: {:?} | Trades: {}",
                    symbol,
                    stats.total_orders,
                    stats.best_bid.map(|p| format_price(p)),
                    stats.best_ask.map(|p| format_price(p)),
                    stats.spread.map(|s| format_price(s)),
                    stats.total_trades
                );
            }

            info!(
                "🚀 Server totals: {} orders, {} trades across {} symbols",
                total_orders,
                total_trades,
                order_books.len()
            );
        }
    });

    // Start Prometheus metrics server
    tokio::spawn(async move {
        if let Err(e) = start_metrics_server().await {
            error!("Failed to start metrics server: {}", e);
        }
    });

    info!("Trading server is running. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    info!("Shutting down trading server...");

    // Print final statistics
    for (symbol, book) in &order_books {
        let stats = book.get_stats();
        info!(
            "Final stats for {}: {} orders, {} trades",
            symbol, stats.total_orders, stats.total_trades
        );
    }

    Ok(())
}

/// Simulate realistic market activity for a symbol
async fn simulate_market_activity(
    book: Arc<OrderBook>,
    symbol: String,
    metrics: Arc<OrderBookMetrics>,
) {
    let mut interval = interval(Duration::from_millis(10)); // 100 ops/second
    let mut base_price = 10000; // Starting price in ticks
    let mut order_counter = 0;

    // Initial market making - add liquidity on both sides
    for i in 0..50 {
        let bid_price = base_price - (i * 10);
        let ask_price = base_price + (i * 10);

        let bid_order = Order::new_limit(symbol.clone(), Side::Buy, bid_price, 100, None);
        let ask_order = Order::new_limit(symbol.clone(), Side::Sell, ask_price, 100, None);

        if let Ok(_) = book.add_limit_order(bid_order) {
            metrics.increment_orders_added();
        }

        if let Ok(_) = book.add_limit_order(ask_order) {
            metrics.increment_orders_added();
        }
    }

    info!("Initial liquidity added for {}", symbol);

    loop {
        interval.tick().await;
        order_counter += 1;

        // Simulate different types of market activity
        match order_counter % 10 {
            // Market orders (20% of activity)
            0 | 1 => {
                let side = if order_counter % 2 == 0 {
                    Side::Buy
                } else {
                    Side::Sell
                };
                let quantity = 50 + (order_counter % 100);

                let market_order = Order::new_market(symbol.clone(), side, quantity, None);

                match metrics.time_add_order(|| book.add_market_order(market_order)) {
                    Ok(events) => {
                        for event in events {
                            if let MarketEvent::Trade { trade } = event {
                                metrics.increment_trades_executed(
                                    trade.quantity,
                                    trade.price * trade.quantity,
                                );
                            }
                        }
                    }
                    Err(_) => {
                        // No liquidity available, add some
                        let price = if side == Side::Buy {
                            base_price + 50
                        } else {
                            base_price - 50
                        };
                        let limit_order = Order::new_limit(
                            symbol.clone(),
                            opposite_side(side),
                            price,
                            quantity,
                            None,
                        );
                        if let Ok(_) = book.add_limit_order(limit_order) {
                            metrics.increment_orders_added();
                        }
                    }
                }
            }

            // Limit orders (60% of activity)
            2..=7 => {
                let side = if order_counter % 2 == 0 {
                    Side::Buy
                } else {
                    Side::Sell
                };
                let price_offset = (order_counter % 20) as u64;
                let price = if side == Side::Buy {
                    base_price - price_offset
                } else {
                    base_price + price_offset
                };
                let quantity = 100 + (order_counter % 200);

                let limit_order = Order::new_limit(symbol.clone(), side, price, quantity, None);

                match metrics.time_add_order(|| book.add_limit_order(limit_order)) {
                    Ok(events) => {
                        metrics.increment_orders_added();
                        for event in events {
                            if let MarketEvent::Trade { trade } = event {
                                metrics.increment_trades_executed(
                                    trade.quantity,
                                    trade.price * trade.quantity,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to add limit order for {}: {}", symbol, e);
                    }
                }
            }

            // Order cancellations (15% of activity)
            8 => {
                // Get a random order to cancel (simplified simulation)
                if book.total_orders() > 10 {
                    // In a real system, you'd track order IDs
                    // For simulation, we'll just add more orders instead
                    let side = if order_counter % 2 == 0 {
                        Side::Buy
                    } else {
                        Side::Sell
                    };
                    let price = base_price + ((order_counter % 10) as u64);
                    let quantity = 50;

                    let order = Order::new_limit(symbol.clone(), side, price, quantity, None);
                    if let Ok(_) = book.add_limit_order(order) {
                        metrics.increment_orders_added();
                    }
                }
            }

            // Price updates (5% of activity)
            9 => {
                // Simulate price movement
                let direction = if order_counter % 4 == 0 { 1 } else { -1 };
                base_price = ((base_price as i64) + direction).max(5000) as u64;

                // Update metrics with current book state
                if let Some(spread) = book.spread() {
                    metrics.set_spread(spread);
                }
                if let Some(bid) = book.best_bid() {
                    metrics.set_best_bid(bid);
                }
                if let Some(ask) = book.best_ask() {
                    metrics.set_best_ask(ask);
                }
                metrics.set_total_orders(book.total_orders() as u64);
            }

            _ => unreachable!(),
        }

        // Periodic cleanup and statistics update
        if order_counter % 100 == 0 {
            let stats = book.get_stats();
            metrics.set_total_orders(stats.total_orders as u64);
            metrics.set_bid_levels(stats.bid_levels as u64);
            metrics.set_ask_levels(stats.ask_levels as u64);
        }
    }
}

/// Get the opposite side for market making
fn opposite_side(side: Side) -> Side {
    match side {
        Side::Buy => Side::Sell,
        Side::Sell => Side::Buy,
    }
}

/// Format price from ticks to dollars
fn format_price(price_ticks: u64) -> String {
    format!("${:.2}", price_ticks as f64 / 100.0)
}

/// Start Prometheus metrics server
async fn start_metrics_server() -> Result<(), Box<dyn std::error::Error>> {
    use metrics_exporter_prometheus::PrometheusBuilder;
    use std::net::SocketAddr;

    let addr: SocketAddr = "0.0.0.0:9090".parse()?;

    info!(
        "Starting Prometheus metrics server on http://{}/metrics",
        addr
    );

    let builder = PrometheusBuilder::new();
    let handle = builder.install()?;

    // In a real implementation, you'd start an HTTP server here
    // For this example, we'll just log that it would be running
    info!(
        "Prometheus metrics would be available at http://{}/metrics",
        addr
    );

    // Keep the handle alive
    std::future::pending::<()>().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_formatting() {
        assert_eq!(format_price(10000), "$100.00");
        assert_eq!(format_price(12550), "$125.50");
        assert_eq!(format_price(99), "$0.99");
    }

    #[test]
    fn test_opposite_side() {
        assert_eq!(opposite_side(Side::Buy), Side::Sell);
        assert_eq!(opposite_side(Side::Sell), Side::Buy);
    }
}
