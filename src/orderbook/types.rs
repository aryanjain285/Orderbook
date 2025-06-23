use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

pub type OrderId = Uuid;
pub type Price = u64; // Price in ticks (e.g., 1 tick = 0.01 cents)
pub type Quantity = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit { stop_price: Price },
    ImmediateOrCancel, // IOC
    FillOrKill,        // FOK
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Price,
    pub original_quantity: Quantity,
    pub remaining_quantity: Quantity,
    pub filled_quantity: Quantity,
    pub status: OrderStatus,
    pub timestamp: DateTime<Utc>,
    pub client_id: Option<String>,
}

impl Order {
    pub fn new_limit(
        symbol: String,
        side: Side,
        price: Price,
        quantity: Quantity,
        client_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            symbol,
            side,
            order_type: OrderType::Limit,
            price,
            original_quantity: quantity,
            remaining_quantity: quantity,
            filled_quantity: 0,
            status: OrderStatus::New,
            timestamp: Utc::now(),
            client_id,
        }
    }

    pub fn new_market(
        symbol: String,
        side: Side,
        quantity: Quantity,
        client_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            symbol,
            side,
            order_type: OrderType::Market,
            price: 0, // Market orders don't have a price
            original_quantity: quantity,
            remaining_quantity: quantity,
            filled_quantity: 0,
            status: OrderStatus::New,
            timestamp: Utc::now(),
            client_id,
        }
    }

    pub fn fill(&mut self, quantity: Quantity) -> Result<(), &'static str> {
        if quantity > self.remaining_quantity {
            return Err("Cannot fill more than remaining quantity");
        }

        self.remaining_quantity -= quantity;
        self.filled_quantity += quantity;

        if self.remaining_quantity == 0 {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }

        Ok(())
    }

    pub fn cancel(&mut self) {
        self.status = OrderStatus::Cancelled;
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Filled
                | OrderStatus::Cancelled
                | OrderStatus::Rejected
                | OrderStatus::Expired
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub symbol: String,
    pub buyer_order_id: OrderId,
    pub seller_order_id: OrderId,
    pub price: Price,
    pub quantity: Quantity,
    pub timestamp: DateTime<Utc>,
}

impl Trade {
    pub fn new(
        symbol: String,
        buyer_order_id: OrderId,
        seller_order_id: OrderId,
        price: Price,
        quantity: Quantity,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            symbol,
            buyer_order_id,
            seller_order_id,
            price,
            quantity,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderLocation {
    pub price: Price,
    pub side: Side,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSnapshot {
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub bids: Vec<PriceLevelInfo>,
    pub asks: Vec<PriceLevelInfo>,
    pub last_trade_price: Option<Price>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelInfo {
    pub price: Price,
    pub quantity: Quantity,
    pub order_count: u32,
}

// Market data events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    OrderAdded {
        order: Order,
    },
    OrderCancelled {
        order_id: OrderId,
        remaining_quantity: Quantity,
    },
    OrderModified {
        order_id: OrderId,
        new_price: Option<Price>,
        new_quantity: Option<Quantity>,
    },
    Trade {
        trade: Trade,
    },
    BookSnapshot {
        snapshot: BookSnapshot,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_creation() {
        let order = Order::new_limit(
            "AAPL".to_string(),
            Side::Buy,
            15000, // $150.00
            100,
            Some("client123".to_string()),
        );

        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, 15000);
        assert_eq!(order.original_quantity, 100);
        assert_eq!(order.remaining_quantity, 100);
        assert_eq!(order.status, OrderStatus::New);
    }

    #[test]
    fn test_order_fill() {
        let mut order = Order::new_limit("AAPL".to_string(), Side::Buy, 15000, 100, None);

        // Partial fill
        order.fill(30).unwrap();
        assert_eq!(order.filled_quantity, 30);
        assert_eq!(order.remaining_quantity, 70);
        assert_eq!(order.status, OrderStatus::PartiallyFilled);

        // Complete fill
        order.fill(70).unwrap();
        assert_eq!(order.filled_quantity, 100);
        assert_eq!(order.remaining_quantity, 0);
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_overfill_error() {
        let mut order = Order::new_limit("AAPL".to_string(), Side::Buy, 15000, 100, None);
        let result = order.fill(150);
        assert!(result.is_err());
    }
}
