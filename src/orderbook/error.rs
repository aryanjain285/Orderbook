use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderBookError {
    /// Order not found in the book
    OrderNotFound,

    /// Invalid symbol for this order book
    InvalidSymbol,

    /// Invalid order type for the operation
    InvalidOrderType,

    /// Invalid price (e.g., zero or negative)
    InvalidPrice,

    /// Invalid quantity (e.g., zero or negative)
    InvalidQuantity,

    /// No liquidity available for market order
    NoLiquidity,

    /// Order already exists
    DuplicateOrder,

    /// Cannot fill more than remaining quantity
    OverFill,

    /// Order is not in a valid state for the operation
    InvalidOrderState,

    /// Self-trade prevention
    SelfTrade,

    /// Order size exceeds maximum allowed
    OrderTooLarge,

    /// Price is outside allowed range
    PriceOutOfRange,

    /// System error
    SystemError(String),
}

impl fmt::Display for OrderBookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderBookError::OrderNotFound => write!(f, "Order not found"),
            OrderBookError::InvalidSymbol => write!(f, "Invalid symbol"),
            OrderBookError::InvalidOrderType => write!(f, "Invalid order type"),
            OrderBookError::InvalidPrice => write!(f, "Invalid price"),
            OrderBookError::InvalidQuantity => write!(f, "Invalid quantity"),
            OrderBookError::NoLiquidity => write!(f, "No liquidity available"),
            OrderBookError::DuplicateOrder => write!(f, "Order already exists"),
            OrderBookError::OverFill => write!(f, "Cannot fill more than remaining quantity"),
            OrderBookError::InvalidOrderState => write!(f, "Invalid order state"),
            OrderBookError::SelfTrade => write!(f, "Self-trade not allowed"),
            OrderBookError::OrderTooLarge => write!(f, "Order size exceeds maximum"),
            OrderBookError::PriceOutOfRange => write!(f, "Price outside allowed range"),
            OrderBookError::SystemError(msg) => write!(f, "System error: {}", msg),
        }
    }
}

impl std::error::Error for OrderBookError {}

/// Result type for order book operations
pub type OrderBookResult<T> = Result<T, OrderBookError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        assert_eq!(OrderBookError::OrderNotFound.to_string(), "Order not found");
        assert_eq!(OrderBookError::InvalidSymbol.to_string(), "Invalid symbol");
        assert_eq!(
            OrderBookError::SystemError("Test error".to_string()).to_string(),
            "System error: Test error"
        );
    }

    #[test]
    fn test_error_serialization() {
        let error = OrderBookError::OrderNotFound;
        let serialized = serde_json::to_string(&error).unwrap();
        let deserialized: OrderBookError = serde_json::from_str(&serialized).unwrap();
        assert_eq!(error, deserialized);
    }
}
