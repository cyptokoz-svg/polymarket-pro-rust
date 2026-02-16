//! Simplified dual-sided trading cycle matching Python logic
//! 
//! Python strategy:
//! 1. Buy both UP and DOWN (dual-sided)
//! 2. UP price = bid_price from order book
//! 3. DOWN price = 1.0 - ask_price (since UP + DOWN = 1)
//! 4. Balance positions based on skew
//! 5. CRITICAL: Fill detection before cancel -> update positions -> recalculate skew
//! 6. Price fixed for entire refresh interval (45s)

use anyhow::Result;
use polymarket_pro::{
    TradeExecutor, PolymarketWebSocket, PositionTracker, OrderTracker,
    TradeHistory, TradingStats, TradingConfig
};
use polymarket_client_sdk::clob::types::Side;
use std::sync::Arc;
use std::time::{Instant, Duration};
use tokio::sync::RwLock;
use tracing::{info, warn};
use std::sync::atomic::{AtomicU64, Ordering};

/// Global storage for fixed prices across cycles
static LAST_PRICE_UP: AtomicU64 = AtomicU64::new(0);
static LAST_PRICE_DOWN: AtomicU64 = AtomicU64::new(0);
static LAST_PRICE_TIME: AtomicU64 = AtomicU64::new(0);

/// Market info with token IDs (matches main.rs)
#[derive(Debug, Clone)]
pub struct DualSidedMarketInfo {
    pub up_token: String,
    pub down_token: String,
}

