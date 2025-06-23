use crate::orderbook::types::{Order, OrderId, Price, Quantity};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

/// Represents a price level in the order book
/// All orders at this price level maintain time priority (FIFO)
#[derive(Debug)]
pub struct PriceLevel {
    pub price: Price,
    orders: RwLock<VecDeque<Order>>,
    total_quantity: AtomicU64,
    order_count: AtomicU64,
}

impl PriceLevel {
    pub fn new(price: Price) -> Self {
        Self {
            price,
            orders: RwLock::new(VecDeque::new()),
            total_quantity: AtomicU64::new(0),
            order_count: AtomicU64::new(0),
        }
    }

    /// Add an order to this price level (maintains time priority)
    pub fn add_order(&self, order: Order) {
        let quantity = order.remaining_quantity;

        {
            let mut orders = self.orders.write();
            orders.push_back(order);
        }

        self.total_quantity.fetch_add(quantity, Ordering::Relaxed);
        self.order_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Remove an order by ID from this price level
    pub fn remove_order(&self, order_id: &OrderId) -> Option<Order> {
        let mut orders = self.orders.write();

        if let Some(pos) = orders.iter().position(|o| &o.id == order_id) {
            if let Some(order) = orders.remove(pos) {
                self.total_quantity
                    .fetch_sub(order.remaining_quantity, Ordering::Relaxed);
                self.order_count.fetch_sub(1, Ordering::Relaxed);
                return Some(order);
            }
        }

        None
    }

    /// Get the first order in the queue (for matching)
    pub fn peek_front(&self) -> Option<Order> {
        let orders = self.orders.read();
        orders.front().cloned()
    }

    /// Take quantity from the front of the queue (for market orders)
    /// Returns Vec of (order, filled_quantity) pairs
    pub fn take_quantity(&self, mut requested_quantity: Quantity) -> Vec<(Order, Quantity)> {
        let mut filled_orders = Vec::new();
        let mut orders = self.orders.write();

        while requested_quantity > 0 && !orders.is_empty() {
            if let Some(mut order) = orders.front_mut() {
                let available = order.remaining_quantity;
                let fill_quantity = requested_quantity.min(available);

                // Fill the order
                order.fill(fill_quantity).expect("Fill should succeed");
                requested_quantity -= fill_quantity;

                // Track the fill
                filled_orders.push((order.clone(), fill_quantity));

                // Remove if completely filled
                if order.remaining_quantity == 0 {
                    orders.pop_front();
                    self.order_count.fetch_sub(1, Ordering::Relaxed);
                }

                self.total_quantity
                    .fetch_sub(fill_quantity, Ordering::Relaxed);

                if fill_quantity < available {
                    break; // Order partially filled, we're done
                }
            }
        }

        filled_orders
    }

    /// Modify an order's quantity at this price level
    pub fn modify_order_quantity(
        &self,
        order_id: &OrderId,
        new_quantity: Quantity,
    ) -> Option<Quantity> {
        let mut orders = self.orders.write();

        if let Some(order) = orders.iter_mut().find(|o| &o.id == order_id) {
            let old_quantity = order.remaining_quantity;
            let quantity_diff = new_quantity as i64 - old_quantity as i64;

            order.remaining_quantity = new_quantity;

            if quantity_diff != 0 {
                if quantity_diff > 0 {
                    self.total_quantity
                        .fetch_add(quantity_diff as u64, Ordering::Relaxed);
                } else {
                    self.total_quantity
                        .fetch_sub((-quantity_diff) as u64, Ordering::Relaxed);
                }
            }

            Some(old_quantity)
        } else {
            None
        }
    }

    /// Get total quantity at this price level
    pub fn total_quantity(&self) -> Quantity {
        self.total_quantity.load(Ordering::Relaxed)
    }

    /// Get number of orders at this price level
    pub fn order_count(&self) -> u32 {
        self.order_count.load(Ordering::Relaxed) as u32
    }

    /// Check if this price level is empty
    pub fn is_empty(&self) -> bool {
        self.order_count() == 0
    }

    /// Get all orders at this price level (for snapshots)
    pub fn get_all_orders(&self) -> Vec<Order> {
        let orders = self.orders.read();
        orders.iter().cloned().collect()
    }

    /// Get depth information for this level
    pub fn get_depth_info(&self) -> (Quantity, u32) {
        (self.total_quantity(), self.order_count())
    }
}

impl Clone for PriceLevel {
    fn clone(&self) -> Self {
        let orders = self.orders.read();
        Self {
            price: self.price,
            orders: RwLock::new(orders.clone()),
            total_quantity: AtomicU64::new(self.total_quantity.load(Ordering::Relaxed)),
            order_count: AtomicU64::new(self.order_count.load(Ordering::Relaxed)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::types::{OrderStatus, OrderType, Side};
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_order(price: Price, quantity: Quantity) -> Order {
        Order {
            id: Uuid::new_v4(),
            symbol: "TEST".to_string(),
            side: Side::Buy,
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
    fn test_price_level_add_order() {
        let level = PriceLevel::new(10000);
        let order = create_test_order(10000, 100);

        level.add_order(order);

        assert_eq!(level.total_quantity(), 100);
        assert_eq!(level.order_count(), 1);
        assert!(!level.is_empty());
    }

    #[test]
    fn test_price_level_time_priority() {
        let level = PriceLevel::new(10000);

        let order1 = create_test_order(10000, 100);
        let order2 = create_test_order(10000, 200);
        let order1_id = order1.id;

        level.add_order(order1);
        level.add_order(order2);

        // First order should be at the front
        let front = level.peek_front().unwrap();
        assert_eq!(front.id, order1_id);
        assert_eq!(front.remaining_quantity, 100);
    }

    #[test]
    fn test_take_quantity_full_fill() {
        let level = PriceLevel::new(10000);
        let order = create_test_order(10000, 100);

        level.add_order(order);

        let fills = level.take_quantity(100);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].1, 100); // Fill quantity
        assert_eq!(level.total_quantity(), 0);
        assert_eq!(level.order_count(), 0);
    }

    #[test]
    fn test_take_quantity_partial_fill() {
        let level = PriceLevel::new(10000);
        let order = create_test_order(10000, 100);

        level.add_order(order);

        let fills = level.take_quantity(50);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].1, 50); // Fill quantity
        assert_eq!(level.total_quantity(), 50);
        assert_eq!(level.order_count(), 1);
    }

    #[test]
    fn test_take_quantity_multiple_orders() {
        let level = PriceLevel::new(10000);

        level.add_order(create_test_order(10000, 100));
        level.add_order(create_test_order(10000, 200));

        let fills = level.take_quantity(250);
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].1, 100); // First order fully filled
        assert_eq!(fills[1].1, 150); // Second order partially filled
        assert_eq!(level.total_quantity(), 50); // 200 - 150 remaining
        assert_eq!(level.order_count(), 1);
    }

    #[test]
    fn test_remove_order() {
        let level = PriceLevel::new(10000);
        let order = create_test_order(10000, 100);
        let order_id = order.id;

        level.add_order(order);

        let removed = level.remove_order(&order_id);
        assert!(removed.is_some());
        assert_eq!(level.total_quantity(), 0);
        assert_eq!(level.order_count(), 0);
        assert!(level.is_empty());
    }

    #[test]
    fn test_modify_order_quantity() {
        let level = PriceLevel::new(10000);
        let order = create_test_order(10000, 100);
        let order_id = order.id;

        level.add_order(order);

        // Increase quantity
        let old_qty = level.modify_order_quantity(&order_id, 150);
        assert_eq!(old_qty, Some(100));
        assert_eq!(level.total_quantity(), 150);

        // Decrease quantity
        let old_qty = level.modify_order_quantity(&order_id, 75);
        assert_eq!(old_qty, Some(150));
        assert_eq!(level.total_quantity(), 75);
    }
}
