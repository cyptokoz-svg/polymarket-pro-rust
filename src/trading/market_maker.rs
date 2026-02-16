//! Market maker implementation
//! 5-minute cycle automated market making strategy

use crate::api::{ClobClient, Order, OrderResponse};
use crate::api::Side;
use crate::trading::TradingError;

/// Market maker configuration
#[derive(Debug, Clone)]
pub struct MarketMakerConfig {
    /// Order size in shares
    pub order_size: f64,
    /// Maximum position size
    pub max_position: f64,
    /// Safe price range lower bound
    pub safe_range_low: f64,
    /// Safe price range upper bound
    pub safe_range_high: f64,
    /// Balance buffer percentage (0.15 = 15%)
    pub balance_buffer: f64,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            order_size: 5.0,
            max_position: 10.0,
            safe_range_low: 0.1,
            safe_range_high: 0.9,
            balance_buffer: 0.15,
        }
    }
}

/// Market maker for a specific market
pub struct MarketMaker {
    market_id: String,
    config: MarketMakerConfig,
}

impl MarketMaker {
    /// Create new market maker
    pub fn new(market_id: String, config: MarketMakerConfig) -> Self {
        Self { market_id, config }
    }
    
    /// Check if can trade given current balance
    pub async fn can_trade(&self,
        balance_usdc: f64,
    ) -> bool {
        let required = self.config.order_size * (1.0 + self.config.balance_buffer);
        balance_usdc >= required
    }
    
    /// Calculate order size based on position imbalance
    pub fn calculate_order_size(
        &self,
        current_position: f64,
        target_side: Side,
    ) -> Result<f64, TradingError> {
        let abs_position = current_position.abs();
        
        // If position exceeds max, don't add more
        if abs_position >= self.config.max_position {
            return Err(TradingError::PositionLimitExceeded {
                current: abs_position,
                new: 0.0,
                max: self.config.max_position,
            });
        }
        
        // Calculate remaining capacity
        let remaining = self.config.max_position - abs_position;
        let size = self.config.order_size.min(remaining);
        
        // If position is imbalanced, only trade the imbalanced side
        match target_side {
            Side::Buy if current_position < 0.0 => Ok(size), // Short position, buy to close
            Side::Sell if current_position > 0.0 => Ok(size), // Long position, sell to close
            _ if current_position == 0.0 => Ok(size), // No position, normal size
            _ => Ok(size / 2.0), // Imbalanced, reduce size
        }
    }
    
    /// Validate price is within safe range
    pub fn validate_price(&self,
        price: f64,
    ) -> Result<(), TradingError> {
        if price < self.config.safe_range_low || price > self.config.safe_range_high {
            return Err(TradingError::PriceOutOfRange { price });
        }
        Ok(())
    }
    
    /// Generate market making orders
    pub fn generate_orders(
        &self,
        current_price: f64,
        spread: f64,
    ) -> Result<Vec<Order>, TradingError> {
        self.validate_price(current_price)?;
        
        let bid_price = (current_price - spread / 2.0).max(self.config.safe_range_low);
        let ask_price = (current_price + spread / 2.0).min(self.config.safe_range_high);
        
        let orders = vec![
            Order {
                market_id: self.market_id.clone(),
                side: Side::Buy,
                size: self.config.order_size,
                price: bid_price,
            },
            Order {
                market_id: self.market_id.clone(),
                side: Side::Sell,
                size: self.config.order_size,
                price: ask_price,
            },
        ];
        
        Ok(orders)
    }
}

/// Market maker execution engine
pub struct MarketMakerEngine<C: ClobClient> {
    market_maker: MarketMaker,
    client: C,
}

impl<C: ClobClient> MarketMakerEngine<C> {
    /// Create new engine
    pub fn new(market_maker: MarketMaker, client: C) -> Self {
        Self { market_maker, client }
    }
    
    /// Execute one market making cycle
    pub async fn execute_cycle(
        &self,
        current_price: f64,
        balance_usdc: f64,
    ) -> Result<Vec<OrderResponse>, TradingError> {
        // Check if can trade
        if !self.market_maker.can_trade(balance_usdc).await {
            return Err(TradingError::InsufficientBalance {
                available: balance_usdc,
                required: self.market_maker.config.order_size,
            });
        }
        
        // Generate orders
        let orders = self.market_maker.generate_orders(current_price, 0.02)?;
        
        // Place orders asynchronously
        let mut responses = Vec::new();
        for order in orders {
            match self.client.place_order(order).await {
                Ok(resp) => responses.push(resp),
                Err(e) => tracing::error!("Failed to place order: {}", e),
            }
        }
        
        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> MarketMakerConfig {
        MarketMakerConfig {
            order_size: 5.0,
            max_position: 10.0,
            safe_range_low: 0.1,
            safe_range_high: 0.9,
            balance_buffer: 0.15,
        }
    }

    #[tokio::test]
    async fn test_can_trade_with_sufficient_balance() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        // Need 5.0 * 1.15 = 5.75 USDC
        assert!(mm.can_trade(6.0).await);
    }

    #[tokio::test]
    async fn test_can_trade_with_insufficient_balance() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        // Need 5.0 * 1.15 = 5.75 USDC
        assert!(!mm.can_trade(5.0).await);
    }

    #[test]
    fn test_validate_price_within_range() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        assert!(mm.validate_price(0.5).is_ok());
        assert!(mm.validate_price(0.1).is_ok());
        assert!(mm.validate_price(0.9).is_ok());
    }

    #[test]
    fn test_validate_price_outside_range() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        assert!(mm.validate_price(0.05).is_err());
        assert!(mm.validate_price(0.95).is_err());
    }

    #[test]
    fn test_generate_orders() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        let orders = mm.generate_orders(0.5, 0.02).unwrap();
        
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].side, Side::Buy);
        assert_eq!(orders[1].side, Side::Sell);
        assert_eq!(orders[0].size, 5.0);
        assert_eq!(orders[1].size, 5.0);
    }

    #[test]
    fn test_calculate_order_size_no_position() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        let size = mm.calculate_order_size(0.0, Side::Buy).unwrap();
        assert_eq!(size, 5.0);
    }

    #[test]
    fn test_calculate_order_size_at_limit() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        let result = mm.calculate_order_size(10.0, Side::Buy);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_order_size_partial() {
        let mm = MarketMaker::new("0xabc".to_string(), create_test_config());
        // Position at 7 (Buy side), max is 10, remaining is 3
        // Same side addition returns size / 2 for risk control
        let size = mm.calculate_order_size(7.0, Side::Buy).unwrap();
        assert_eq!(size, 1.5); // 3.0 / 2 = 1.5
    }
}