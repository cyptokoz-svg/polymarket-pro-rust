//! Position tracking

use crate::api::Side;
use std::collections::HashMap;

/// Position data
#[derive(Debug, Clone)]
pub struct Position {
    pub market_id: String,
    pub side: Side,
    pub total_size: f64,
    pub avg_price: f64,
    pub entries: Vec<PositionEntry>,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            market_id: String::new(),
            side: Side::Buy, // Default to Buy
            total_size: 0.0,
            avg_price: 0.0,
            entries: Vec::new(),
        }
    }
}

/// Individual position entry
#[derive(Debug, Clone)]
pub struct PositionEntry {
    pub size: f64,
    pub price: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Inventory status summary
#[derive(Debug, Clone)]
pub struct InventoryStatus {
    pub up_value: f64,
    pub down_value: f64,
    pub total_value: f64,
    pub skew: f64,
    pub is_balanced: bool,
    pub recommendation: String,
}

/// Balance adjustment action
#[derive(Debug, Clone)]
pub enum Action {
    BalancedEntry,
    ReduceUp,
    ReduceDown,
    BuyUp,
    BuyDown,
}

/// Balance adjustment recommendation
#[derive(Debug, Clone)]
pub struct BalanceAdjustment {
    pub action: Action,
    pub amount: f64,
    pub reason: String,
}

/// Position tracker
pub struct PositionTracker {
    positions: HashMap<String, Position>,
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }
    
    /// Update position with new trade
    pub async fn update_position(
        &mut self,
        market_id: &str,
        side: Side,
        size: f64,
        price: f64,
    ) {
        let entry = PositionEntry {
            size,
            price,
            timestamp: chrono::Utc::now(),
        };
        
        self.positions
            .entry(market_id.to_string())
            .and_modify(|pos| {
                // Update existing position
                if pos.side == side {
                    // Adding to same side
                    let total_value = pos.total_size * pos.avg_price + size * price;
                    pos.total_size += size;
                    pos.avg_price = total_value / pos.total_size;
                } else {
                    // Reducing position or flipping
                    if size >= pos.total_size {
                        // Flip side
                        pos.side = side;
                        pos.total_size = size - pos.total_size;
                        pos.avg_price = price;
                    } else {
                        // Reduce position
                        pos.total_size -= size;
                    }
                }
                pos.entries.push(entry.clone());
            })
            .or_insert(Position {
                market_id: market_id.to_string(),
                side,
                total_size: size,
                avg_price: price,
                entries: vec![entry],
            });
    }
    
    /// Get position for a market
    pub async fn get_position(
        &self,
        market_id: &str,
    ) -> Option<&Position> {
        self.positions.get(market_id)
    }
    
    /// Get all positions
    pub async fn get_all_positions(&self) -> &HashMap<String, Position> {
        &self.positions
    }
    
    /// Clear position for a market (e.g., after redemption)
    pub async fn clear_position(
        &mut self,
        market_id: &str,
    ) {
        self.positions.remove(market_id);
    }
    
    /// Get total exposure across all markets
    pub async fn get_total_exposure(&self) -> f64 {
        self.positions.values().map(|p| p.total_size * p.avg_price).sum()
    }

    /// Calculate inventory skew (-1 to 1, positive means more long positions)
    /// Matches Python: calculate_inventory_skew()
    pub async fn calculate_inventory_skew(&self) -> f64 {
        let mut up_value = 0.0;
        let mut down_value = 0.0;

        for pos in self.positions.values() {
            let value = pos.total_size * pos.avg_price;
            match pos.side {
                Side::Buy => up_value += value,
                Side::Sell => down_value += value,
                _ => {}, // Handle any future variants
            }
        }

        let total = up_value + down_value;
        if total == 0.0 {
            0.0
        } else {
            (up_value - down_value) / total
        }
    }

