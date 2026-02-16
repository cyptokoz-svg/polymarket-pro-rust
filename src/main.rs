//! Polymarket Pro - Main entry point
//!
//! 支持配置文件: polymarket-pro.toml, polymarket-pro.yaml, config.toml

use anyhow::Result;
use futures::FutureExt;
use polymarket_pro::*;
use polymarket_pro::api::Side;
use polymarket_pro::trading::PriceWarningTracker;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn, Level};

// Import BTC market finder
mod btc_market;
use btc_market::{find_btc_5min_market, get_market_token_ids};

#[tokio::main]
async fn main() -> Result<()> {
    // Set up panic hook to log panics
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("🛑 PANIC: {}", info);
        eprintln!("{}", msg);
        // Also try to write to a crash log file
        let _ = std::fs::write("/tmp/polymarket-pro-crash.log", msg);
    }));
    
    // Set up custom panic hook for tokio
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_else(|| "unknown".to_string());
        let msg = format!("🛑 PANIC at {}: {}", location, info);
        eprintln!("{}", msg);
        let _ = std::fs::write("/tmp/polymarket-pro-crash.log", format!("{}\n", msg));
        default_panic(info);
    }));
    
    let config = load_config().await?;

    let log_level = config.log_level.as_deref().unwrap_or("info");
    let level = parse_log_level(log_level);
    
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!("Starting Polymarket Pro v{}", VERSION);
    info!("Configuration loaded");
    info!("  Order size: {}", config.trading.order_size);
    info!("  Max position: {}", config.trading.max_position);
    info!("  Safe range: {} - {}", config.trading.safe_range_low, config.trading.safe_range_high);
    info!("  Refresh interval: {}s", config.trading.refresh_interval);

    let executor = Arc::new(
        TradeExecutor::new(
            &config.pk,
            config.api.key.clone(),
            config.api.secret.clone(),
            config.api.passphrase.clone(),
        ).await.map_err(|e| anyhow::anyhow!("Failed to create trade executor: {}", e))?
    );
    
    // Check if simulation mode is enabled via environment variable
    let simulation_mode = std::env::var("SIMULATION_MODE").unwrap_or_default() == "true";
    if simulation_mode {
        warn!("🎮 SIMULATION MODE ENABLED - No real orders will be placed!");
    }
    
    info!("Trade executor initialized");

    match executor.server_time().await {
        Ok(time) => info!("Server time: {}", time),
        Err(e) => warn!("Failed to get server time: {}", e),
    }

    let position_tracker = Arc::new(RwLock::new(PositionTracker::new()));
    let order_tracker = Arc::new(RwLock::new(OrderTracker::new()));
    let trade_history = Arc::new(TradeHistory::default());

    // Initialize trading stats (will be loaded from file later)
    let stats = Arc::new(RwLock::new(TradingStats::load_or_new()));

    let rate_limiter = Arc::new(utils::rate_limiter::RateLimiter::new_default());
    let price_warning_tracker = Arc::new(RwLock::new(PriceWarningTracker::new(
        config.trading.price_warn_cooldown
    )));
    let price_freshness = Arc::new(RwLock::new(PriceFreshness::new(5)));

    let ws_subscriber: Option<Arc<PolymarketWebSocket>> = if config.websocket.enabled {
        let ws = Arc::new(PolymarketWebSocket::new());
        ws.start(vec![]).await;
        info!("WebSocket connected");
        Some(ws)
    } else {
        None
    };

    let wallet_addr = executor.address();
    let _safe = SafeWallet::new(
        &config.safe_address,
        &wallet_addr,
    )?;

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(1);

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = shutdown_tx.send(()).await;
    });

    info!("Bot initialized successfully, starting event loop...");

    // REMOVED: Background cleanup task - now handled in main trading cycle
    // to avoid conflicts with fill detection logic
    // The main loop's 45-second cycle with proper fill detection is sufficient

    let stats_logger = stats.clone();
    tokio::spawn(async move {
        let mut stats_interval = interval(Duration::from_secs(300));
        loop {
            stats_interval.tick().await;
            let stats = stats_logger.read().await;
            info!("{}", stats.summary());
            if let Err(e) = stats.save_to_file() {
                error!("Failed to save stats: {}", e);
            }
        }
    });

    // Main event loop - matches Python polymaker_5m.py
    // - Find a 5-minute market (e.g., BTC)
    // - Trade on this market every 45 seconds
    // - When market expires, find new 5-minute market
    let trading_interval = Duration::from_secs(config.trading.refresh_interval); // 45 seconds
    let market_check_interval = Duration::from_secs(60); // Check for new market every 60 seconds
    let mut trading_tick = interval(trading_interval);
    let mut market_check_tick = interval(market_check_interval);
    
    // Clone ws_subscriber for trading cycle
    let ws_subscriber_trading = ws_subscriber.clone();
    
    // Current active 5-minute market
    let mut current_market: Option<MarketInfo> = None;
    
    // Initial market search on startup
    info!("🔍 Initial market search on startup...");
    match find_btc_5min_market(&executor).await {
        Some(market) => {
            let condition_id = market.get("conditionId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            info!("✅ Found initial 5-minute market: {}", condition_id);
            
            // Subscribe to WebSocket for initial market and get token IDs
            if !condition_id.is_empty() {
                // Retry getting token IDs up to 3 times
                let mut token_ids = None;
                for attempt in 1..=3 {
                    token_ids = subscribe_to_market_ws(&condition_id, ws_subscriber.clone()).await;
                    if token_ids.is_some() {
                        break;
                    }
                    warn!("⚠️ Could not get token IDs (attempt {}), retrying...", attempt);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
                
                if let Some((up_token, down_token)) = token_ids {
                    current_market = Some(MarketInfo {
                        condition_id,
                        up_token,
                        down_token,
                        end_date: market.get("endDate").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    });
                } else {
                    error!("🛑 Failed to get token IDs after 3 attempts, skipping this market");
                    // Don't use condition_id as fallback - it's not a valid token ID
                    // The market will be skipped and we'll retry in the next cycle
                }
            }
        }
        None => {
            warn!("⚠️ No 5-minute market found on startup, will retry in 60s");
        }
    }
    
    // Flag to skip first trading cycle to allow WebSocket to connect
    let mut first_cycle = true;
    
    loop {
        tokio::select! {
            // Check for new market periodically
            _ = market_check_tick.tick() => {
                // If no current market or current market expired, find new one
                let need_new_market = if let Some(ref market_info) = current_market {
                    if let Some(ref end_date_str) = market_info.end_date {
                        if let Some(end_date) = parse_market_end_time(end_date_str) {
                            let now = chrono::Utc::now();
                            let time_to_expiry = end_date.signed_duration_since(now).num_seconds();
                            time_to_expiry <= 60 // Need new market if expiring in 60s
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                } else {
                    true
                };
                
                if need_new_market {
                    info!("🔍 Looking for BTC updown 5m market...");
                    
                    // Wrap market finding in panic catcher
                    let market_result = std::panic::AssertUnwindSafe(async {
                        find_btc_5min_market(&executor).await
                    }).catch_unwind().await;
                    
                    match market_result {
                        Ok(Some(new_market)) => {
                            let new_condition_id = new_market.get("conditionId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let old_condition_id = current_market.as_ref().map(|m| m.condition_id.clone());
                            
                            if old_condition_id.as_ref() != Some(&new_condition_id) {
                                info!("✅ Found new 5-minute market: {}", new_condition_id);
                                
                                // Cancel orders on old market if exists
                                if let Some(ref old_market_info) = current_market {
                                    info!("📤 Unsubscribing from old market: {}", old_market_info.up_token);
                                    let _ = executor.cancel_orders_for_market(&old_market_info.up_token).await;
                                }
                                
                                // Update WebSocket subscription for new market and get token IDs
                                if !new_condition_id.is_empty() {
                                    // Retry getting token IDs up to 3 times
                                    let mut token_ids = None;
                                    for attempt in 1..=3 {
                                        token_ids = subscribe_to_market_ws(&new_condition_id, ws_subscriber.clone()).await;
                                        if token_ids.is_some() {
                                            break;
                                        }
                                        warn!("⚠️ Could not get token IDs for new market (attempt {}), retrying...", attempt);
                                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    }
                                    
                                    if let Some((up_token, down_token)) = token_ids {
                                        current_market = Some(MarketInfo {
                                            condition_id: new_condition_id,
                                            up_token,
                                            down_token,
                                            end_date: new_market.get("endDate").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                        });
                                    } else {
                                        error!("🛑 Failed to get token IDs for new market after 3 attempts, keeping old market");
                                        // Don't update current_market - keep trading on old market
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            warn!("⚠️ No 5-minute market found");
                        }
                        Err(_) => {
                            error!("🛑 Panic caught while finding new market, continuing...");
                        }
                    }
                }
            }
            
            // Trading cycle every 45 seconds
            _ = trading_tick.tick() => {
                // Check if we have an active market
                let market_info = match &current_market {
                    Some(m) => m.clone(),
                    None => {
                        warn!("⚠️ No active 5-minute market, skipping trading cycle");
                        continue;
                    }
                };
                
                // Check if market is still valid
                if let Some(ref end_date_str) = market_info.end_date {
                    if let Some(end_date) = parse_market_end_time(end_date_str) {
                        let now = chrono::Utc::now();
                        let time_to_expiry = end_date.signed_duration_since(now).num_seconds();
                        
                        if time_to_expiry <= 0 {
                            info!("⏭️ Current market expired, will find new one");
                            current_market = None;
                            continue;
                        }
                        
                        info!("⏰ Trading on 5-minute market, expires in {}s", time_to_expiry);
                    }
                }
                
                // Skip first cycle to allow WebSocket to connect and receive prices
                if first_cycle {
                    info!("⏳ Skipping first trading cycle to allow WebSocket initialization...");
                    first_cycle = false;
                    continue;
                }
                
                // Apply rate limiting
                rate_limiter.wait().await;

                // Run trading cycle on current 5-minute market
                let ws_ref = ws_subscriber_trading.clone();
                if let Err(e) = run_trading_cycle_single_market(
                    executor.clone(),
                    ws_ref,
                    position_tracker.clone(),
                    order_tracker.clone(),
                    trade_history.clone(),
                    stats.clone(),
                    price_freshness.clone(),
                    price_warning_tracker.clone(),
                    &config.trading,
                    &market_info,
                ).await {
                    error!("Trading cycle error: {}", e);
                    stats.write().await.record_error();
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, saving state...");

                // Save stats
                if let Err(e) = stats.read().await.save_to_file() {
                    error!("Failed to save stats: {}", e);
                }

                // Save trade history
                // Note: trade_history is auto-saved on each trade

                info!("State saved, shutting down...");
                break;
            }
        }
    }

    info!("Shutting down...");
    Ok(())
}

/// Load configuration with environment variable priority
/// 
/// Security best practice: Environment variables take precedence over config files
/// to avoid accidentally committing sensitive data like private keys.
async fn load_config() -> Result<Config> {
    // First, try to load from config file as base configuration
    let mut config = match Config::load() {
        Ok(cfg) => {
            info!("Loaded base configuration from file");
            cfg
        }
        Err(e) => {
            warn!("No config file found ({}), using defaults", e);
            Config::default()
        }
    };
    
    // Environment variables ALWAYS override config file values
    // This ensures sensitive data like private keys are not hardcoded
    let env_config = config::from_env()?;
    
    // Merge: env vars take precedence
    if !env_config.pk.is_empty() {
        config.pk = env_config.pk;
        info!("Using private key from environment variable PK");
    } else if !config.pk.is_empty() {
        warn!("⚠️  WARNING: Private key loaded from config file!");
        warn!("⚠️  For security, please set the PK environment variable instead.");
    }
    
    if !env_config.safe_address.is_empty() {
        config.safe_address = env_config.safe_address;
        info!("Using Safe address from environment variable SAFE_ADDRESS");
    }
    
    // Merge API config (env vars take precedence)
    if env_config.api.key.is_some() {
        config.api.key = env_config.api.key;
    }
    if env_config.api.secret.is_some() {
        config.api.secret = env_config.api.secret;
    }
    if env_config.api.passphrase.is_some() {
        config.api.passphrase = env_config.api.passphrase;
    }
    
    // Validate final configuration
    config.validate()?;
    
    // Security check: warn if private key is still not from env
    if std::env::var("PK").is_err() {
        warn!("⚠️  SECURITY WARNING: PK environment variable not set!");
        warn!("⚠️  Private key should be provided via environment variable, not config file.");
    }
    
    Ok(config)
}

/// Parse log level string
fn parse_log_level(level: &str) -> Level {
    match level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    }
}

/// Market info with token IDs
#[derive(Debug, Clone)]
struct MarketInfo {
    condition_id: String,
    up_token: String,
    down_token: String,
    end_date: Option<String>,
}

/// Subscribe to WebSocket for a market and return token IDs
async fn subscribe_to_market_ws(
    condition_id: &str,
    ws_subscriber: Option<Arc<PolymarketWebSocket>>,
) -> Option<(String, String)> {
    if let Some(ref ws) = ws_subscriber {
        info!("📡 Fetching token IDs for market: {}", condition_id);
        
        // Get UP and DOWN token IDs from Gamma API
        match get_market_token_ids(condition_id).await {
            Some((up_token, down_token)) => {
                info!("📡 Subscribing to UP: {}, DOWN: {}", 
                    &up_token[..20.min(up_token.len())], 
                    &down_token[..20.min(down_token.len())]);
                
                let token_ids = vec![up_token.clone(), down_token.clone()];
                ws.update_subscription(token_ids).await;
                
                // Set token labels - use FIRST 20 chars to match Gamma API behavior
                let mut labels = std::collections::HashMap::new();
                let up_short = &up_token[..20.min(up_token.len())];
                let down_short = &down_token[..20.min(down_token.len())];
                labels.insert(up_short.to_string(), "UP".to_string());
                labels.insert(down_short.to_string(), "DOWN".to_string());
                ws.set_token_labels(labels).await;
                
                info!("✅ WebSocket subscription updated successfully");
                return Some((up_token, down_token));
            }
            None => {
                warn!("⚠️ Could not fetch token IDs for {}, using condition_id as fallback", condition_id);
                let token_ids = vec![condition_id.to_string()];
                ws.update_subscription(token_ids).await;
                return None;
            }
        }
    } else {
        warn!("⚠️ WebSocket not available, cannot subscribe to market");
        None
    }
}

/// Parse market end time from string
fn parse_market_end_time(end_date: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Try RFC3339 format first
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(end_date) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    
    // Try other common formats
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.3fZ",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d",
    ];
    
    for format in &formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(end_date, format) {
            return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
        }
    }
    
    None
}

/// Run trading cycle on a single market (for 5-minute market strategy)
#[allow(clippy::too_many_arguments)]
async fn run_trading_cycle_single_market(
    executor: Arc<TradeExecutor>,
    ws: Option<Arc<PolymarketWebSocket>>,
    position_tracker: Arc<RwLock<PositionTracker>>,
    order_tracker: Arc<RwLock<OrderTracker>>,
    trade_history: Arc<TradeHistory>,
    stats: Arc<RwLock<TradingStats>>,
    price_freshness: Arc<RwLock<PriceFreshness>>,
    price_warning_tracker: Arc<RwLock<PriceWarningTracker>>,
    trading_config: &TradingConfig,
    market_info: &MarketInfo,
) -> Result<()> {
    // Wrap the actual implementation with panic catching
    let result = std::panic::AssertUnwindSafe(run_trading_cycle_single_market_inner(
        executor,
        ws,
        position_tracker,
        order_tracker,
        trade_history,
        stats,
        price_freshness,
        price_warning_tracker,
        trading_config,
        market_info,
    )).catch_unwind().await;
    
    match result {
        Ok(inner_result) => inner_result,
        Err(_) => {
            error!("🛑 PANIC caught in trading cycle! This is a bug.");
            // Return error to stop the cycle gracefully
            Err(anyhow::anyhow!("Trading cycle panicked"))
        }
    }
}

/// Inner implementation of trading cycle
#[allow(clippy::too_many_arguments)]
async fn run_trading_cycle_single_market_inner(
    executor: Arc<TradeExecutor>,
    ws: Option<Arc<PolymarketWebSocket>>,
    position_tracker: Arc<RwLock<PositionTracker>>,
    order_tracker: Arc<RwLock<OrderTracker>>,
    trade_history: Arc<TradeHistory>,
    stats: Arc<RwLock<TradingStats>>,
    _price_freshness: Arc<RwLock<PriceFreshness>>,
    price_warning_tracker: Arc<RwLock<PriceWarningTracker>>,
    trading_config: &TradingConfig,
    market_info: &MarketInfo,
) -> Result<()> {
    info!("Running trading cycle on single market...");
    let cycle_start = Instant::now();

    // Use UP token for trading (represents the market)
    let up_token_id = market_info.up_token.clone();
    let down_token_id = market_info.down_token.clone();
    
    // DEBUG: Print token IDs
    info!("DEBUG: up_token_id length: {}, value: {}", up_token_id.len(), &up_token_id[..up_token_id.len().min(30)]);
    info!("DEBUG: down_token_id length: {}, value: {}", down_token_id.len(), &down_token_id[..down_token_id.len().min(30)]);
    
    // Check WebSocket prices - if not available, skip this cycle (non-blocking)
    let ws_prices_available = if let Some(ref ws) = ws {
        let prices = ws.get_all_prices().await;
        if !prices.is_empty() {
            info!("✅ WebSocket prices available: {} tokens", prices.len());
            true
        } else {
            warn!("⚠️ WebSocket prices not available, skipping trading cycle");
            return Ok(());
        }
    } else {
        warn!("⚠️ WebSocket not available, skipping trading cycle");
        return Ok(());
    };

    // Calculate inventory skew and status once, reuse throughout the function
    let (inventory_skew, status, should_return) = {
        let tracker = position_tracker.read().await;
        let skew = tracker.calculate_inventory_skew().await;
        let status = tracker.get_inventory_status().await;
        let should_return = status.total_value >= trading_config.max_total_position;
        (skew, status, should_return)
    };
    info!("Current inventory skew: {:.2}", inventory_skew);
    
    info!("📊 Inventory: UP=${:.2} | DOWN=${:.2} | Total=${:.2} | Skew={:.1}%",
        status.up_value, status.down_value, status.total_value, status.skew * 100.0);

    // Check total position limit
    if should_return {
        warn!("Total position limit reached: ${:.2} >= ${:.2}",
            status.total_value, trading_config.max_total_position);
        return Ok(());
    }

    // Check merge opportunity first (Python feature)
    if let Some(merge_amount) = position_tracker.read().await.check_merge_opportunity(
        &up_token_id, trading_config.merge_threshold) {
        info!("💡 Merge opportunity: {:.2} shares for {}", merge_amount, up_token_id);
        stats.write().await.record_merge();
    }

    // Get prices from WebSocket for both UP and DOWN tokens
    let (up_price, down_price) = if let Some(ref ws) = ws {
        let up = ws.get_price(&up_token_id).await.map(|(bid, ask)| (bid + ask) / 2.0);
        let down = ws.get_price(&down_token_id).await.map(|(bid, ask)| (bid + ask) / 2.0);
        (up, down)
    } else {
        (None, None)
    };
    
    let up_price = match up_price {
        Some(p) => p,
        None => {
            warn!("No UP price available for {}, skipping", up_token_id);
            return Ok(());
        }
    };
    
    let down_price = match down_price {
        Some(p) => p,
        None => {
            warn!("No DOWN price available for {}, skipping", down_token_id);
            return Ok(());
        }
    };

    info!("💰 Prices - UP: {:.4}, DOWN: {:.4}", up_price, down_price);

    // Validate price range with min/max price (Python style)
    if up_price < trading_config.min_price || up_price > trading_config.max_price {
        warn!("UP price {:.4} outside valid range [{:.2}, {:.2}]",
            up_price, trading_config.min_price, trading_config.max_price);
        return Ok(());
    }
    
    if down_price < trading_config.min_price || down_price > trading_config.max_price {
        warn!("DOWN price {:.4} outside valid range [{:.2}, {:.2}]",
            down_price, trading_config.min_price, trading_config.max_price);
        return Ok(());
    }

    // Check if price is in safe range (warning but allow, with cooldown)
    if up_price < trading_config.safe_range_low {
        price_warning_tracker.write().await.log_price_warning(
            up_price, "below", trading_config.safe_range_low, trading_config.safe_range_high, "UP"
        );
    } else if up_price > trading_config.safe_range_high {
        price_warning_tracker.write().await.log_price_warning(
            up_price, "above", trading_config.safe_range_low, trading_config.safe_range_high, "UP"
        );
    }

    // BUG FIX: Also check DOWN price safe range
    if down_price < trading_config.safe_range_low {
        price_warning_tracker.write().await.log_price_warning(
            down_price, "below", trading_config.safe_range_low, trading_config.safe_range_high, "DOWN"
        );
    } else if down_price > trading_config.safe_range_high {
        price_warning_tracker.write().await.log_price_warning(
            down_price, "above", trading_config.safe_range_low, trading_config.safe_range_high, "DOWN"
        );
    }

    // CRITICAL FIX: Check for fills BEFORE cancelling to avoid double-counting
    // Step 1: Get tracked order IDs for both tokens
    let tracked_orders_up: Vec<String> = {
        let tracker = order_tracker.read().await;
        tracker.get_all_orders()
            .values()
            .filter(|o| o.token == up_token_id)
            .map(|o| o.order_id.clone())
            .collect()
    };
    
    let tracked_orders_down: Vec<String> = {
        let tracker = order_tracker.read().await;
        tracker.get_all_orders()
            .values()
            .filter(|o| o.token == down_token_id)
            .map(|o| o.order_id.clone())
            .collect()
    };
    
    // Step 2: Check which orders have filled (no longer in open orders)
    let filled_order_ids_up = if !tracked_orders_up.is_empty() {
        match executor.get_filled_orders(&up_token_id, &tracked_orders_up).await {
            Ok(filled) => {
                if !filled.is_empty() {
                    info!("🎯 Detected {} filled orders for UP: {:?}", 
                        filled.len(), filled);
                }
                filled
            }
            Err(e) => {
                warn!("⚠️ Failed to check filled orders for UP: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };
    
    let filled_order_ids_down = if !tracked_orders_down.is_empty() {
        match executor.get_filled_orders(&down_token_id, &tracked_orders_down).await {
            Ok(filled) => {
                if !filled.is_empty() {
                    info!("🎯 Detected {} filled orders for DOWN: {:?}", 
                        filled.len(), filled);
                }
                filled
            }
            Err(e) => {
                warn!("⚠️ Failed to check filled orders for DOWN: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };
    
    // Step 3: Update positions for filled orders
    for filled_id in &filled_order_ids_up {
        if let Some(order) = order_tracker.read().await.get_all_orders()
            .values()
            .find(|o| o.order_id == *filled_id && o.token == up_token_id) {
            
            let side = if order.side == "BUY" { Side::Buy } else { Side::Sell };
            let size = order.size;
            let price = order.price;
            
            info!("📈 Updating position for filled UP order {}: {:?} {} @ {}", 
                filled_id, side, size, price);
            
            position_tracker.write().await.update_position(
                &up_token_id,
                side,
                size,
                price,
            ).await;
            
            stats.write().await.record_order_filled(size);
        }
    }
    
    for filled_id in &filled_order_ids_down {
        if let Some(order) = order_tracker.read().await.get_all_orders()
            .values()
            .find(|o| o.order_id == *filled_id && o.token == down_token_id) {
            
            let side = if order.side == "BUY" { Side::Buy } else { Side::Sell };
            let size = order.size;
            let price = order.price;
            
            info!("📈 Updating position for filled DOWN order {}: {:?} {} @ {}", 
                filled_id, side, size, price);
            
            position_tracker.write().await.update_position(
                &down_token_id,
                side,
                size,
                price,
            ).await;
            
            stats.write().await.record_order_filled(size);
        }
    }
    
    // Step 4: Now cancel remaining open orders for both tokens
    match executor.cancel_orders_for_market(&up_token_id).await {
        Ok(result) => {
            info!("✅ Cancelled {} existing orders for UP {}", result.cancelled, up_token_id);
        }
        Err(e) => {
            error!("❌ Failed to cancel orders for UP {}: {}", up_token_id, e);
            error!("🛑 Stopping trading cycle to avoid duplicate orders");
            return Ok(());
        }
    }
    
    match executor.cancel_orders_for_market(&down_token_id).await {
        Ok(result) => {
            info!("✅ Cancelled {} existing orders for DOWN {}", result.cancelled, down_token_id);
        }
        Err(e) => {
            error!("❌ Failed to cancel orders for DOWN {}: {}", down_token_id, e);
            error!("🛑 Stopping trading cycle to avoid duplicate orders");
            return Ok(());
        }
    }

    // Step 5: Clear tracked orders for both tokens (including filled ones)
    order_tracker.write().await.clear_orders_for_token(&up_token_id);
    order_tracker.write().await.clear_orders_for_token(&down_token_id);

    // Wait for cancellation to propagate and verify
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    // Verify no open orders remain (with retry)
    for attempt in 1..=3 {
        let open_orders_up = executor.get_open_orders().await.unwrap_or_default()
            .into_iter()
            .filter(|o| o.get("asset_id").and_then(|v| v.as_str()) == Some(&up_token_id))
            .count();
        let open_orders_down = executor.get_open_orders().await.unwrap_or_default()
            .into_iter()
            .filter(|o| o.get("asset_id").and_then(|v| v.as_str()) == Some(&down_token_id))
            .count();
        
        if open_orders_up == 0 && open_orders_down == 0 {
            info!("✅ Verified no open orders remaining");
            break;
        }
        
        warn!("⚠️ Still have {} UP and {} DOWN open orders (attempt {})", 
            open_orders_up, open_orders_down, attempt);
        
        if attempt == 3 {
            error!("🛑 Failed to cancel all orders after 3 attempts, stopping cycle");
            return Ok(());
        }
        
        // Retry cancel
        let _ = executor.cancel_orders_for_market(&up_token_id).await;
        let _ = executor.cancel_orders_for_market(&down_token_id).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }

    // CRITICAL FIX: Recalculate inventory skew after processing fills
    let inventory_skew = position_tracker.read().await.calculate_inventory_skew().await;
    info!("🔄 Recalculated inventory skew after fills: {:.2}", inventory_skew);

    // Check skip sides and get position limits for UP and DOWN separately
    // Matches Python: only limit BUY orders, SELL is always allowed
    let (
        (skip_buy_up, reason_buy_up),
        (skip_buy_down, reason_buy_down),
        buy_limit_up,
        buy_limit_down,
    ) = {
        let skew = inventory_skew; // Use the pre-calculated skew
        
        // UP logic: Skip buying UP when skew is high (UP too much)
        // Python: max_skew = order_size * 0.4 (normalized to ~0.4)
        let max_skew_threshold = 0.4;  // Changed from 0.7 to match Python
        let skip_buy_up = skew > max_skew_threshold;
        let reason_buy_up = if skip_buy_up {
            format!("UP inventory too high ({:.1}%), skip buying UP", skew * 100.0)
        } else {
            "OK to buy UP".to_string()
        };
        
        // DOWN logic: Skip buying DOWN when skew is low (DOWN too much)
        let skip_buy_down = skew < -max_skew_threshold;
        let reason_buy_down = if skip_buy_down {
            format!("DOWN inventory too high ({:.1}%), skip buying DOWN", skew.abs() * 100.0)
        } else {
            "OK to buy DOWN".to_string()
        };
        
        // Calculate limits based on skew (Python: get_position_limit)
        let base_limit = trading_config.max_position / 2.0;
        
        let buy_limit_up = if skew > 0.5 {
            base_limit * (1.0 - skew)  // Stricter when UP high
        } else {
            base_limit * (1.0 + skew.abs())  // Looser when UP low
        };
        
        let buy_limit_down = if skew < -0.5 {
            base_limit * (1.0 - skew.abs())  // Stricter when DOWN high
        } else {
            base_limit * (1.0 + skew.abs())  // Looser when DOWN low
        };
        
        (
            (skip_buy_up, reason_buy_up),
            (skip_buy_down, reason_buy_down),
            buy_limit_up,
            buy_limit_down,
        )
    };
    
    if skip_buy_up {
        info!("Skipping Buy for UP {}: {}", up_token_id, reason_buy_up);
    }
    
    if skip_buy_down {
        info!("Skipping Buy for DOWN {}: {}", down_token_id, reason_buy_down);
    }

    // Calculate prices with spread adjustment based on inventory skew
    let spread = trading_config.spread;
    
    // Adjust prices based on inventory skew
    // Matches Python: inventory_adjust = inventory_skew * 0.01
    // Both bid and ask move in same direction with skew
    let skew_adjustment = inventory_skew * 0.01; // 1% adjustment per unit of skew
    
    // UP token prices - Python: bid = mid - half_spread + inventory_adjust
    let up_bid_price = ((up_price - spread / 2.0 + skew_adjustment) * 100.0).round() / 100.0;
    let up_bid_price = up_bid_price.max(trading_config.safe_range_low).min(trading_config.safe_range_high);
    let up_ask_price = ((up_price + spread / 2.0 + skew_adjustment) * 100.0).round() / 100.0;
    let up_ask_price = up_ask_price.max(trading_config.safe_range_low).min(trading_config.safe_range_high);
    
    // DOWN token prices - same adjustment as UP
    let down_bid_price = ((down_price - spread / 2.0 + skew_adjustment) * 100.0).round() / 100.0;
    let down_bid_price = down_bid_price.max(trading_config.safe_range_low).min(trading_config.safe_range_high);
    let down_ask_price = ((down_price + spread / 2.0 + skew_adjustment) * 100.0).round() / 100.0;
    let down_ask_price = down_ask_price.max(trading_config.safe_range_low).min(trading_config.safe_range_high);

    info!("💰 Order prices - UP: bid={:.4}, ask={:.4} | DOWN: bid={:.4}, ask={:.4}",
        up_bid_price, up_ask_price, down_bid_price, down_ask_price);

    // Python-style balance check with 15% buffer
    let up_need = up_price * trading_config.order_size;
    let down_need = down_price * trading_config.order_size;
    
    // Calculate actual needs based on skew (only buy the side that needs it)
    let (actual_up_need, actual_down_need) = if inventory_skew > 0.4 {
        // UP too much, only buy DOWN
        (0.0, down_need)
    } else if inventory_skew < -0.4 {
        // DOWN too much, only buy UP
        (up_need, 0.0)
    } else {
        // Balanced, buy both
        (up_need, down_need)
    };
    
    const BUFFER_RATIO: f64 = 1.15; // 15% buffer like Python
    let total_need = (actual_up_need + actual_down_need) * BUFFER_RATIO;
    
    // Check balance
    let balance = executor.get_usdc_balance().await.unwrap_or(0.0);
    if balance < total_need {
        warn!("⚠️ Insufficient balance (with buffer): {:.2} < {:.2} (need UP:{:.2} + DOWN:{:.2} × {:.2})",
            balance, total_need, actual_up_need, actual_down_need, BUFFER_RATIO);
        warn!("⏹️ Skipping orders - need both sides for hedge");
        return Ok(());
    }
    info!("✅ Balance sufficient (with buffer): {:.2} >= {:.2}", balance, total_need);

    // Buy-and-hold strategy: Only place BUY orders on UP and DOWN tokens
    // Wait for market settlement, no active market making
    
    // Calculate actual sizes based on skew
    let base_size = trading_config.order_size;
    let remaining = trading_config.max_total_position - status.total_value;
    
    let (up_size, down_size) = if inventory_skew > 0.4 {
        // UP too much, only buy DOWN
        warn!("⚠️ UP skew too high ({:.1}), buying only DOWN to balance", inventory_skew);
        (0.0, base_size.min(remaining))
    } else if inventory_skew < -0.4 {
        // DOWN too much, only buy UP
        warn!("⚠️ DOWN skew too high ({:.1}), buying only UP to balance", inventory_skew.abs());
        (base_size.min(remaining), 0.0)
    } else {
        // Balanced, buy both
        (base_size.min(remaining / 2.0), base_size.min(remaining / 2.0))
    };
    
    if up_size == 0.0 && down_size == 0.0 {
        warn!("⏹️ No size to trade");
        return Ok(());
    }
    
    info!("📊 Will place: UP={:.1}, DOWN={:.1}", up_size, down_size);

    // Place UP buy order first (skip balance check as per Python)
    let mut placed_up = 0.0;
    let mut placed_down = 0.0;
    
    if up_size > 0.0 && !skip_buy_up {
        info!("🔍 UP: BUY @ {:.4} size={:.1}", up_bid_price, up_size);
        match executor.place_order_complete(
            &up_token_id,
            Side::Buy,
            up_bid_price,
            up_size,
            trading_config.safe_range_low,
            trading_config.safe_range_high,
        ).await {
            Ok(Some(order_id)) => {
                info!("✅ UP order placed: {}", order_id);
                placed_up = up_size;
                stats.write().await.record_order_placed(up_size);
                order_tracker.write().await.track_order(
                    up_token_id.clone(), order_id, "BUY".to_string(), up_bid_price, up_size);
            }
            Ok(None) => {
                warn!("❌ UP order failed (returned None)");
            }
            Err(e) => {
                warn!("❌ UP order failed: {}", e);
            }
        }
    }

    // Place DOWN buy order (check balance again as per Python)
    if down_size > 0.0 && !skip_buy_down {
        // Re-check balance after UP order
        let balance_after_up = executor.get_usdc_balance().await.unwrap_or(0.0);
        let down_need_now = down_bid_price * down_size * BUFFER_RATIO;
        
        if balance_after_up < down_need_now {
            warn!("⚠️ Insufficient balance for DOWN after UP order: {:.2} < {:.2}",
                balance_after_up, down_need_now);
        } else {
            info!("🔍 DOWN: BUY @ {:.4} size={:.1}", down_bid_price, down_size);
            match executor.place_order_complete(
                &down_token_id,
                Side::Buy,
                down_bid_price,
                down_size,
                trading_config.safe_range_low,
                trading_config.safe_range_high,
            ).await {
                Ok(Some(order_id)) => {
                    info!("✅ DOWN order placed: {}", order_id);
                    placed_down = down_size;
                    stats.write().await.record_order_placed(down_size);
                    order_tracker.write().await.track_order(
                        down_token_id.clone(), order_id, "BUY".to_string(), down_bid_price, down_size);
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

    info!("✅ Trading cycle completed: UP={:.1}, DOWN={:.1}", placed_up, placed_down);
    info!("⏱️ Trading cycle took: {:?}", cycle_start.elapsed());
    Ok(())
}

/// Place order for a specific side
#[allow(clippy::too_many_arguments)]
async fn place_side_order(
    executor: &TradeExecutor,
    order_tracker: &RwLock<OrderTracker>,
    trade_history: &TradeHistory,
    stats: &RwLock<TradingStats>,
    token_id: &str,
    side: Side,
    price: f64,
    size: f64,
    safe_low: f64,
    safe_high: f64,
    outcome: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Use the complete order placement with validation
    match executor.place_order_complete(
        token_id,
        side,
        price,
        size,
        safe_low,
        safe_high,
    ).await {
        Ok(Some(order_id)) => {
            stats.write().await.record_order_placed(size);
            
            order_tracker.write().await.track_order(
                token_id.to_string(),
                order_id.clone(),
                format!("{:?}", side).to_uppercase(),
                price,
                size,
            );
            
            let _ = trade_history.add_trade(
                token_id.to_string(),
                order_id.clone(),
                format!("{:?}", side).to_uppercase(),
                outcome.to_string(),
                size,
                price,
            );
            
            info!("✅ Order placed: {} {:?} {} @ {} for {}",
                order_id.clone(), side, size, price, outcome);
        }
        Ok(None) => {
            info!("⚠️ Order validation failed for {} {:?} @ {}", outcome, side, price);
            // Return error to stop trading cycle - partial fills can cause imbalance
            return Err(format!("Order validation failed for {} {:?}", outcome, side).into());
        }
        Err(e) => {
            error!("❌ {:?} order failed: {}", side, e);
            stats.write().await.record_error();
            // Return error to stop trading cycle - partial fills can cause imbalance
            return Err(format!("Order placement failed: {}", e).into());
        }
    }
    Ok(())
}
