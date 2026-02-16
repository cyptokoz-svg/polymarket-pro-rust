//! Order book depth analysis
//! Matches Python polymaker_5m.py logic


/// Order book level
#[derive(Debug, Clone)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

/// Order book depth analysis
#[derive(Debug, Clone)]
pub struct OrderBookDepth {
    pub best_bid: OrderBookLevel,
    pub best_ask: OrderBookLevel,
    pub second_bid: OrderBookLevel,
    pub second_ask: OrderBookLevel,
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub imbalance: f64,
}

impl OrderBookDepth {
    /// Calculate mid price
    pub fn mid_price(&self) -> f64 {
        (self.best_bid.price + self.best_ask.price) / 2.0
    }

    /// Calculate spread
    pub fn spread(&self) -> f64 {
        self.best_ask.price - self.best_bid.price
    }

    /// Calculate spread percentage
    pub fn spread_pct(&self) -> f64 {
        let mid = self.mid_price();
        if mid > 0.0 {
            self.spread() / mid
        } else {
            0.0
        }
    }
}

/// Analyze order book depth with fallback
/// Returns None if data is insufficient to avoid using default prices
pub fn analyze_order_book_depth_safe(
    bids: &[serde_json::Value],
    asks: &[serde_json::Value],
    min_size: f64,
    depth_lookback: usize,
) -> Option<OrderBookDepth> {
    // Parse and filter bids
    let mut parsed_bids: Vec<OrderBookLevel> = bids
        .iter()
        .take(depth_lookback)
        .filter_map(|b| {
            let price = b.get("price")?.as_str()?.parse::<f64>().ok()?;
            let size = b.get("size")?.as_str()?.parse::<f64>().ok()?;
            if size >= min_size {
                Some(OrderBookLevel { price, size })
            } else {
                None
            }
        })
        .collect();

    // Parse and filter asks
    let mut parsed_asks: Vec<OrderBookLevel> = asks
        .iter()
        .take(depth_lookback)
        .filter_map(|a| {
            let price = a.get("price")?.as_str()?.parse::<f64>().ok()?;
            let size = a.get("size")?.as_str()?.parse::<f64>().ok()?;
            if size >= min_size {
                Some(OrderBookLevel { price, size })
            } else {
                None
            }
        })
        .collect();

    // Check if we have enough data
    if parsed_bids.len() < 2 || parsed_asks.len() < 2 {
        // Return None to indicate insufficient data
        // Caller should fallback to WebSocket/API price
        return None;
    }

    // Sort bids descending, asks ascending
    parsed_bids.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap_or(std::cmp::Ordering::Equal));
    parsed_asks.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap_or(std::cmp::Ordering::Equal));

    let best_bid = parsed_bids[0].clone();
    let best_ask = parsed_asks[0].clone();
    let second_bid = parsed_bids.get(1).cloned().unwrap_or_else(|| best_bid.clone());
    let second_ask = parsed_asks.get(1).cloned().unwrap_or_else(|| best_ask.clone());

    let bid_depth: f64 = parsed_bids.iter().map(|b| b.size).sum();
    let ask_depth: f64 = parsed_asks.iter().map(|a| a.size).sum();

    // Calculate imbalance (-1 to 1, positive means more asks)
    let total_depth = bid_depth + ask_depth;
    let imbalance = if total_depth > 0.0 {
        (ask_depth - bid_depth) / total_depth
    } else {
        0.0
    };

    Some(OrderBookDepth {
        best_bid,
        best_ask,
        second_bid,
        second_ask,
        bid_depth,
        ask_depth,
        imbalance,
    })
}

/// Calculate market making prices with depth analysis
pub fn calculate_mm_prices(
    depth: &OrderBookDepth,
    inventory_skew: f64,
    min_spread: f64,
    max_spread: f64,
) -> (f64, f64) {
    let mid = depth.mid_price();
    let spread = depth.spread().clamp(min_spread, max_spread);

    // Base half spread
    let half_spread = spread / 2.0;

    // Inventory adjustment (positive skew = long, lower bid/ask)
    let inventory_adjust = inventory_skew * 0.01; // 1% adjustment

    // Imbalance adjustment (more asks = negative imbalance, raise bid to attract sellers)
    let imbalance_adjust = -depth.imbalance * 0.005; // 0.5% adjustment

    // Calculate final prices
    let bid_price = mid - half_spread + inventory_adjust + imbalance_adjust;
    let ask_price = mid + half_spread + inventory_adjust + imbalance_adjust;

    // Clamp to valid range
    let bid_price = bid_price.clamp(0.01, 0.99);
    let ask_price = ask_price.clamp(0.01, 0.99);

    (round_price(bid_price), round_price(ask_price))
}

/// Round price to 4 decimal places
fn round_price(price: f64) -> f64 {
    (price * 10000.0).round() / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_book_depth_safe_sufficient_data() {
        let bids = vec![
            serde_json::json!({"price": "0.52", "size": "100"}),
            serde_json::json!({"price": "0.51", "size": "200"}),
        ];
        let asks = vec![
            serde_json::json!({"price": "0.54", "size": "150"}),
            serde_json::json!({"price": "0.55", "size": "100"}),
        ];
        
        let result = analyze_order_book_depth_safe(&bids, &asks, 10.0, 5);
        assert!(result.is_some());
        
        let depth = result.unwrap();
        assert!((depth.best_bid.price - 0.52).abs() < 0.001);
        assert!((depth.best_ask.price - 0.54).abs() < 0.001);
        assert!((depth.mid_price() - 0.53).abs() < 0.001);
        assert!((depth.spread() - 0.02).abs() < 0.001);
    }
    
    #[test]
    fn test_order_book_depth_safe_insufficient_data() {
        let bids = vec![
            serde_json::json!({"price": "0.52", "size": "100"}),
            // Only 1 bid
        ];
        let asks = vec![
            serde_json::json!({"price": "0.54", "size": "150"}),
            serde_json::json!({"price": "0.55", "size": "100"}),
        ];
        
        // Should return None for insufficient data
        let result = analyze_order_book_depth_safe(&bids, &asks, 10.0, 5);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_mm_prices() {
        let depth = OrderBookDepth {
            best_bid: OrderBookLevel { price: 0.52, size: 100.0 },
            best_ask: OrderBookLevel { price: 0.54, size: 150.0 },
            second_bid: OrderBookLevel { price: 0.51, size: 200.0 },
            second_ask: OrderBookLevel { price: 0.55, size: 100.0 },
            bid_depth: 300.0,
            ask_depth: 250.0,
            imbalance: -0.1,
        };

        let (bid, ask) = calculate_mm_prices(&depth, 0.0, 0.005, 0.02);

        assert!(bid < ask);
        assert!(bid >= 0.01 && bid <= 0.99);
        assert!(ask >= 0.01 && ask <= 0.99);
    }
}