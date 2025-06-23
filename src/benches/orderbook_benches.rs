//! High-Performance OrderBook Trading Engine
//!
//! A production-grade, lock-free order book engine designed for electronic trading systems.
//! Features sub-microsecond latency, millions of orders per second throughput, and comprehensive
//! monitoring capabilities.
//!
//! # Features
//!
//! - **Lock-free Design**: Uses atomic operations and concurrent data structures
//! - **High Performance**: Sub-microsecond order operations, millions of orders/second
//! - **Price-Time Priority**: Maintains strict FIFO ordering within price levels
//! - **Comprehensive Monitoring**: Built-in metrics with Prometheus and InfluxDB support
//! - **Multiple Order Types**: Market, Limit, Stop, IOC, FOK orders
//! - **Thread Safe**: Designed for high-concurrency trading environments
//!
//! # Quick Start
//!
//! ```rust
//! use orderbook_trading_engine::orderbook::{OrderBook, types::*};
//!
//! // Create a new order book
//! let book = OrderBook::new("AAPL".to_string());
//!
//! // Add a limit order
//! let order = Order::new_limit("AAPL".to_string(), Side::Buy, 15000, 100, None);
//! let events = book.add_limit_order(order)?;
//!
//! // Check best bid/ask
//! println!("Best bid: {:?}", book.best_bid());
//! println!("Best ask: {:?}", book.best_ask());
//!
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Architecture
//!
//! The order book uses a two-level data structure:
//!
//! 1. **Price Levels**: `DashMap<Price, Arc<PriceLevel>>` for lock-free price level access
//! 2. **Order Queues**: Within each price level, orders maintain time priority using `VecDeque`
//!
//! This design optimizes for:
//! - Fast order insertion/cancellation
//! - Efficient order matching
//! - Minimal memory allocations
//! - Cache-friendly data layout

pub mod metrics;
pub mod orderbook;
pub mod utils;

// Re-export commonly used types
pub use orderbook::{
    error::{OrderBookError, OrderBookResult},
    types::{Order, OrderId, OrderStatus, OrderType, Price, Quantity, Side, Trade},
    OrderBook,
};

pub use metrics::OrderBookMetrics;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_trading_workflow() {
        let book = OrderBook::new("TEST".to_string());

        // Add liquidity
        let sell_order = Order::new_limit("TEST".to_string(), Side::Sell, 10000, 100, None);
        let events = book.add_limit_order(sell_order).unwrap();
        assert_eq!(events.len(), 1);

        // Match with market order
        let buy_order = Order::new_market("TEST".to_string(), Side::Buy, 50, None);
        let events = book.add_market_order(buy_order).unwrap();
        assert_eq!(events.len(), 1);

        // Verify trade occurred
        if let orderbook::types::MarketEvent::Trade { trade } = &events[0] {
            assert_eq!(trade.price, 10000);
            assert_eq!(trade.quantity, 50);
        } else {
            panic!("Expected trade event");
        }
    }

    #[test]
    fn test_concurrent_trading() {
        let book = Arc::new(OrderBook::new("TEST".to_string()));
        let mut handles = vec![];

        // Spawn multiple trading threads
        for thread_id in 0..4 {
            let book_clone = Arc::clone(&book);
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    let price = 10000 + (thread_id * 100) + i;
                    let order = Order::new_limit("TEST".to_string(), Side::Buy, price, 100, None);
                    book_clone.add_limit_order(order).unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for completion
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all orders were added
        assert_eq!(book.total_orders(), 400);
    }
}
