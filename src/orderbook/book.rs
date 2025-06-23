use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::orderbook::error::OrderBookError;
use crate::orderbook::price_level::PriceLevel;
use crate::orderbook::types::{
    BookSnapshot, MarketEvent, Order, OrderId, OrderLocation, OrderStatus, OrderType, Price,
    PriceLevelInfo, Quantity, Side, Trade,
};

/// High-performance lock-free order book
#[derive(Debug)]
pub struct OrderBook {
    pub symbol: String,

    // Price levels: Price -> PriceLevel
    // Using u64 keys for cache-friendly iteration
    bids: DashMap<Price, Arc<PriceLevel>>, // Buy orders (highest price first)
    asks: DashMap<Price, Arc<PriceLevel>>, // Sell orders (lowest price first)

    // Order tracking
    order_locations: DashMap<OrderId, OrderLocation>,

    // Market state
    last_trade_price: AtomicU64,
    sequence_number: AtomicU64,

    // Statistics
    total_trades: AtomicU64,
    total_volume: AtomicU64,
}

impl OrderBook {
    pub fn new(symbol: String) -> Self {
        info!("Creating new order book for symbol: {}", symbol);

        Self {
            symbol,
            bids: DashMap::new(),
            asks: DashMap::new(),
            order_locations: DashMap::new(),
            last_trade_price: AtomicU64::new(0),
            sequence_number: AtomicU64::new(0),
            total_trades: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
        }
    }

    /// Add a limit order to the book
    pub fn add_limit_order(&self, mut order: Order) -> Result<Vec<MarketEvent>, OrderBookError> {
        debug!("Adding limit order: {:?}", order);

        if order.symbol != self.symbol {
            return Err(OrderBookError::InvalidSymbol);
        }

        let mut events = Vec::new();

        // Try to match against opposite side first
        let trades = self.match_order(&mut order)?;

        // Add trade events
        for trade in trades {
            events.push(MarketEvent::Trade { trade });
        }

        // If order has remaining quantity, add to book
        if order.remaining_quantity > 0 {
            self.add_order_to_book(order.clone())?;
            events.push(MarketEvent::OrderAdded { order });
        }

        Ok(events)
    }

    /// Add a market order (always executes immediately)
    pub fn add_market_order(&self, mut order: Order) -> Result<Vec<MarketEvent>, OrderBookError> {
        debug!("Adding market order: {:?}", order);

        if order.symbol != self.symbol {
            return Err(OrderBookError::InvalidSymbol);
        }

        if order.order_type != OrderType::Market {
            return Err(OrderBookError::InvalidOrderType);
        }

        let mut events = Vec::new();

        // Market orders must execute immediately
        let trades = self.execute_market_order(&mut order)?;

        if trades.is_empty() {
            return Err(OrderBookError::NoLiquidity);
        }

        // Add trade events
        for trade in trades {
            events.push(MarketEvent::Trade { trade });
        }

        Ok(events)
    }

    /// Cancel an order
    pub fn cancel_order(&self, order_id: &OrderId) -> Result<MarketEvent, OrderBookError> {
        debug!("Cancelling order: {}", order_id);

        let location = self
            .order_locations
            .remove(order_id)
            .map(|(_, loc)| loc)
            .ok_or(OrderBookError::OrderNotFound)?;

        let price_levels = match location.side {
            Side::Buy => &self.bids,
            Side::Sell => &self.asks,
        };

        if let Some(level) = price_levels.get(&location.price) {
            if let Some(mut order) = level.remove_order(order_id) {
                let remaining_quantity = order.remaining_quantity;
                order.cancel();

                // Clean up empty price level
                if level.is_empty() {
                    price_levels.remove(&location.price);
                }

                return Ok(MarketEvent::OrderCancelled {
                    order_id: *order_id,
                    remaining_quantity,
                });
            }
        }

        Err(OrderBookError::OrderNotFound)
    }

    /// Modify an order's quantity
    pub fn modify_order_quantity(
        &self,
        order_id: &OrderId,
        new_quantity: Quantity,
    ) -> Result<MarketEvent, OrderBookError> {
        debug!("Modifying order {} to quantity {}", order_id, new_quantity);

        let location = self
            .order_locations
            .get(order_id)
            .map(|entry| entry.value().clone())
            .ok_or(OrderBookError::OrderNotFound)?;

        let price_levels = match location.side {
            Side::Buy => &self.bids,
            Side::Sell => &self.asks,
        };

        if let Some(level) = price_levels.get(&location.price) {
            if level
                .modify_order_quantity(order_id, new_quantity)
                .is_some()
            {
                return Ok(MarketEvent::OrderModified {
                    order_id: *order_id,
                    new_price: None,
                    new_quantity: Some(new_quantity),
                });
            }
        }

        Err(OrderBookError::OrderNotFound)
    }

