use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::orderbook::error::{OrderBookError, OrderBookResult};
use crate::orderbook::price_level::PriceLevel;
use crate::orderbook::types::{
    MarketEvent, Order, OrderId, OrderStatus, OrderType, Price, Quantity, Side,
};

/// Order operations manager
pub struct OrderOperations;

impl OrderOperations {
    /// Add a new order to the book
    pub fn add_order(
        order: Order,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> OrderBookResult<Vec<MarketEvent>> {
        debug!("Adding order: {:?}", order);

        // Validate order
        Self::validate_order(&order)?;

        let mut events = Vec::new();
        let mut order = order;

        // Try to match the order first
        let trades = Self::match_order(&mut order, bids, asks)?;

        // Add trade events
        for trade in trades {
            events.push(MarketEvent::Trade { trade });
        }

        // If order has remaining quantity, add to book
        if order.remaining_quantity > 0 && !order.is_complete() {
            Self::add_order_to_book(order.clone(), bids, asks, order_locations)?;
            events.push(MarketEvent::OrderAdded { order });
        }

        Ok(events)
    }

    /// Cancel an existing order
    pub fn cancel_order(
        order_id: &OrderId,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> OrderBookResult<MarketEvent> {
        debug!("Cancelling order: {}", order_id);

        // Find and remove order location
        let location = order_locations
            .remove(order_id)
            .map(|(_, loc)| loc)
            .ok_or(OrderBookError::OrderNotFound)?;

        // Get the appropriate price level map
        let price_levels = match location.side {
            Side::Buy => bids,
            Side::Sell => asks,
        };

        // Remove order from price level
        if let Some(level) = price_levels.get(&location.price) {
            if let Some(mut order) = level.remove_order(order_id) {
                let remaining_quantity = order.remaining_quantity;
                order.cancel();

                // Clean up empty price level
                if level.is_empty() {
                    price_levels.remove(&location.price);
                }

                info!(
                    "Order {} cancelled, {} shares remaining",
                    order_id, remaining_quantity
                );
                return Ok(MarketEvent::OrderCancelled {
                    order_id: *order_id,
                    remaining_quantity,
                });
            }
        }

        Err(OrderBookError::OrderNotFound)
    }

    /// Modify an existing order
    pub fn modify_order(
        order_id: &OrderId,
        new_price: Option<Price>,
        new_quantity: Option<Quantity>,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> OrderBookResult<Vec<MarketEvent>> {
        debug!(
            "Modifying order: {} price: {:?} quantity: {:?}",
            order_id, new_price, new_quantity
        );

        // If changing price, we need to cancel and re-add
        if let Some(new_price) = new_price {
            return Self::modify_order_with_price_change(
                order_id,
                new_price,
                new_quantity,
                bids,
                asks,
                order_locations,
            );
        }

        // If only changing quantity, modify in place
        if let Some(new_quantity) = new_quantity {
            return Self::modify_order_quantity_only(
                order_id,
                new_quantity,
                bids,
                asks,
                order_locations,
            );
        }

        Err(OrderBookError::InvalidOrderState)
    }

    /// Replace an order (cancel old, add new)
    pub fn replace_order(
        old_order_id: &OrderId,
        new_order: Order,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> OrderBookResult<Vec<MarketEvent>> {
        debug!(
            "Replacing order: {} with new order: {}",
            old_order_id, new_order.id
        );

        let mut events = Vec::new();

        // Cancel old order
        match Self::cancel_order(old_order_id, bids, asks, order_locations) {
            Ok(cancel_event) => events.push(cancel_event),
            Err(OrderBookError::OrderNotFound) => {
                warn!("Order {} not found for replacement", old_order_id);
            }
            Err(e) => return Err(e),
        }

        // Add new order
        let mut add_events = Self::add_order(new_order, bids, asks, order_locations)?;
        events.append(&mut add_events);

        Ok(events)
    }

    // Private helper methods

    fn validate_order(order: &Order) -> OrderBookResult<()> {
        // Check quantity
        if order.original_quantity == 0 || order.remaining_quantity == 0 {
            return Err(OrderBookError::InvalidQuantity);
        }

        // Check price for limit orders
        match order.order_type {
            OrderType::Limit | OrderType::ImmediateOrCancel | OrderType::FillOrKill => {
                if order.price == 0 {
                    return Err(OrderBookError::InvalidPrice);
                }
            }
            OrderType::Market => {
                // Market orders don't need price validation
            }
            OrderType::Stop | OrderType::StopLimit { .. } => {
                return Err(OrderBookError::InvalidOrderType);
            }
        }

        // Check order state
        if order.status != OrderStatus::New {
            return Err(OrderBookError::InvalidOrderState);
        }

        Ok(())
    }

    fn match_order(
        order: &mut Order,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
    ) -> OrderBookResult<Vec<crate::orderbook::types::Trade>> {
        use crate::orderbook::matching::MatchingEngine;

        // Get opposite side levels
        let opposite_levels = match order.side {
            Side::Buy => {
                let mut levels: Vec<_> = asks
                    .iter()
                    .map(|entry| (*entry.key(), Arc::clone(entry.value())))
                    .collect();
                levels.sort_by_key(|(price, _)| *price); // Ascending for asks
                levels
            }
            Side::Sell => {
                let mut levels: Vec<_> = bids
                    .iter()
                    .map(|entry| (*entry.key(), Arc::clone(entry.value())))
                    .collect();
                levels.sort_by_key(|(price, _)| std::cmp::Reverse(*price)); // Descending for bids
                levels
            }
        };

        // Validate order for matching
        MatchingEngine::validate_order_for_matching(order)?;

        // Perform matching
        let trades = MatchingEngine::match_order(order, &opposite_levels)?;

        // Clean up empty levels
        for (price, level) in opposite_levels {
            if MatchingEngine::should_cleanup_level(&level) {
                match order.side {
                    Side::Buy => {
                        asks.remove(&price);
                    }
                    Side::Sell => {
                        bids.remove(&price);
                    }
                }
            }
        }

        Ok(trades)
    }

    fn add_order_to_book(
        order: Order,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> OrderBookResult<()> {
        let price = order.price;
        let side = order.side;
        let order_id = order.id;

        // Choose the correct side of the book
        let price_levels = match side {
            Side::Buy => bids,
            Side::Sell => asks,
        };

        // Get or create price level
        let level = price_levels
            .entry(price)
            .or_insert_with(|| Arc::new(PriceLevel::new(price)))
            .clone();

        // Add order to price level
        level.add_order(order);

        // Track order location
        order_locations.insert(
            order_id,
            crate::orderbook::types::OrderLocation { price, side },
        );

        debug!(
            "Order {} added to book at price {} on {} side",
            order_id, price, side
        );
        Ok(())
    }

    fn modify_order_with_price_change(
        order_id: &OrderId,
        new_price: Price,
        new_quantity: Option<Quantity>,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> OrderBookResult<Vec<MarketEvent>> {
        debug!(
            "Modifying order {} with price change to {}",
            order_id, new_price
        );

        // Find the current order
        let location = order_locations
            .get(order_id)
            .map(|entry| entry.value().clone())
            .ok_or(OrderBookError::OrderNotFound)?;

        let price_levels = match location.side {
            Side::Buy => bids,
            Side::Sell => asks,
        };

        // Remove order from current location
        let mut order = if let Some(level) = price_levels.get(&location.price) {
            level
                .remove_order(order_id)
                .ok_or(OrderBookError::OrderNotFound)?
        } else {
            return Err(OrderBookError::OrderNotFound);
        };

        // Update order properties
        order.price = new_price;
        if let Some(quantity) = new_quantity {
            // Validate new quantity
            if quantity == 0 {
                return Err(OrderBookError::InvalidQuantity);
            }
            if quantity < order.filled_quantity {
                return Err(OrderBookError::InvalidQuantity);
            }
            order.remaining_quantity = quantity - order.filled_quantity;
            order.original_quantity = quantity;
        }

        // Clean up old price level if empty
        if let Some(old_level) = price_levels.get(&location.price) {
            if old_level.is_empty() {
                price_levels.remove(&location.price);
            }
        }

        // Remove old location
        order_locations.remove(order_id);

        // Re-add order with new properties (this will try matching first)
        Self::add_order(order, bids, asks, order_locations)
    }

    fn modify_order_quantity_only(
        order_id: &OrderId,
        new_quantity: Quantity,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> OrderBookResult<Vec<MarketEvent>> {
        debug!("Modifying order {} quantity to {}", order_id, new_quantity);

        if new_quantity == 0 {
            return Err(OrderBookError::InvalidQuantity);
        }

        let location = order_locations
            .get(order_id)
            .map(|entry| entry.value().clone())
            .ok_or(OrderBookError::OrderNotFound)?;

        let price_levels = match location.side {
            Side::Buy => bids,
            Side::Sell => asks,
        };

        if let Some(level) = price_levels.get(&location.price) {
            if level
                .modify_order_quantity(order_id, new_quantity)
                .is_some()
            {
                return Ok(vec![MarketEvent::OrderModified {
                    order_id: *order_id,
                    new_price: None,
                    new_quantity: Some(new_quantity),
                }]);
            }
        }

        Err(OrderBookError::OrderNotFound)
    }
}

/// Batch operations for high-performance scenarios
pub struct BatchOperations;

impl BatchOperations {
    /// Process multiple orders in a batch
    pub fn process_batch(
        orders: Vec<Order>,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> Vec<OrderBookResult<Vec<MarketEvent>>> {
        orders
            .into_iter()
            .map(|order| OrderOperations::add_order(order, bids, asks, order_locations))
            .collect()
    }

    /// Cancel multiple orders in a batch
    pub fn cancel_batch(
        order_ids: Vec<OrderId>,
        bids: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        asks: &dashmap::DashMap<Price, Arc<PriceLevel>>,
        order_locations: &dashmap::DashMap<OrderId, crate::orderbook::types::OrderLocation>,
    ) -> Vec<OrderBookResult<MarketEvent>> {
        order_ids
            .into_iter()
            .map(|order_id| OrderOperations::cancel_order(&order_id, bids, asks, order_locations))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::types::{OrderStatus, OrderType, Side};
    use chrono::Utc;
    use dashmap::DashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    fn create_test_order(side: Side, price: Price, quantity: Quantity) -> Order {
        Order {
            id: Uuid::new_v4(),
            symbol: "TEST".to_string(),
            side,
            order_type: OrderType::Limit,
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
    fn test_add_order() {
        let bids = Arc::new(DashMap::new());
        let asks = Arc::new(DashMap::new());
        let locations = Arc::new(DashMap::new());

        let order = create_test_order(Side::Buy, 10000, 100);
        let events = OrderOperations::add_order(order, &bids, &asks, &locations).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(bids.len(), 1);
        assert_eq!(locations.len(), 1);
    }

    #[test]
    fn test_cancel_order() {
        let bids = Arc::new(DashMap::new());
        let asks = Arc::new(DashMap::new());
        let locations = Arc::new(DashMap::new());

        let order = create_test_order(Side::Buy, 10000, 100);
        let order_id = order.id;

        OrderOperations::add_order(order, &bids, &asks, &locations).unwrap();

        let cancel_event =
            OrderOperations::cancel_order(&order_id, &bids, &asks, &locations).unwrap();

        if let MarketEvent::OrderCancelled {
            order_id: cancelled_id,
            remaining_quantity,
        } = cancel_event
        {
            assert_eq!(cancelled_id, order_id);
            assert_eq!(remaining_quantity, 100);
        } else {
            panic!("Expected cancel event");
        }

        assert_eq!(bids.len(), 0);
        assert_eq!(locations.len(), 0);
    }

    #[test]
    fn test_modify_order_quantity() {
        let bids = Arc::new(DashMap::new());
        let asks = Arc::new(DashMap::new());
        let locations = Arc::new(DashMap::new());

        let order = create_test_order(Side::Buy, 10000, 100);
        let order_id = order.id;

        OrderOperations::add_order(order, &bids, &asks, &locations).unwrap();

        let events =
            OrderOperations::modify_order(&order_id, None, Some(150), &bids, &asks, &locations)
                .unwrap();

        assert_eq!(events.len(), 1);
        if let MarketEvent::OrderModified {
            order_id: modified_id,
            new_quantity,
            ..
        } = &events[0]
        {
            assert_eq!(*modified_id, order_id);
            assert_eq!(*new_quantity, Some(150));
        } else {
            panic!("Expected modify event");
        }
    }
}
