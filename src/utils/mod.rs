pub mod time;

/// Convert price from ticks to human-readable format
pub fn format_price(price_ticks: u64, tick_size: f64) -> String {
    format!("${:.2}", price_ticks as f64 * tick_size)
}

/// Convert human price to ticks
pub fn price_to_ticks(price: f64, tick_size: f64) -> u64 {
    (price / tick_size).round() as u64
}

/// Generate a simple hash for price levels (for benchmarking)
pub fn price_hash(price: u64) -> u64 {
    // Simple hash function for price distribution
    price.wrapping_mul(0x9E3779B97F4A7C15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_price() {
        assert_eq!(format_price(10000, 0.01), "$100.00");
        assert_eq!(format_price(12550, 0.01), "$125.50");
    }

    #[test]
    fn test_price_to_ticks() {
        assert_eq!(price_to_ticks(100.0, 0.01), 10000);
        assert_eq!(price_to_ticks(125.50, 0.01), 12550);
    }
}