    /// Get inventory status summary
    /// Matches Python: get_inventory_status()
    pub async fn get_inventory_status(&self) -> InventoryStatus {
        let mut up_value = 0.0;
        let mut down_value = 0.0;

        for pos in self.positions.values() {
            let value = pos.total_size * pos.avg_price;
            match pos.side {
                Side::Buy => up_value += value,
                Side::Sell => down_value += value,
                _ => {}, // Handle any future variants
            }
        }

        let total = up_value + down_value;
        let skew = if total == 0.0 {
            0.0
        } else {
            (up_value - down_value) / total
        };

        // Consider balanced if skew < 30%
        let is_balanced = skew.abs() < 0.3;

        let recommendation = if skew > 0.5 {
            format!("UP position too large ({:.1}%), reduce UP or add DOWN", skew * 100.0)
        } else if skew < -0.5 {
            format!("DOWN position too large ({:.1}%), reduce DOWN or add UP", skew.abs() * 100.0)
        } else if skew > 0.3 {
            format!("UP slightly overweight ({:.1}%), consider balancing", skew * 100.0)
        } else if skew < -0.3 {
            format!("DOWN slightly overweight ({:.1}%), consider balancing", skew.abs() * 100.0)
        } else {
            "Portfolio balanced".to_string()
        };

        InventoryStatus {
            up_value,
            down_value,
            total_value: total,
            skew,
            is_balanced,
            recommendation,
        }
    }

    /// Check merge opportunity
    /// Matches Python: check_merge_opportunity()
    pub fn check_merge_opportunity(
        &self,
        market_id: &str,
        merge_threshold: f64,
    ) -> Option<f64> {
        // Get positions for this market
        let up_pos = self.positions.values().find(|p| {
            p.market_id == market_id && matches!(p.side, Side::Buy)
        });
        
        let down_pos = self.positions.values().find(|p| {
            p.market_id == market_id && matches!(p.side, Side::Sell)
        });

        if let (Some(up), Some(down)) = (up_pos, down_pos) {
            if up.total_size > 0.0 && down.total_size > 0.0 {
                let merge_amount = up.total_size.min(down.total_size);
                if merge_amount >= merge_threshold {
                    return Some(merge_amount);
                }
            }
        }

        None
    }

    /// Check if should skip trading on one side
    /// Matches Python: should_skip_side()
    pub async fn should_skip_side(
        &self,
        side: Side,
    ) -> (bool, String) {
        let skew = self.calculate_inventory_skew().await;

        // Skip buying UP if already too much UP
        if matches!(side, Side::Buy) && skew > 0.7 {
            return (true, format!("UP inventory too high ({:.1}%), skip buying UP", skew * 100.0));
        }

        // Skip buying DOWN (selling) if already too much DOWN
        if matches!(side, Side::Sell) && skew < -0.7 {
            return (true, format!("DOWN inventory too high ({:.1}%), skip selling", skew.abs() * 100.0));
        }

        (false, "OK to trade".to_string())
    }

    /// Get dynamic position limit for a side
    /// Matches Python: get_position_limit()
    pub async fn get_position_limit(
        &self,
        side: Side,
        base_max_position: f64,
    ) -> f64 {
        let skew = self.calculate_inventory_skew().await;
        let base_limit = base_max_position / 2.0; // Base limit is half of max

        match side {
            Side::Buy => {
                if skew > 0.5 {
                    // UP already high, stricter limit
                    base_limit * (1.0 - skew)
                } else {
                    // UP low, allow more
                    base_limit * (1.0 + skew.abs())
                }
            }
            Side::Sell => {
                if skew < -0.5 {
                    // DOWN already high, stricter limit
                    base_limit * (1.0 - skew.abs())
                } else {
                    // DOWN low, allow more
                    base_limit * (1.0 + skew)
                }
            }
            _ => base_limit, // Handle any future variants
        }
    }

