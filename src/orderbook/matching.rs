use std::sync::Arc;
use tracing::{debug, warn};

use crate::orderbook::error::OrderBookError;
use crate::orderbook::price_level::PriceLevel;
use crate::orderbook::types::{Order, OrderType, Price, Quantity, Side, Trade};

/// Advanced matching engine with support for different order types
pub struct MatchingEngine;

impl MatchingEngine {
    /// Match an incoming order against existing orders in the book
    pub fn match_order(
        order: &mut Order,
        opposite_levels: &[(Price, Arc<PriceLevel>)],
    ) -> Result<Vec<Trade>, OrderBookError> {
        match order.order_type {
            OrderType::Market => Self::match_market_order(order, opposite_levels),
            OrderType::Limit => Self::match_limit_order(order, opposite_levels),
            OrderType::ImmediateOrCancel => Self::match_ioc_order(order, opposite_levels),
            OrderType::FillOrKill => Self::match_fok_order(order, opposite_levels),
            OrderType::Stop => Err(OrderBookError::InvalidOrderType), // Stop orders need special handling
            OrderType::StopLimit { .. } => Err(OrderBookError::InvalidOrderType), // Stop-limit orders need special handling
        }
    }

    /// Match a market order (executes at any available price)
    fn match_market_order(
        order: &mut Order,
        opposite_levels: &[(Price, Arc<PriceLevel>)],
    ) -> Result<Vec<Trade>, OrderBookError> {
        let mut trades = Vec::new();

        debug!(
            "Matching market order {} for {} shares",
            order.id, order.remaining_quantity
        );

        for (price, level) in opposite_levels {
            if order.remaining_quantity == 0 {
                break;
            }

            let available_quantity = level.total_quantity();
            if available_quantity == 0 {
                continue;
            }

            let match_quantity = order.remaining_quantity.min(available_quantity);
            let fills = level.take_quantity(match_quantity);

            for (mut matched_order, fill_quantity) in fills {
                let (buyer_id, seller_id) = match order.side {
                    Side::Buy => (order.id, matched_order.id),
                    Side::Sell => (matched_order.id, order.id),
                };

                let trade = Trade::new(
                    order.symbol.clone(),
                    buyer_id,
                    seller_id,
                    *price,
                    fill_quantity,
                );

                // Update order quantities
                order.fill(fill_quantity);

                trades.push(trade);

                if order.remaining_quantity == 0 {
                    break;
                }
            }
        }

        debug!(
            "Market order {} generated {} trades",
            order.id,
            trades.len()
        );
        Ok(trades)
    }

    /// Match a limit order (only executes at specified price or better)
    fn match_limit_order(
        order: &mut Order,
        opposite_levels: &[(Price, Arc<PriceLevel>)],
    ) -> Result<Vec<Trade>, OrderBookError> {
        let mut trades = Vec::new();

        debug!(
            "Matching limit order {} at price {} for {} shares",
            order.id, order.price, order.remaining_quantity
        );

        for (price, level) in opposite_levels {
            if order.remaining_quantity == 0 {
                break;
            }

            // Check if we can match at this price level
            let can_match = match order.side {
                Side::Buy => order.price >= *price, // Buy order can match at ask price <= limit price
                Side::Sell => order.price <= *price, // Sell order can match at bid price >= limit price
            };

            if !can_match {
                break; // No more matches possible at better prices
            }

            let available_quantity = level.total_quantity();
            if available_quantity == 0 {
                continue;
            }

            let match_quantity = order.remaining_quantity.min(available_quantity);
            let fills = level.take_quantity(match_quantity);

            for (mut matched_order, fill_quantity) in fills {
                let (buyer_id, seller_id) = match order.side {
                    Side::Buy => (order.id, matched_order.id),
                    Side::Sell => (matched_order.id, order.id),
                };

                let trade = Trade::new(
                    order.symbol.clone(),
                    buyer_id,
                    seller_id,
                    *price, // Trade executes at the price of the resting order
                    fill_quantity,
                );

                // Update order quantities
                order.fill(fill_quantity);

                trades.push(trade);

                if order.remaining_quantity == 0 {
                    break;
                }
            }
        }

        debug!("Limit order {} generated {} trades", order.id, trades.len());
        Ok(trades)
    }