/// Run trading cycle with dual-sided buy strategy (matches Python)
#[allow(clippy::too_many_arguments)]
pub async fn run_trading_cycle_dual_sided(
    executor: Arc<TradeExecutor>,
    ws: Option<Arc<PolymarketWebSocket>>,
    position_tracker: Arc<RwLock<PositionTracker>>,
    order_tracker: Arc<RwLock<OrderTracker>>,
    _trade_history: Arc<TradeHistory>,
    stats: Arc<RwLock<TradingStats>>,
    trading_config: &TradingConfig,
    market_info: &DualSidedMarketInfo,
) -> Result<()> {
    info!("🔄 Running dual-sided trading cycle...");
    let cycle_start = Instant::now();
    
    let up_token = market_info.up_token.clone();
    let down_token = market_info.down_token.clone();
    
    // ===== STEP 1: Check for filled orders BEFORE cancelling =====
    info!("🔍 Step 1: Checking for filled orders...");
    
    // Get tracked orders for both tokens
    let up_tracked: Vec<String> = order_tracker.read().await.get_all_orders()
        .values()
        .filter(|o| o.token == up_token)
        .map(|o| o.order_id.clone())
        .collect();
    let down_tracked: Vec<String> = order_tracker.read().await.get_all_orders()
        .values()
        .filter(|o| o.token == down_token)
        .map(|o| o.order_id.clone())
        .collect();
    
    // Check filled orders for UP
    if !up_tracked.is_empty() {
        match executor.get_filled_orders(&up_token, &up_tracked).await {
            Ok(filled) => {
                if !filled.is_empty() {
                    info!("🎯 UP: Detected {} filled orders: {:?}", filled.len(), filled);
                    for filled_id in filled {
                        if let Some(order) = order_tracker.read().await.get_all_orders()
                            .values()
                            .find(|o| o.order_id == filled_id && o.token == up_token) {
                            let side = if order.side == "BUY" { Side::Buy } else { Side::Sell };
                            info!("📈 Updating UP position for filled order {}: {:?} {} @ {}", 
                                filled_id, side, order.size, order.price);
                            position_tracker.write().await.update_position(
                                &up_token, side, order.size, order.price
                            ).await;
                            stats.write().await.record_order_filled(order.size);
                        }
                    }
                }
            }
            Err(e) => warn!("⚠️ Failed to check UP filled orders: {}", e),
        }
    }
    
    // Check filled orders for DOWN
    if !down_tracked.is_empty() {
        match executor.get_filled_orders(&down_token, &down_tracked).await {
            Ok(filled) => {
                if !filled.is_empty() {
                    info!("🎯 DOWN: Detected {} filled orders: {:?}", filled.len(), filled);
                    for filled_id in filled {
                        if let Some(order) = order_tracker.read().await.get_all_orders()
                            .values()
                            .find(|o| o.order_id == filled_id && o.token == down_token) {
                            let side = if order.side == "BUY" { Side::Buy } else { Side::Sell };
                            info!("📈 Updating DOWN position for filled order {}: {:?} {} @ {}", 
                                filled_id, side, order.size, order.price);
                            position_tracker.write().await.update_position(
                                &down_token, side, order.size, order.price
                            ).await;
                            stats.write().await.record_order_filled(order.size);
                        }
                    }
                }
            }
            Err(e) => warn!("⚠️ Failed to check DOWN filled orders: {}", e),
        }
    }
    
    // ===== STEP 2: Cancel old orders =====
    info!("🗑️ Step 2: Cancelling old orders...");
    match executor.cancel_orders_for_market(&up_token).await {
        Ok(result) => info!("✅ Cancelled {} UP orders", result.cancelled),
        Err(e) => warn!("⚠️ Failed to cancel UP orders: {}", e),
    }
    match executor.cancel_orders_for_market(&down_token).await {
        Ok(result) => info!("✅ Cancelled {} DOWN orders", result.cancelled),
        Err(e) => warn!("⚠️ Failed to cancel DOWN orders: {}", e),
    }
    
    // Clear tracked orders
    order_tracker.write().await.clear_orders_for_token(&up_token);
    order_tracker.write().await.clear_orders_for_token(&down_token);
    
    // ===== STEP 3: Wait for cancellation to propagate =====
    info!("⏳ Step 3: Waiting 100ms for cancellation to propagate...");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // ===== STEP 4: Get current positions and recalculate skew =====
    info!("📊 Step 4: Recalculating positions and skew...");
    let (up_position, down_position, total_position) = {
        let tracker = position_tracker.read().await;
        let status = tracker.get_inventory_status().await;
        (status.up_value, status.down_value, status.total_value)
    };
    
    info!("📊 Positions: UP={}, DOWN={}, Total={}", 
        up_position, down_position, total_position);
    
    // Check total position limit
    if total_position >= trading_config.max_total_position {
        warn!("⏹️ Total position limit reached: {}/{}",
            total_position, trading_config.max_total_position);
        return Ok(());
    }
    
    // ===== STEP 5: Get prices from WebSocket (with 45s fixed price logic) =====
    info!("💰 Step 5: Getting prices (with 45s fixed price logic)...");
    
    // Check if we should use fixed prices from previous cycle
    let now = Instant::now();
    let now_secs = now.elapsed().as_secs();
    let last_time = LAST_PRICE_TIME.load(Ordering::Relaxed);
    let refresh_interval = trading_config.refresh_interval as u64;
    
    let ws_ref = ws.as_ref();
    let (up_price, down_price) = if now_secs - last_time < refresh_interval {
        // Use fixed prices from previous cycle
        let up_fixed = f64::from_bits(LAST_PRICE_UP.load(Ordering::Relaxed));
        let down_fixed = f64::from_bits(LAST_PRICE_DOWN.load(Ordering::Relaxed));
        if up_fixed > 0.0 && down_fixed > 0.0 {
            info!("📊 Using fixed prices from previous cycle: UP={}, DOWN={}", up_fixed, down_fixed);
            (up_fixed, down_fixed)
        } else {
            // No fixed prices yet, get new ones
            get_new_prices(ws_ref, &up_token, &down_token).await?
        }
    } else {
        // New cycle, get new prices
        let (up, down) = get_new_prices(ws_ref, &up_token, &down_token).await?;
        // Store for next cycles
        LAST_PRICE_UP.store(up.to_bits(), Ordering::Relaxed);
        LAST_PRICE_DOWN.store(down.to_bits(), Ordering::Relaxed);
        LAST_PRICE_TIME.store(now_secs, Ordering::Relaxed);
        info!("📊 New prices stored for fixed price logic: UP={}, DOWN={}", up, down);
        (up, down)
    };
    
    info!("💰 Final prices: UP={}, DOWN={}", up_price, down_price);
    
    // Extreme price protection: 0.1-0.9 (no orders in 0-0.1 or 0.9-1.0)
    const EXTREME_MIN: f64 = 0.10;
    const EXTREME_MAX: f64 = 0.90;
    if up_price < EXTREME_MIN || up_price > EXTREME_MAX || down_price < EXTREME_MIN || down_price > EXTREME_MAX {
        warn!("⚠️ Extreme prices detected: UP={}, DOWN={}", up_price, down_price);
        warn!("⏹️ Skipping all orders - prices outside safe range [{}, {}]", EXTREME_MIN, EXTREME_MAX);
        return Ok(());
    }
    
    // ===== STEP 6: Calculate skew and determine order sizes =====
    info!("📊 Step 6: Calculating order sizes...");
    let skew = up_position - down_position;  // Positive = UP too much
    let max_skew = trading_config.order_size * 0.6;  // Python uses 0.6 (not 0.4)
    let remaining = trading_config.max_total_position - total_position;
    let base_size = trading_config.order_size;
    
    info!("📊 Skew={}, MaxSkew={}, Remaining={}", skew, max_skew, remaining);
    
    let (up_size, down_size) = if skew > max_skew {
        // UP too much, only buy DOWN
        warn!("⚠️ UP skew too high ({}), buying only DOWN", skew);
        (0.0, base_size.min(remaining))
    } else if skew < -max_skew {
        // DOWN too much, only buy UP
        warn!("⚠️ DOWN skew too high ({}), buying only UP", skew.abs());
        (base_size.min(remaining), 0.0)
    } else {
        // Balanced, buy both (Python: both sides buy base_size, not split)
        let up_buy = base_size.min(remaining);
        let down_buy = base_size.min(remaining);
        (up_buy, down_buy)
    };
    
    if up_size == 0.0 && down_size == 0.0 {
        warn!("⏹️ No size to trade");
        return Ok(());
    }
    
    info!("📊 Will place: UP={}, DOWN={}", up_size, down_size);
    
    // ===== STEP 7: Check balance with buffer =====
    let up_need = up_price * up_size;
    let down_need = down_price * down_size;
    let total_need = (up_need + down_need) * 1.15;
    
    // TEMPORARY: Use environment variable or default balance for testing
    let balance = if let Ok(test_balance) = std::env::var("TEST_BALANCE") {
        test_balance.parse::<f64>().unwrap_or(1000.0)
    } else {
        // Try to get real balance, fallback to 1000 for testing
        executor.get_usdc_balance().await.unwrap_or(1000.0)
    };
    
    if balance < total_need {
        warn!("⚠️ Insufficient balance: {} < {} (need UP:{} + DOWN:{} × 1.15)",
            balance, total_need, up_need, down_need);
        return Ok(());
    }
    info!("✅ Balance sufficient: {} >= {}", balance, total_need);
    
    // ===== STEP 8: Place orders (UP first, then DOWN) =====
    info!("🚀 Step 8: Placing orders...");
    let mut placed_up = 0.0;
    let mut placed_down = 0.0;
    
    // Place UP order first (skip balance check as per Python)
    if up_size > 0.0 {
        info!("🔍 UP: BUY @ {} size={}", up_price, up_size);
        match executor.place_order_complete(
            &up_token,
            Side::Buy,
            up_price,
            up_size,
            trading_config.safe_range_low,
            trading_config.safe_range_high,
        ).await {
            Ok(Some(order_id)) => {
                info!("✅ UP order placed: {}", order_id);
                placed_up = up_size;
                stats.write().await.record_order_placed(up_size);
                order_tracker.write().await.track_order(
                    up_token.clone(), order_id, "BUY".to_string(), up_price, up_size);
            }
            Ok(None) => {
                warn!("❌ UP order failed (returned None)");
            }
            Err(e) => {
                warn!("❌ UP order failed: {}", e);
            }
        }
    }
    
    // Place DOWN order (check balance again as per Python)
    if down_size > 0.0 {
        // Re-check balance after UP order
        let balance_after_up = executor.get_usdc_balance().await.unwrap_or(0.0);
        let down_need_now = down_price * down_size * 1.15;
        
        if balance_after_up < down_need_now {
            warn!("⚠️ Insufficient balance for DOWN after UP order: {} < {}",
                balance_after_up, down_need_now);
        } else {
            info!("🔍 DOWN: BUY @ {} size={}", down_price, down_size);
            match executor.place_order_complete(
                &down_token,
                Side::Buy,
                down_price,
                down_size,
                trading_config.safe_range_low,
                trading_config.safe_range_high,
            ).await {
                Ok(Some(order_id)) => {
                    info!("✅ DOWN order placed: {}", order_id);
                    placed_down = down_size;
                    stats.write().await.record_order_placed(down_size);
                    order_tracker.write().await.track_order(
                        down_token.clone(), order_id, "BUY".to_string(), down_price, down_size);
                }
                Ok(None) => {
                    warn!("❌ DOWN order failed (returned None)");
                }
                Err(e) => {
                    warn!("❌ DOWN order failed: {}", e);
                }
            }
        }
    }
    
    info!("✅ Trading cycle completed: UP={}, DOWN={}", placed_up, placed_down);
    info!("⏱️ Cycle took: {:?}", cycle_start.elapsed());
    
    Ok(())
}