    /// Calculate balance adjustment recommendation
    /// Matches Python: calculate_balance_adjustment()
    pub async fn calculate_balance_adjustment(
        &self,
        order_size: f64,
        imbalance_threshold: f64,
    ) -> Option<BalanceAdjustment> {
        let status = self.get_inventory_status().await;
        let skew = status.skew;
        let total = status.total_value;

        // No positions, suggest balanced entry
        if total == 0.0 {
            return Some(BalanceAdjustment {
                action: Action::BalancedEntry,
                amount: order_size,
                reason: "No positions, enter balanced".to_string(),
            });
        }

        // UP overweight, suggest reducing UP or adding DOWN
        if skew > imbalance_threshold {
            let up_positions: Vec<_> = self.positions.values()
                .filter(|p| matches!(p.side, Side::Buy))
                .collect();
            
            if !up_positions.is_empty() {
                let total_up: f64 = up_positions.iter().map(|p| p.total_size).sum();
                let reduce_amount = (total_up * 0.3).min(order_size);
                return Some(BalanceAdjustment {
                    action: Action::ReduceUp,
                    amount: reduce_amount,
                    reason: format!("UP overweight ({:.1}%), reduce position", skew * 100.0),
                });
            } else {
                return Some(BalanceAdjustment {
                    action: Action::BuyDown,
                    amount: order_size,
                    reason: format!("UP overweight ({:.1}%), hedge with DOWN", skew * 100.0),
                });
            }
        }

        // DOWN overweight, suggest reducing DOWN or adding UP
        if skew < -imbalance_threshold {
            let down_positions: Vec<_> = self.positions.values()
                .filter(|p| matches!(p.side, Side::Sell))
                .collect();
            
            if !down_positions.is_empty() {
                let total_down: f64 = down_positions.iter().map(|p| p.total_size).sum();
                let reduce_amount = (total_down * 0.3).min(order_size);
                return Some(BalanceAdjustment {
                    action: Action::ReduceDown,
                    amount: reduce_amount,
                    reason: format!("DOWN overweight ({:.1}%), reduce position", skew.abs() * 100.0),
                });
            } else {
                return Some(BalanceAdjustment {
                    action: Action::BuyUp,
                    amount: order_size,
                    reason: format!("DOWN overweight ({:.1}%), hedge with UP", skew.abs() * 100.0),
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_position() {
        let mut tracker = PositionTracker::new();
        tracker.update_position("market_1", Side::Buy, 5.0, 0.5).await;

        let pos = tracker.get_position("market_1").await.unwrap();
        assert_eq!(pos.total_size, 5.0);
        assert_eq!(pos.avg_price, 0.5);
        assert_eq!(pos.side, Side::Buy);
    }

    #[tokio::test]
    async fn test_add_to_same_side() {
        let mut tracker = PositionTracker::new();
        tracker.update_position("market_1", Side::Buy, 5.0, 0.5).await;
        tracker.update_position("market_1", Side::Buy, 5.0, 0.6).await;

        let pos = tracker.get_position("market_1").await.unwrap();
        assert_eq!(pos.total_size, 10.0);
        // Average price: (5*0.5 + 5*0.6) / 10 = 0.55
        assert!((pos.avg_price - 0.55).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_reduce_position() {
        let mut tracker = PositionTracker::new();
        tracker.update_position("market_1", Side::Buy, 10.0, 0.5).await;
        tracker.update_position("market_1", Side::Sell, 3.0, 0.6).await;

        let pos = tracker.get_position("market_1").await.unwrap();
        assert_eq!(pos.total_size, 7.0);
        assert_eq!(pos.side, Side::Buy);
    }

    #[tokio::test]
    async fn test_flip_position() {
        let mut tracker = PositionTracker::new();
        tracker.update_position("market_1", Side::Buy, 5.0, 0.5).await;
        tracker.update_position("market_1", Side::Sell, 8.0, 0.6).await;

        let pos = tracker.get_position("market_1").await.unwrap();
        assert_eq!(pos.total_size, 3.0);
        assert_eq!(pos.side, Side::Sell);
        assert_eq!(pos.avg_price, 0.6);
    }

    #[tokio::test]
    async fn test_clear_position() {
        let mut tracker = PositionTracker::new();
        tracker.update_position("market_1", Side::Buy, 5.0, 0.5).await;
        tracker.clear_position("market_1").await;

        assert!(tracker.get_position("market_1").await.is_none());
    }

    #[tokio::test]
    async fn test_total_exposure() {
        let mut tracker = PositionTracker::new();
        tracker.update_position("market_1", Side::Buy, 10.0, 0.5).await; // 5.0 exposure
        tracker.update_position("market_2", Side::Buy, 5.0, 0.6).await;  // 3.0 exposure

        let exposure = tracker.get_total_exposure().await;
        assert!((exposure - 8.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_multiple_markets() {
        let mut tracker = PositionTracker::new();
        tracker.update_position("market_1", Side::Buy, 5.0, 0.5).await;
        tracker.update_position("market_2", Side::Sell, 3.0, 0.6).await;

        let all = tracker.get_all_positions().await;
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("market_1"));
        assert!(all.contains_key("market_2"));
    }
}