    /// Match an Immediate-or-Cancel (IOC) order
    fn match_ioc_order(
        order: &mut Order,
        opposite_levels: &[(Price, Arc<PriceLevel>)],
    ) -> Result<Vec<Trade>, OrderBookError> {
        // IOC orders are like limit orders but any unfilled quantity is cancelled
        let trades = Self::match_limit_order(order, opposite_levels)?;

        // Cancel any remaining quantity
        if order.remaining_quantity > 0 {
            debug!(
                "IOC order {} has {} shares remaining - cancelling",
                order.id, order.remaining_quantity
            );
            order.cancel();
        }

        Ok(trades)
    }

    /// Match a Fill-or-Kill (FOK) order
    fn match_fok_order(
        order: &mut Order,
        opposite_levels: &[(Price, Arc<PriceLevel>)],
    ) -> Result<Vec<Trade>, OrderBookError> {
        // First, check if the entire order can be filled
        let total_available = Self::calculate_available_quantity(order, opposite_levels);

        if total_available < order.remaining_quantity {
            debug!(
                "FOK order {} cannot be completely filled - rejecting",
                order.id
            );
            order.cancel();
            return Ok(Vec::new()); // No trades if order cannot be completely filled
        }

        // If we can fill the entire order, proceed with matching
        Self::match_limit_order(order, opposite_levels)
    }

    /// Calculate total available quantity that can match an order
    fn calculate_available_quantity(
        order: &Order,
        opposite_levels: &[(Price, Arc<PriceLevel>)],
    ) -> Quantity {
        let mut total_available = 0;

        for (price, level) in opposite_levels {
            // Check if we can match at this price level
            let can_match = match order.side {
                Side::Buy => order.price >= *price,
                Side::Sell => order.price <= *price,
            };

            if !can_match {
                break;
            }

            total_available += level.total_quantity();
        }

        total_available
    }

    /// Check if two orders would result in a self-trade
    pub fn is_self_trade(order1: &Order, order2: &Order) -> bool {
        if let (Some(client1), Some(client2)) = (&order1.client_id, &order2.client_id) {
            client1 == client2
        } else {
            false
        }
    }

    /// Calculate the fair value price for a trade between two orders
    pub fn calculate_trade_price(aggressive_order: &Order, passive_order: &Order) -> Price {
        // In most markets, trades execute at the price of the passive (resting) order
        // This gives price priority to orders that were placed first
        passive_order.price
    }

    /// Validate that an order can be matched
    pub fn validate_order_for_matching(order: &Order) -> Result<(), OrderBookError> {
        if order.remaining_quantity == 0 {
            return Err(OrderBookError::InvalidQuantity);
        }

        if order.is_complete() {
            return Err(OrderBookError::InvalidOrderState);
        }

        match order.order_type {
            OrderType::Market => {
                // Market orders don't need price validation
                Ok(())
            }
            OrderType::Limit | OrderType::ImmediateOrCancel | OrderType::FillOrKill => {
                if order.price == 0 {
                    return Err(OrderBookError::InvalidPrice);
                }
                Ok(())
            }
            OrderType::Stop | OrderType::StopLimit { .. } => {
                // Stop orders need special validation
                Err(OrderBookError::InvalidOrderType)
            }
        }
    }

    /// Determine if a price level should be cleaned up after matching
    pub fn should_cleanup_level(level: &PriceLevel) -> bool {
        level.is_empty()
    }
}

/// Trade execution context for advanced order types
#[derive(Debug)]
pub struct TradeContext {
    pub symbol: String,
    pub best_bid: Option<Price>,
    pub best_ask: Option<Price>,
    pub last_trade_price: Option<Price>,
    pub market_open: bool,
}