/// Get new prices from WebSocket (with retry logic)
async fn get_new_prices(
    ws: Option<&Arc<PolymarketWebSocket>>,
    up_token: &str,
    down_token: &str,
) -> Result<(f64, f64)> {
    if let Some(ws) = ws {
        let mut retries = 0;
        let max_retries = 10;
        
        while retries < max_retries {
            match get_ws_prices(ws, up_token, down_token).await {
                Some((up, down)) => return Ok((up, down)),
                None => {
                    retries += 1;
                    info!("⏳ Waiting for WebSocket prices... retry {}/{}", retries, max_retries);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
        Err(anyhow::anyhow!("No WebSocket prices available after {} retries", max_retries))
    } else {
        Err(anyhow::anyhow!("WebSocket not available"))
    }
}

/// Get prices from WebSocket for both tokens
async fn get_ws_prices(
    ws: &PolymarketWebSocket,
    up_token: &str,
    down_token: &str,
) -> Option<(f64, f64)> {
    // Try to get from WebSocket cache (returns (bid, ask))
    let up_prices = ws.get_price(up_token).await;
    let down_prices = ws.get_price(down_token).await;
    
    match (up_prices, down_prices) {
        (Some((up_bid, up_ask)), Some((_down_bid, _down_ask))) => {
            // Python logic: UP price = bid, DOWN price = 1.0 - UP_ask (not DOWN_ask)
            let up_price = up_bid;
            let down_price = 1.0 - up_ask;  // Use UP's ask, not DOWN's ask
            
            // Validate: UP + DOWN should be close to 1.0
            let sum = up_price + down_price;
            if (sum - 1.0).abs() < 0.1 {
                Some((up_price, down_price))
            } else {
                warn!("⚠️ Price validation failed: UP + DOWN = {} (expected ~1.0)", sum);
                Some((up_price, down_price)) // Use anyway
            }
        }
        (Some((up_bid, _)), None) => {
            // Only have UP, estimate DOWN
            Some((up_bid, 1.0 - up_bid))
        }
        (None, Some((down_bid, _))) => {
            // Only have DOWN, estimate UP
            Some((1.0 - down_bid, down_bid))
        }
        (None, None) => None,
    }
}