    /// Get current best bid price
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.iter().map(|entry| *entry.key()).max()
    }

    /// Get current best ask price
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.iter().map(|entry| *entry.key()).min()
    }

    /// Get current spread
    pub fn spread(&self) -> Option<Price> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) if ask > bid => Some(ask - bid),
            _ => None,
        }
    }

    /// Get last trade price
    pub fn last_trade_price(&self) -> Option<Price> {
        let price = self.last_trade_price.load(Ordering::Relaxed);
        if price == 0 {
            None
        } else {
            Some(price)
        }
    }

    /// Generate order book snapshot
    pub fn snapshot(&self) -> BookSnapshot {
        let mut bids: Vec<_> = self
            .bids
            .iter()
            .map(|entry| {
                let price = *entry.key();
                let level = entry.value();
                let (quantity, order_count) = level.get_depth_info();
                PriceLevelInfo {
                    price,
                    quantity,
                    order_count,
                }
            })
            .collect();

        let mut asks: Vec<_> = self
            .asks
            .iter()
            .map(|entry| {
                let price = *entry.key();
                let level = entry.value();
                let (quantity, order_count) = level.get_depth_info();
                PriceLevelInfo {
                    price,
                    quantity,
                    order_count,
                }
            })
            .collect();

        // Sort bids by price descending (highest first)
        bids.sort_by(|a, b| b.price.cmp(&a.price));

        // Sort asks by price ascending (lowest first)
        asks.sort_by(|a, b| a.price.cmp(&b.price));

        BookSnapshot {
            symbol: self.symbol.clone(),
            timestamp: chrono::Utc::now(),
            bids,
            asks,
            last_trade_price: self.last_trade_price(),
        }
    }

    /// Get total number of orders in the book
    pub fn total_orders(&self) -> usize {
        self.order_locations.len()
    }

    /// Get statistics
    pub fn get_stats(&self) -> OrderBookStats {
        OrderBookStats {
            symbol: self.symbol.clone(),
            total_orders: self.total_orders(),
            bid_levels: self.bids.len(),
            ask_levels: self.asks.len(),
            best_bid: self.best_bid(),
            best_ask: self.best_ask(),
            spread: self.spread(),
            last_trade_price: self.last_trade_price(),
            total_trades: self.total_trades.load(Ordering::Relaxed),
            total_volume: self.total_volume.load(Ordering::Relaxed),
        }
    }

    // Private helper methods

    fn match_order(&self, order: &mut Order) -> Result<Vec<Trade>, OrderBookError> {
        let mut trades = Vec::new();
        let opposite_side = match order.side {
            Side::Buy => &self.asks,
            Side::Sell => &self.bids,
        };

        // Get sorted prices for matching
        let mut prices: Vec<Price> = opposite_side.iter().map(|entry| *entry.key()).collect();

        // Sort prices for optimal matching
        match order.side {
            Side::Buy => prices.sort(), // Buy orders match against lowest ask prices first
            Side::Sell => prices.sort_by(|a, b| b.cmp(a)), // Sell orders match against highest bid prices first
        }

        for price in prices {
            if order.remaining_quantity == 0 {
                break;
            }

            // Check if we can match at this price
            let can_match = match order.side {
                Side::Buy => order.price >= price,  // Buy order price >= ask price
                Side::Sell => order.price <= price, // Sell order price <= bid price
            };

            if !can_match {
                break; // No more matches possible
            }

            if let Some(level) = opposite_side.get(&price) {
                let available_quantity = level.total_quantity();
                if available_quantity == 0 {
                    continue;
                }

                let match_quantity = order.remaining_quantity.min(available_quantity);
                let fills = level.take_quantity(match_quantity);

                for (mut matched_order, fill_quantity) in fills {
                    // Create trade
                    let (buyer_id, seller_id) = match order.side {
                        Side::Buy => (order.id, matched_order.id),
                        Side::Sell => (matched_order.id, order.id),
                    };

                    let trade = Trade::new(
                        self.symbol.clone(),
                        buyer_id,
                        seller_id,
                        price,
                        fill_quantity,
                    );

                    // Update order quantities
                    order.fill(fill_quantity)?;

                    // Remove completely filled orders from tracking
                    if matched_order.is_complete() {
                        self.order_locations.remove(&matched_order.id);
                    }

                    trades.push(trade);
                }

                // Clean up empty price level
                if level.is_empty() {
                    opposite_side.remove(&price);
                }
            }
        }

        // Update statistics
        if !trades.is_empty() {
            let total_volume: u64 = trades.iter().map(|t| t.quantity).sum();
            self.total_trades
                .fetch_add(trades.len() as u64, Ordering::Relaxed);
            self.total_volume.fetch_add(total_volume, Ordering::Relaxed);

            // Update last trade price
            if let Some(last_trade) = trades.last() {
                self.last_trade_price
                    .store(last_trade.price, Ordering::Relaxed);
            }
        }

        Ok(trades)
    }

    fn execute_market_order(&self, order: &mut Order) -> Result<Vec<Trade>, OrderBookError> {
        let mut trades = Vec::new();
        let opposite_side = match order.side {
            Side::Buy => &self.asks,
            Side::Sell => &self.bids,
        };

        // Get sorted prices for market order execution
        let mut prices: Vec<Price> = opposite_side.iter().map(|entry| *entry.key()).collect();

        // Market orders take the best available prices
        match order.side {
            Side::Buy => prices.sort(), // Buy at lowest ask prices first
            Side::Sell => prices.sort_by(|a, b| b.cmp(a)), // Sell at highest bid prices first
        }

        for price in prices {
            if order.remaining_quantity == 0 {
                break;
            }

            if let Some(level) = opposite_side.get(&price) {
                let available_quantity = level.total_quantity();
                if available_quantity == 0 {
                    continue;
                }

                let match_quantity = order.remaining_quantity.min(available_quantity);
                let fills = level.take_quantity(match_quantity);

                for (mut matched_order, fill_quantity) in fills {
                    // Create trade
                    let (buyer_id, seller_id) = match order.side {
                        Side::Buy => (order.id, matched_order.id),
                        Side::Sell => (matched_order.id, order.id),
                    };

                    let trade = Trade::new(
                        self.symbol.clone(),
                        buyer_id,
                        seller_id,
                        price,
                        fill_quantity,
                    );

                    // Update order quantities
                    order.fill(fill_quantity)?;

                    // Remove completely filled orders from tracking
                    if matched_order.is_complete() {
                        self.order_locations.remove(&matched_order.id);
                    }

                    trades.push(trade);
                }

                // Clean up empty price level
                if level.is_empty() {
                    opposite_side.remove(&price);
                }
            }
        }

        // Update statistics
        if !trades.is_empty() {
            let total_volume: u64 = trades.iter().map(|t| t.quantity).sum();
            self.total_trades
                .fetch_add(trades.len() as u64, Ordering::Relaxed);
            self.total_volume.fetch_add(total_volume, Ordering::Relaxed);

            // Update last trade price
            if let Some(last_trade) = trades.last() {
                self.last_trade_price
                    .store(last_trade.price, Ordering::Relaxed);
            }
        }

        Ok(trades)
    }

    fn add_order_to_book(&self, order: Order) -> Result<(), OrderBookError> {
        let price = order.price;
        let side = order.side;
        let order_id = order.id;

        // Choose the correct side of the book
        let price_levels = match side {
            Side::Buy => &self.bids,
            Side::Sell => &self.asks,
        };

        // Get or create price level
        let level = price_levels
            .entry(price)
            .or_insert_with(|| Arc::new(PriceLevel::new(price)))
            .clone();

        // Add order to price level
        level.add_order(order);

        // Track order location
        self.order_locations
            .insert(order_id, OrderLocation { price, side });

        Ok(())
    }

    fn next_sequence(&self) -> u64 {
        self.sequence_number.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct OrderBookStats {
    pub symbol: String,
    pub total_orders: usize,
    pub bid_levels: usize,
    pub ask_levels: usize,
    pub best_bid: Option<Price>,
    pub best_ask: Option<Price>,
    pub spread: Option<Price>,
    pub last_trade_price: Option<Price>,
    pub total_trades: u64,
    pub total_volume: u64,
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new("DEFAULT".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::types::{OrderStatus, OrderType};

    fn create_limit_order(side: Side, price: Price, quantity: Quantity) -> Order {
        Order::new_limit("TEST".to_string(), side, price, quantity, None)
    }

    fn create_market_order(side: Side, quantity: Quantity) -> Order {
        Order::new_market("TEST".to_string(), side, quantity, None)
    }

    #[test]
    fn test_empty_book() {
        let book = OrderBook::new("TEST".to_string());
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
        assert_eq!(book.spread(), None);
        assert_eq!(book.total_orders(), 0);
    }

    #[test]
    fn test_add_limit_orders() {
        let book = OrderBook::new("TEST".to_string());

        // Add buy order
        let buy_order = create_limit_order(Side::Buy, 10000, 100);
        let events = book.add_limit_order(buy_order).unwrap();
        assert_eq!(events.len(), 1);

        // Add sell order
        let sell_order = create_limit_order(Side::Sell, 10100, 100);
        let events = book.add_limit_order(sell_order).unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(book.best_bid(), Some(10000));
        assert_eq!(book.best_ask(), Some(10100));
        assert_eq!(book.spread(), Some(100));
        assert_eq!(book.total_orders(), 2);
    }

    #[test]
    fn test_order_matching() {
        let book = OrderBook::new("TEST".to_string());

        // Add sell order first
        let sell_order = create_limit_order(Side::Sell, 10000, 100);
        book.add_limit_order(sell_order).unwrap();

        // Add buy order that matches
        let buy_order = create_limit_order(Side::Buy, 10000, 50);
        let events = book.add_limit_order(buy_order).unwrap();

        // Should have one trade event
        assert_eq!(events.len(), 1);
        if let MarketEvent::Trade { trade } = &events[0] {
            assert_eq!(trade.price, 10000);
            assert_eq!(trade.quantity, 50);
        } else {
            panic!("Expected trade event");
        }

        // Sell order should have remaining quantity
        assert_eq!(book.total_orders(), 1);
        assert_eq!(book.best_ask(), Some(10000));
    }

    #[test]
    fn test_market_order() {
        let book = OrderBook::new("TEST".to_string());

        // Add some limit orders for liquidity
        book.add_limit_order(create_limit_order(Side::Sell, 10000, 50))
            .unwrap();
        book.add_limit_order(create_limit_order(Side::Sell, 10100, 50))
            .unwrap();

        // Add market buy order
        let market_order = create_market_order(Side::Buy, 75);
        let events = book.add_market_order(market_order).unwrap();

        // Should have two trade events (fills both levels partially)
        assert_eq!(events.len(), 2);

        // First trade at 10000 for 50 shares
        if let MarketEvent::Trade { trade } = &events[0] {
            assert_eq!(trade.price, 10000);
            assert_eq!(trade.quantity, 50);
        }

        // Second trade at 10100 for 25 shares
        if let MarketEvent::Trade { trade } = &events[1] {
            assert_eq!(trade.price, 10100);
            assert_eq!(trade.quantity, 25);
        }
    }

    #[test]
    fn test_cancel_order() {
        let book = OrderBook::new("TEST".to_string());

        let order = create_limit_order(Side::Buy, 10000, 100);
        let order_id = order.id;

        book.add_limit_order(order).unwrap();
        assert_eq!(book.total_orders(), 1);

        let event = book.cancel_order(&order_id).unwrap();
        if let MarketEvent::OrderCancelled {
            order_id: cancelled_id,
            remaining_quantity,
        } = event
        {
            assert_eq!(cancelled_id, order_id);
            assert_eq!(remaining_quantity, 100);
        }

        assert_eq!(book.total_orders(), 0);
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn test_modify_order_quantity() {
        let book = OrderBook::new("TEST".to_string());

        let order = create_limit_order(Side::Buy, 10000, 100);
        let order_id = order.id;

        book.add_limit_order(order).unwrap();

        let event = book.modify_order_quantity(&order_id, 150).unwrap();
        if let MarketEvent::OrderModified {
            order_id: modified_id,
            new_quantity,
            ..
        } = event
        {
            assert_eq!(modified_id, order_id);
            assert_eq!(new_quantity, Some(150));
        }
    }

    #[test]
    fn test_price_time_priority() {
        let book = OrderBook::new("TEST".to_string());

        // Add two buy orders at same price
        let order1 = create_limit_order(Side::Buy, 10000, 100);
        let order2 = create_limit_order(Side::Buy, 10000, 200);

        book.add_limit_order(order1).unwrap();
        book.add_limit_order(order2).unwrap();

        // Add sell order that partially matches
        let sell_order = create_limit_order(Side::Sell, 10000, 150);
        let events = book.add_limit_order(sell_order).unwrap();

        // Should trade with first order completely (100) and second order partially (50)
        assert_eq!(events.len(), 1);
        if let MarketEvent::Trade { trade } = &events[0] {
            assert_eq!(trade.quantity, 150);
        }
    }
}