impl TradeContext {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            best_bid: None,
            best_ask: None,
            last_trade_price: None,
            market_open: true,
        }
    }

    /// Update market data
    pub fn update(
        &mut self,
        best_bid: Option<Price>,
        best_ask: Option<Price>,
        last_trade: Option<Price>,
    ) {
        self.best_bid = best_bid;
        self.best_ask = best_ask;
        if let Some(price) = last_trade {
            self.last_trade_price = Some(price);
        }
    }

    /// Get current mid-price
    pub fn mid_price(&self) -> Option<Price> {
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) => Some((bid + ask) / 2),
            _ => None,
        }
    }

    /// Get current spread
    pub fn spread(&self) -> Option<Price> {
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) if ask > bid => Some(ask - bid),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::types::{OrderStatus, OrderType};
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_order(
        side: Side,
        price: Price,
        quantity: Quantity,
        order_type: OrderType,
    ) -> Order {
        Order {
            id: Uuid::new_v4(),
            symbol: "TEST".to_string(),
            side,
            order_type,
            price,
            original_quantity: quantity,
            remaining_quantity: quantity,
            filled_quantity: 0,
            status: OrderStatus::New,
            timestamp: Utc::now(),
            client_id: None,
        }
    }

    #[test]
    fn test_validate_order_for_matching() {
        let order = create_test_order(Side::Buy, 10000, 100, OrderType::Limit);
        assert!(MatchingEngine::validate_order_for_matching(&order).is_ok());

        let market_order = create_test_order(Side::Buy, 0, 100, OrderType::Market);
        assert!(MatchingEngine::validate_order_for_matching(&market_order).is_ok());

        let invalid_order = create_test_order(Side::Buy, 0, 100, OrderType::Limit);
        assert!(MatchingEngine::validate_order_for_matching(&invalid_order).is_err());
    }

    #[test]
    fn test_calculate_available_quantity() {
        let levels = vec![
            (10000, Arc::new(PriceLevel::new(10000))),
            (10100, Arc::new(PriceLevel::new(10100))),
        ];

        // Add some orders to the levels
        for level in &levels {
            let order = create_test_order(Side::Sell, level.0, 100, OrderType::Limit);
            level.1.add_order(order);
        }

        let buy_order = create_test_order(Side::Buy, 10050, 250, OrderType::Limit);
        let available = MatchingEngine::calculate_available_quantity(&buy_order, &levels);

        // Should only match first level since buy price (10050) < second level price (10100)
        assert_eq!(available, 100);

        let buy_order_high = create_test_order(Side::Buy, 10200, 250, OrderType::Limit);
        let available_high = MatchingEngine::calculate_available_quantity(&buy_order_high, &levels);

        // Should match both levels
        assert_eq!(available_high, 200);
    }

    #[test]
    fn test_is_self_trade() {
        let order1 = Order {
            id: Uuid::new_v4(),
            symbol: "TEST".to_string(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: 10000,
            original_quantity: 100,
            remaining_quantity: 100,
            filled_quantity: 0,
            status: OrderStatus::New,
            timestamp: Utc::now(),
            client_id: Some("client1".to_string()),
        };

        let order2 = Order {
            id: Uuid::new_v4(),
            symbol: "TEST".to_string(),
            side: Side::Sell,
            order_type: OrderType::Limit,
            price: 10000,
            original_quantity: 100,
            remaining_quantity: 100,
            filled_quantity: 0,
            status: OrderStatus::New,
            timestamp: Utc::now(),
            client_id: Some("client1".to_string()),
        };

        let order3 = Order {
            id: Uuid::new_v4(),
            symbol: "TEST".to_string(),
            side: Side::Sell,
            order_type: OrderType::Limit,
            price: 10000,
            original_quantity: 100,
            remaining_quantity: 100,
            filled_quantity: 0,
            status: OrderStatus::New,
            timestamp: Utc::now(),
            client_id: Some("client2".to_string()),
        };

        assert!(MatchingEngine::is_self_trade(&order1, &order2));
        assert!(!MatchingEngine::is_self_trade(&order1, &order3));
    }

    #[test]
    fn test_trade_context() {
        let mut context = TradeContext::new("TEST".to_string());

        context.update(Some(9950), Some(10050), Some(10000));

        assert_eq!(context.best_bid, Some(9950));
        assert_eq!(context.best_ask, Some(10050));
        assert_eq!(context.last_trade_price, Some(10000));
        assert_eq!(context.mid_price(), Some(10000));
        assert_eq!(context.spread(), Some(100));
    }
}
