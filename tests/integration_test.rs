//! Integration tests for Polymarket Pro
//! Tests the complete trading flow with mocked APIs

use polymarket_pro::*;
use polymarket_pro::api::Side;
use std::sync::Arc;
use tokio::sync::RwLock;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

/// Setup mock Polymarket API server
async fn setup_mock_api() -> MockServer {
    let mock_server = MockServer::start().await;

    // Mock server time endpoint
    Mock::given(method("GET"))
        .and(path("/time"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({
                "server_time": 1704067200000u64
            })))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Test complete trading cycle flow
#[tokio::test]
async fn test_trading_cycle_flow() {
    // Setup
    let position_tracker = Arc::new(RwLock::new(PositionTracker::new()));
    let order_tracker = Arc::new(RwLock::new(OrderTracker::new()));
    let trade_history = Arc::new(TradeHistory::default());
    let stats = Arc::new(RwLock::new(TradingStats::new()));

    // Test inventory skew calculation
    {
        let tracker = position_tracker.read().await;
        let skew = tracker.calculate_inventory_skew().await;
        assert_eq!(skew, 0.0, "Initial skew should be 0");
    }

    // Test position update
    {
        let mut tracker = position_tracker.write().await;
        tracker.update_position("market_1", Side::Buy, 1.0, 0.5).await;
    }

    // Verify position
    {
        let tracker = position_tracker.read().await;
        let skew = tracker.calculate_inventory_skew().await;
        assert!(skew > 0.0, "Skew should be positive after buying");
    }
}

/// Test order tracking
#[tokio::test]
async fn test_order_tracking() {
    let mut tracker = OrderTracker::new();

    // Track an order
    tracker.track_order(
        "token_1".to_string(),
        "order_123".to_string(),
        "BUY".to_string(),
        0.5,
        1.0,
    );

    assert_eq!(tracker.count(), 1);

    // Get the order
    let order = tracker.get_order("token_1").unwrap();
    assert_eq!(order.order_id, "order_123");
    assert_eq!(order.side, "BUY");

    // Remove the order
    tracker.remove_order("token_1");
    assert_eq!(tracker.count(), 0);
}

/// Test inventory status calculation
#[tokio::test]
async fn test_inventory_status() {
    let mut tracker = PositionTracker::new();

    // Add some positions
    tracker.update_position("market_1", Side::Buy, 1.0, 0.6).await;
    tracker.update_position("market_2", Side::Sell, 0.5, 0.4).await;

    let status = tracker.get_inventory_status().await;

    assert!(status.up_value > 0.0);
    assert!(status.down_value > 0.0);
    assert!(status.total_value > 0.0);
}

/// Test should skip side logic
#[tokio::test]
async fn test_should_skip_side() {
    let mut tracker = PositionTracker::new();

    // Initial state - should not skip
    let (skip_buy, _) = tracker.should_skip_side(Side::Buy).await;
    let (skip_sell, _) = tracker.should_skip_side(Side::Sell).await;
    assert!(!skip_buy);
    assert!(!skip_sell);

    // Add large long position - should skip buy
    tracker.update_position("market_1", Side::Buy, 10.0, 0.6).await;
    let (skip_buy, reason) = tracker.should_skip_side(Side::Buy).await;
    assert!(skip_buy, "Should skip buy when heavily long: {}", reason);
}

/// Test position limits
#[tokio::test]
async fn test_position_limits() {
    let mut tracker = PositionTracker::new();

    // No positions - should get base limit (max_position / 2)
    let limit = tracker.get_position_limit(Side::Buy, 5.0).await;
    assert_eq!(limit, 2.5, "Initial limit should be max_position / 2");

    // Add long position - limit should be reduced
    tracker.update_position("market_1", Side::Buy, 5.0, 0.6).await;
    let limit = tracker.get_position_limit(Side::Buy, 5.0).await;
    assert!(limit < 2.5, "Limit should be reduced when long: got {}", limit);
}

/// Test order book depth analysis
#[tokio::test]
async fn test_order_book_analysis() {
    use polymarket_pro::trading::analyze_order_book_depth_safe;

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
}

/// Test market making price calculation
#[tokio::test]
async fn test_mm_price_calculation() {
    use polymarket_pro::trading::{OrderBookDepth, OrderBookLevel, calculate_mm_prices};

    let depth = OrderBookDepth {
        best_bid: OrderBookLevel { price: 0.48, size: 100.0 },
        best_ask: OrderBookLevel { price: 0.52, size: 100.0 },
        second_bid: OrderBookLevel { price: 0.47, size: 100.0 },
        second_ask: OrderBookLevel { price: 0.53, size: 100.0 },
        bid_depth: 200.0,
        ask_depth: 200.0,
        imbalance: 0.0,
    };

    // No inventory skew
    let (bid, ask) = calculate_mm_prices(&depth, 0.0, 0.002, 0.02);
    assert!(bid < 0.50, "Bid should be below mid: got {}", bid);
    assert!(ask > 0.50, "Ask should be above mid: got {}", ask);
    assert!(bid < ask, "Bid should be less than ask");

    // With long skew - inventory_adjust = 0.5 * 0.01 = 0.005
    // This raises both bid and ask slightly, not lowers them
    let (bid_skewed, ask_skewed) = calculate_mm_prices(&depth, 0.5, 0.002, 0.02);
    // Positive skew raises prices to discourage buying
    assert!(bid_skewed > bid, "Bid should be higher with long skew: {} vs {}", bid_skewed, bid);
    assert!(ask_skewed > ask, "Ask should be higher with long skew: {} vs {}", ask_skewed, ask);
}

/// Test trading stats
#[tokio::test]
async fn test_trading_stats() {
    let mut stats = TradingStats::new();

    stats.record_order_placed(1.0);
    stats.record_order_filled(1.0);
    stats.record_order_cancelled();

    assert_eq!(stats.orders_placed, 1);
    assert_eq!(stats.orders_filled, 1);
    assert_eq!(stats.orders_cancelled, 1);

    let summary = stats.summary();
    assert!(summary.contains("Orders placed=1"));
}

/// Test price warning tracker
#[tokio::test]
async fn test_price_warning_tracker() {
    use polymarket_pro::trading::PriceWarningTracker;

    let mut tracker = PriceWarningTracker::new(60);

    // First warning should log
    let should_log = tracker.should_warn(0.05, "below");
    assert!(should_log);

    // Second warning within cooldown should not log
    let should_log = tracker.should_warn(0.05, "below");
    assert!(!should_log);
}

/// Test complete flow: position update → skew calculation → order decision
#[tokio::test]
async fn test_complete_flow() {
    let position_tracker = Arc::new(RwLock::new(PositionTracker::new()));
    let order_tracker = Arc::new(RwLock::new(OrderTracker::new()));

    // Step 1: Initial state - should allow both sides
    {
        let tracker = position_tracker.read().await;
        let (skip_buy, _) = tracker.should_skip_side(Side::Buy).await;
        let (skip_sell, _) = tracker.should_skip_side(Side::Sell).await;
        assert!(!skip_buy);
        assert!(!skip_sell);
    }

    // Step 2: Simulate buy order filled
    {
        let mut tracker = position_tracker.write().await;
        tracker.update_position("market_1", Side::Buy, 5.0, 0.5).await;
    }

    // Step 3: Track the order
    {
        let mut tracker = order_tracker.write().await;
        tracker.track_order("token_1".to_string(), "order_1".to_string(), "BUY".to_string(), 0.5, 5.0);
    }

    // Step 4: Check skew - should be positive (long)
    {
        let tracker = position_tracker.read().await;
        let skew = tracker.calculate_inventory_skew().await;
        assert!(skew > 0.0);
    }

    // Step 5: Should skip buy when heavily long
    {
        let tracker = position_tracker.read().await;
        let (skip_buy, _) = tracker.should_skip_side(Side::Buy).await;
        assert!(skip_buy);
    }
}

/// Test concurrent position updates
#[tokio::test]
async fn test_concurrent_updates() {
    let tracker = Arc::new(RwLock::new(PositionTracker::new()));
    let mut handles = vec![];

    // Spawn multiple concurrent updates
    for i in 0..10 {
        let tracker_clone = tracker.clone();
        let handle = tokio::spawn(async move {
            let mut tracker = tracker_clone.write().await;
            tracker.update_position(
                &format!("market_{}", i),
                Side::Buy,
                1.0,
                0.5
            ).await;
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all positions were recorded
    let tracker = tracker.read().await;
    let skew = tracker.calculate_inventory_skew().await;
    assert!(skew > 0.0);
}
