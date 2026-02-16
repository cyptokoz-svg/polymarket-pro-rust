//! Integration tests for Polymarket Pro

use polymarket_pro::*;

// Test configuration loading
#[test]
fn test_config_default() {
    let config = config::Config::default();
    assert_eq!(config.trading.order_size, 1.0);  // Changed to match Python
    assert_eq!(config.trading.max_position, 5.0);  // Changed to match Python
    assert_eq!(config.trading.refresh_interval, 45);
}

#[test]
fn test_config_validation() {
    let mut config = config::Config::default();
    // Use valid 64-char hex private key (with 0x prefix = 66 chars)
    config.pk = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string();
    // Use valid 40-char hex address (with 0x prefix = 42 chars)
    config.safe_address = "0x1234567890abcdef1234567890abcdef12345678".to_string();
    // Set BROWSER_ADDRESS env var for validation
    std::env::set_var("BROWSER_ADDRESS", "0xabc");
    
    // Should pass validation
    assert!(config.validate().is_ok());
    
    // Invalid range
    config.trading.safe_range_low = 0.9;
    config.trading.safe_range_high = 0.1;
    assert!(config.validate().is_err());
}

// Test retry mechanism
#[tokio::test]
async fn test_retry_success() {
    use polymarket_pro::utils::retry::{retry_with_backoff, RetryConfig};
    
    let counter = std::sync::atomic::AtomicU32::new(0);
    
    let result = retry_with_backoff(
        "test",
        RetryConfig::new(3, 10),
        || async {
            let count = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count < 2 {
                Err::<&str, &str>("fail")
            } else {
                Ok("success")
            }
        },
    ).await;
    
    assert_eq!(result.unwrap(), "success");
}

// Test position tracking
#[tokio::test]
async fn test_position_tracker_integration() {
    let mut tracker = PositionTracker::new();
    
    // Add multiple positions
    tracker.update_position("market_1", Side::Buy, 10.0, 0.5).await;
    tracker.update_position("market_2", Side::Sell, 5.0, 0.6).await;
    tracker.update_position("market_1", Side::Buy, 5.0, 0.55).await;
    
    // Check positions
    let pos1 = tracker.get_position("market_1").await.unwrap();
    assert_eq!(pos1.total_size, 15.0);
    
    let pos2 = tracker.get_position("market_2").await.unwrap();
    assert_eq!(pos2.total_size, 5.0);
    
    // Check total exposure
    let exposure = tracker.get_total_exposure().await;
    assert!(exposure > 0.0);
}

// Test market maker with different scenarios
#[test]
fn test_market_maker_scenarios() {
    let config = MarketMakerConfig {
        order_size: 5.0,
        max_position: 10.0,
        safe_range_low: 0.1,
        safe_range_high: 0.9,
        balance_buffer: 0.15,
    };
    
    let mm = MarketMaker::new("0xabc".to_string(), config);
    
    // Test 1: Normal price
    let orders = mm.generate_orders(0.5, 0.02).unwrap();
    assert_eq!(orders.len(), 2);
    
    // Test 2: Price at boundary
    let orders = mm.generate_orders(0.1, 0.02).unwrap();
    assert_eq!(orders[0].price, 0.1); // Bid clamped to low
    
    // Test 3: Price out of range
    assert!(mm.generate_orders(0.05, 0.02).is_err());
    assert!(mm.generate_orders(0.95, 0.02).is_err());
}

// Test wallet integration
#[tokio::test]
async fn test_wallet_signing_integration() {
    let pk = "***REMOVED***";
    let wallet = PrivateKeyWallet::from_private_key(pk, 137).unwrap();
    
    // Sign message
    let message = b"test message";
    let signature = wallet.sign_message(message).await.unwrap();
    
    assert_eq!(signature.len(), 65);
    
    // Address should be consistent
    let addr1 = wallet.address();
    let addr2 = wallet.address();
    assert_eq!(addr1, addr2);
}

// Test Safe wallet integration
#[test]
fn test_safe_wallet_integration() {
    let safe_addr = "0x45dCeb24119296fB57D06d83c1759cC191c3c96E";
    let owner = "0xB18Ec66081b444037F7C1B5ffEE228693B854E7A";
    
    let safe = SafeWallet::new(safe_addr, owner).unwrap();
    
    assert!(safe.is_owner_valid());
    assert_eq!(safe.nonce(), 0);
    
    // Test nonce increment
    let mut safe = safe;
    safe.increment_nonce();
    assert_eq!(safe.nonce(), 1);
}

// Test WebSocket price cache
#[tokio::test]
async fn test_websocket_price_cache() {
    let ws = PolymarketWebSocket::new();
    
    // Initially empty
    let price = ws.get_price("market_1").await;
    assert!(price.is_none());
    
    // Note: Actual WebSocket connection test would require network
    // This is just a structure test
}

// Test trading config
#[test]
fn test_trading_config() {
    let config = TradingConfig {
        order_size: 10.0,
        max_position: 20.0,
        max_total_position: 30.0,
        max_spread: 0.02,
        min_spread: 0.005,
        merge_threshold: 0.5,
        max_hold_time: 180,
        exit_before_expiry: 120,
        take_profit: 0.03,
        stop_loss: 0.05,
        depth_lookback: 5,
        imbalance_threshold: 0.3,
        min_price: 0.01,
        max_price: 0.99,
        safe_range_low: 0.2,
        safe_range_high: 0.8,
        price_warn_cooldown: 60,
        refresh_interval: 45,
        spread: 0.02,
    };
    
    assert_eq!(config.order_size, 10.0);
    assert_eq!(config.max_position, 20.0);
    assert!(config.safe_range_low < config.safe_range_high);
}

// Test error types
#[test]
fn test_error_types() {
    use polymarket_pro::trading::TradingError;
    
    let err = TradingError::InsufficientBalance {
        available: 5.0,
        required: 10.0,
    };
    
    let msg = err.to_string();
    assert!(msg.contains("5"));
    assert!(msg.contains("10"));
}

// Test redeem structures
#[test]
fn test_redeem_structures() {
    use polymarket_pro::redeem::{SettledMarket, RedeemResult};
    
    let market = SettledMarket {
        condition_id: "0xabc".to_string(),
        amount: 1000000,
        outcome: "Yes".to_string(),
    };
    
    assert_eq!(market.condition_id, "0xabc");
    assert_eq!(market.amount, 1000000);
    
    let result = RedeemResult {
        condition_id: "0xabc".to_string(),
        success: true,
        transaction_hash: Some("0x123".to_string()),
        error: None,
    };
    
    assert!(result.success);
    assert!(result.transaction_hash.is_some());
}