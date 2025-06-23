//! Core order book implementation module
//!
//! This module contains the main order book data structures and algorithms
//! for high-performance electronic trading systems.

pub mod book;
pub mod error;
pub mod matching;
pub mod operations;
pub mod price_level;
pub mod types;

// Re-export main types for convenience
pub use book::{OrderBook, OrderBookStats};
pub use error::{OrderBookError, OrderBookResult};
pub use price_level::PriceLevel;
pub use types::{
    BookSnapshot, MarketEvent, Order, OrderId, OrderLocation, OrderStatus, OrderType, Price,
    PriceLevelInfo, Quantity, Side, Trade,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Test that all main types are accessible
        let _book = OrderBook::new("TEST".to_string());
        let _order = Order::new_limit("TEST".to_string(), Side::Buy, 10000, 100, None);
        let _error = OrderBookError::OrderNotFound;
    }
}
