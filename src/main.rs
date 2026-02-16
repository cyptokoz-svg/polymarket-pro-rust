//! Polymarket Pro - Main entry point
//!
//! 支持配置文件: polymarket-pro.toml, polymarket-pro.yaml, config.toml

use anyhow::Result;
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
mod market_manager;
use btc_market::find_btc_5min_market;

#[tokio::main]
async fn main() -> Result<()> {
    let config = load_config().await?;

    let log_level = config.log_level.as_deref().unwrap_or("info");
    tracing_subscriber::fmt()
        .with_max_level(parse_log_level(log_level))
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
        // Note: simulation_mode is read-only in TradeExecutor, would need to modify executor creation
    }
    
    info!("Trade executor initialized");

    match executor.get_server_time().await {
        Ok(time) => info!("Server time: {}", time),
        Err(e) => warn!("Failed to get server time: {}", e),
    }

    let position_tracker = Arc::new(RwLock::new(PositionTracker::new()));
    let order_tracker = Arc::new(RwLock::new(OrderTracker::new()));
    let trade_history = Arc::new(TradeHistory::default());

    // Initialize trading stats (will be loaded from file later)
    // Note: stats is loaded at line 113

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

    let wallet_addr = format!("{:?}", executor.signer().address());
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

    let stats = Arc::new(RwLock::new(TradingStats::load_or_new()));

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
            info!("✅ Found initial 5-minute market: {:?}", market.condition_id);
            
            // Subscribe to WebSocket for initial market and get token IDs
            if let Some(ref condition_id) = market.condition_id {
                if let Some((up_token, down_token)) = subscribe_to_market_ws(condition_id, ws_subscriber.clone()).await {
                    current_market = Some(MarketInfo {
                        market,
                        up_token,
                        down_token,
                    });
                } else {
                    warn!("⚠️ Could not get token IDs, using condition_id as fallback");
                    let condition = condition_id.clone();
                    current_market = Some(MarketInfo {
                        market,
                        up_token: condition.clone(),
                        down_token: condition,
                    });
                }
            }
        }
        None => {
            warn!("⚠️ No 5-minute market found on startup, will retry in 60s");
        }
    }
    
    loop {
        tokio::select! {
            // Check for new market periodically
            _ = market_check_tick.tick() => {
                // If no current market or current market expired, find new one
                let need_new_market = if let Some(ref market_info) = current_market {
                    if let Some(ref end_date_str) = market_info.market.end_date {
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
                    match find_btc_5min_market(&executor).await {
                        Some(new_market) => {
                            let new_condition_id = new_market.condition_id.clone();
                            let old_condition_id = current_market.as_ref().and_then(|m| m.market.condition_id.clone());
                            
                            if old_condition_id != new_condition_id {
                                info!("✅ Found new 5-minute market: {:?}", new_condition_id);
                                
                                // Cancel orders on old market if exists
                                if let Some(ref old_market_info) = current_market {
                                    info!("📤 Unsubscribing from old market: {}", old_market_info.up_token);
                                    let _ = executor.cancel_orders_for_market(&old_market_info.up_token).await;
                                }
                                
                                // Update WebSocket subscription for new market and get token IDs
                                if let Some(ref condition_id) = new_condition_id {
                                    if let Some((up_token, down_token)) = subscribe_to_market_ws(condition_id, ws_subscriber.clone()).await {
                                        current_market = Some(MarketInfo {
                                            market: new_market,
                                            up_token,
                                            down_token,
                                        });
                                    } else {
                                        warn!("⚠️ Could not get token IDs for new market");
                                        let condition = condition_id.clone();
                                        current_market = Some(MarketInfo {
                                            market: new_market,
                                            up_token: condition.clone(),
                                            down_token: condition,
                                        });
                                    }
                                }
                            }
                        }
                        None => {
                            warn!("⚠️ No 5-minute market found");
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
                if let Some(ref end_date_str) = market_info.market.end_date {
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

/// Run one trading cycle
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
async fn run_trading_cycle(
    executor: Arc<TradeExecutor>,
    ws: Option<Arc<PolymarketWebSocket>>,
    position_tracker: Arc<RwLock<PositionTracker>>,
    order_tracker: Arc<RwLock<OrderTracker>>,
    trade_history: Arc<TradeHistory>,
    stats: Arc<RwLock<TradingStats>>,
    price_freshness: Arc<RwLock<PriceFreshness>>,
    price_warning_tracker: Arc<RwLock<PriceWarningTracker>>,
    trading_config: &TradingConfig,
    markets: &[rs_clob_client::Market], // Added: cached markets
) -> Result<()> {
    info!("Running trading cycle...");

    // Check WebSocket price freshness
    let ws_fresh = price_freshness.read().await.is_fresh();
    if !ws_fresh {
        warn!("⚠️ WebSocket prices may be stale");
    }

    // Use cached markets (already loaded every 5 minutes like Python)
    info!("Trading on {} cached markets", markets.len());

    // Calculate inventory skew for price adjustment
    let inventory_skew = position_tracker.read().await.calculate_inventory_skew().await;
    info!("Current inventory skew: {:.2}", inventory_skew);

    // Log inventory status
    let status = position_tracker.read().await.get_inventory_status().await;
    info!("📊 Inventory: UP=${:.2} | DOWN=${:.2} | Total=${:.2} | Skew={:.1}%",
        status.up_value, status.down_value, status.total_value, status.skew * 100.0);

    // Check total position limit
    if status.total_value >= trading_config.max_total_position {
        warn!("Total position limit reached: ${:.2} >= ${:.2}",
            status.total_value, trading_config.max_total_position);
        return Ok(());
    }

    // Place orders for top active markets (matches Python)
    for market in markets.iter().take(5) {
        let token_id = match &market.condition_id {
            Some(id) => id.clone(),
            None => continue,
        };

        // Check if market is expired (Python: skip expired markets)
        if let Some(ref end_date_str) = market.end_date {
            if let Some(end_date) = parse_market_end_time(end_date_str) {
                let now = chrono::Utc::now();
                let time_to_expiry = end_date.signed_duration_since(now).num_seconds();
                
                if time_to_expiry <= 0 {
                    info!("⏭️ Skipping expired market: {}", token_id);
                    continue;
                }
            }
        }

        // Check merge opportunity first (Python feature)
        if let Some(merge_amount) = position_tracker.read().await.check_merge_opportunity(
            &token_id, trading_config.merge_threshold) {
            info!("💡 Merge opportunity: {:.2} shares for {}", merge_amount, token_id);
            stats.write().await.record_merge();
        }

        // Get price from WebSocket or API
        let ws_ref = ws.as_ref().map(|arc| arc.as_ref());
        let price = match get_market_price(market, ws_ref).await {
            Some(p) => {
                info!("💰 Got price for {}: {:.4} (from WebSocket)", token_id, p);
                p
            }
            None => {
                warn!("⚠️ No WebSocket price for {}, trying API", token_id);
                continue;
            }
        };

        // Validate price range with min/max price (Python style)
        if price < trading_config.min_price || price > trading_config.max_price {
            warn!("Price {:.4} outside valid range [{:.2}, {:.2}]",
                price, trading_config.min_price, trading_config.max_price);
            continue;
        }

        // Check if price is in safe range (warning but allow, with cooldown)
        if price < trading_config.safe_range_low {
            price_warning_tracker.write().await.log_price_warning(
                price, "below", trading_config.safe_range_low, trading_config.safe_range_high, ""
            );
        } else if price > trading_config.safe_range_high {
            price_warning_tracker.write().await.log_price_warning(
                price, "above", trading_config.safe_range_low, trading_config.safe_range_high, ""
            );
        }

        // Cancel orders for this specific market first (Python style)
        if let Err(e) = executor.cancel_orders_for_market(&token_id).await {
            warn!("Failed to cancel orders for {}: {}", token_id, e);
        }

        // Clear tracked orders for this token to prevent single-leg accumulation
        order_tracker.write().await.clear_orders_for_token(&token_id);

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check if should skip Buy side (Python: should_skip_side)
        let (skip_buy, reason_buy) = position_tracker.read().await.should_skip_side(Side::Buy).await;
        if skip_buy {
            info!("Skipping Buy for {}: {}", token_id, reason_buy);
        }

        // Check if should skip Sell side (Python: should_skip_side)
        let (skip_sell, reason_sell) = position_tracker.read().await.should_skip_side(Side::Sell).await;
        if skip_sell {
            info!("Skipping Sell for {}: {}", token_id, reason_sell);
        }

        // Get dynamic position limits (Python: get_position_limit)
        let buy_limit = position_tracker.read().await.get_position_limit(Side::Buy, trading_config.max_position).await;
        let sell_limit = position_tracker.read().await.get_position_limit(Side::Sell, trading_config.max_position).await;

        // Use order book depth analysis if available and sufficient, otherwise fallback
        let (bid_price, ask_price) = if let Some((bids, asks)) = get_order_book(&executor, &token_id).await {
            if let Some(depth) = trading::analyze_order_book_depth_safe(
                &bids,
                &asks,
                10.0,
                trading_config.depth_lookback as usize
            ) {
                trading::calculate_mm_prices(
                    &depth,
                    inventory_skew,
                    trading_config.min_spread,
                    trading_config.max_spread,
                )
            } else {
                // Order book data insufficient, fallback to WebSocket/API price
                info!("Order book data insufficient for {}, using WebSocket/API price", token_id);
                let spread = trading_config.spread;
                let bid_price = (price - spread / 2.0).max(trading_config.safe_range_low);
                let ask_price = (price + spread / 2.0).min(trading_config.safe_range_high);
                (bid_price, ask_price)
            }
        } else {
            // Fallback to simple calculation using WebSocket/API price
            let spread = trading_config.spread;
            let bid_price = (price - spread / 2.0).max(trading_config.safe_range_low);
            let ask_price = (price + spread / 2.0).min(trading_config.safe_range_high);
            (bid_price, ask_price)
        };

        // Place orders concurrently (matches Python: asyncio.gather)
        let buy_task = async {
            if !skip_buy {
                let size = trading_config.order_size.min(buy_limit);
                place_side_order(
                    &executor,
                    &order_tracker,
                    &trade_history,
                    &stats,
                    &token_id,
                    Side::Buy,
                    bid_price,
                    size,
                    trading_config.safe_range_low,
                    trading_config.safe_range_high,
                    "Yes",
                ).await
            } else {
                Ok(())
            }
        };

        let sell_task = async {
            if !skip_sell {
                let size = trading_config.order_size.min(sell_limit);
                place_side_order(
                    &executor,
                    &order_tracker,
                    &trade_history,
                    &stats,
                    &token_id,
                    Side::Sell,
                    ask_price,
                    size,
                    trading_config.safe_range_low,
                    trading_config.safe_range_high,
                    "No",
                ).await
            } else {
                Ok(())
            }
        };

        // Execute both orders concurrently
        let (buy_result, sell_result) = tokio::join!(buy_task, sell_task);

        if let Err(e) = buy_result {
            error!("Buy order task failed: {}", e);
        }
        if let Err(e) = sell_result {
            error!("Sell order task failed: {}", e);
        }
    }

    info!("Trading cycle completed");
    Ok(())
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
    info!("Running trading cycle on single market...");
    let cycle_start = Instant::now();

    // Use UP token for trading (represents the market)
    let token_id = market_info.up_token.clone();
    let _condition_id = market_info.market.condition_id.clone().unwrap_or_default();
    // Check WebSocket price freshness
    let ws_fresh = price_freshness.read().await.is_fresh();
    if !ws_fresh {
        warn!("⚠️ WebSocket prices may be stale");
    }

    // Calculate inventory skew for price adjustment
    let inventory_skew = position_tracker.read().await.calculate_inventory_skew().await;
    info!("Current inventory skew: {:.2}", inventory_skew);

    // Log inventory status and check position limit in one read lock
    let (status, should_return) = {
        let tracker = position_tracker.read().await;
        let status = tracker.get_inventory_status().await;
        let should_return = status.total_value >= trading_config.max_total_position;
        (status, should_return)
    };
    
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
        &token_id, trading_config.merge_threshold) {
        info!("💡 Merge opportunity: {:.2} shares for {}", merge_amount, token_id);
        stats.write().await.record_merge();
    }

    // Get price from WebSocket or API
    let price = if let Some(ref ws) = ws {
        get_market_price(&market_info.market, Some(ws)).await
    } else {
        get_market_price(&market_info.market, None).await
    };
    let price = match price {
        Some(p) => p,
        None => {
            warn!("No price available for {}, skipping", token_id);
            return Ok(());
        }
    };

    // Validate price range with min/max price (Python style)
    if price < trading_config.min_price || price > trading_config.max_price {
        warn!("Price {:.4} outside valid range [{:.2}, {:.2}]",
            price, trading_config.min_price, trading_config.max_price);
        return Ok(());
    }

    // Check if price is in safe range (warning but allow, with cooldown)
    if price < trading_config.safe_range_low {
        price_warning_tracker.write().await.log_price_warning(
            price, "below", trading_config.safe_range_low, trading_config.safe_range_high, ""
        );
    } else if price > trading_config.safe_range_high {
        price_warning_tracker.write().await.log_price_warning(
            price, "above", trading_config.safe_range_low, trading_config.safe_range_high, ""
        );
    }

    // CRITICAL FIX: Check for fills BEFORE cancelling to avoid double-counting
    // Step 1: Get tracked order IDs for this token
    let tracked_orders: Vec<String> = {
        let tracker = order_tracker.read().await;
        tracker.get_all_orders()
            .values()
            .filter(|o| o.token == token_id)
            .map(|o| o.order_id.clone())
            .collect()
    };
    
    // Step 2: Check which orders have filled (no longer in open orders)
    let filled_order_ids = if !tracked_orders.is_empty() {
        match executor.get_filled_orders(&token_id, &tracked_orders).await {
            Ok(filled) => {
                if !filled.is_empty() {
                    info!("🎯 Detected {} filled orders for {}: {:?}", 
                        filled.len(), token_id, filled);
                }
                filled
            }
            Err(e) => {
                warn!("⚠️ Failed to check filled orders for {}: {}", token_id, e);
                vec![]
            }
        }
    } else {
        vec![]
    };
    
    // Step 3: Update positions for filled orders (simulate fill for now)
    for filled_id in &filled_order_ids {
        // Find the tracked order details
        if let Some(order) = order_tracker.read().await.get_all_orders()
            .values()
            .find(|o| o.order_id == *filled_id && o.token == token_id) {
            
            let side = if order.side == "BUY" { Side::Buy } else { Side::Sell };
            let size = order.size;
            let price = order.price;
            
            info!("📈 Updating position for filled order {}: {:?} {} @ {}", 
                filled_id, side, size, price);
            
            // Update position tracker with the fill
            position_tracker.write().await.update_position(
                &token_id,
                side,
                size,
                price,
            ).await;
            
            // Record in stats
            stats.write().await.record_order_filled(size);
        }
    }
    
    // Step 4: Now cancel remaining open orders
    match executor.cancel_orders_for_market(&token_id).await {
        Ok(result) => {
            info!("✅ Cancelled {} existing orders for {}", result.cancelled, token_id);
        }
        Err(e) => {
            error!("❌ Failed to cancel orders for {}: {}", token_id, e);
            error!("🛑 Stopping trading cycle to avoid duplicate orders");
            return Ok(()); // Stop this trading cycle
        }
    }

    // Step 5: Clear tracked orders for this token (including filled ones)
    order_tracker.write().await.clear_orders_for_token(&token_id);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // CRITICAL FIX: Recalculate inventory skew after processing fills
    // This ensures we don't double-count positions
    let inventory_skew = position_tracker.read().await.calculate_inventory_skew().await;
    info!("🔄 Recalculated inventory skew after fills: {:.2}", inventory_skew);

    // Check skip sides and get position limits in one read lock
    let ((skip_buy, reason_buy), (skip_sell, reason_sell), buy_limit, sell_limit) = {
        let tracker = position_tracker.read().await;
        let buy_check = tracker.should_skip_side(Side::Buy).await;
        let sell_check = tracker.should_skip_side(Side::Sell).await;
        let buy_lim = tracker.get_position_limit(Side::Buy, trading_config.max_position).await;
        let sell_lim = tracker.get_position_limit(Side::Sell, trading_config.max_position).await;
        (buy_check, sell_check, buy_lim, sell_lim)
    };
    
    if skip_buy {
        info!("Skipping Buy for {}: {}", token_id, reason_buy);
    }
    
    if skip_sell {
        info!("Skipping Sell for {}: {}", token_id, reason_sell);
    }

    // Use order book depth analysis if available and sufficient, otherwise fallback
    let (bid_price, ask_price) = if let Some((bids, asks)) = get_order_book(&executor, &token_id).await {
        if let Some(depth) = trading::analyze_order_book_depth_safe(
            &bids,
            &asks,
            10.0,
            trading_config.depth_lookback as usize
        ) {
            trading::calculate_mm_prices(
                &depth,
                inventory_skew,
                trading_config.min_spread,
                trading_config.max_spread,
            )
        } else {
            // Order book data insufficient, fallback to WebSocket/API price
            info!("Order book data insufficient for {}, using WebSocket/API price", token_id);
            let spread = trading_config.spread;
            let bid_price = (price - spread / 2.0).max(trading_config.safe_range_low);
            let ask_price = (price + spread / 2.0).min(trading_config.safe_range_high);
            (bid_price, ask_price)
        }
    } else {
        // Fallback to simple calculation using WebSocket/API price
        let spread = trading_config.spread;
        let bid_price = (price - spread / 2.0).max(trading_config.safe_range_low);
        let ask_price = (price + spread / 2.0).min(trading_config.safe_range_high);
        (bid_price, ask_price)
    };

    info!("💰 Prices for {}: bid={:.4}, ask={:.4}, mid={:.4}",
        token_id, bid_price, ask_price, price);

    // Place orders concurrently (matches Python: asyncio.gather)
    let buy_task = async {
        if !skip_buy {
            let size = trading_config.order_size.min(buy_limit);
            place_side_order(
                &executor,
                &order_tracker,
                &trade_history,
                &stats,
                &token_id,
                Side::Buy,
                bid_price,
                size,
                trading_config.safe_range_low,
                trading_config.safe_range_high,
                "Yes",
            ).await
        } else {
            Ok(())
        }
    };

    let sell_task = async {
        if !skip_sell {
            let size = trading_config.order_size.min(sell_limit);
            place_side_order(
                &executor,
                &order_tracker,
                &trade_history,
                &stats,
                &token_id,
                Side::Sell,
                ask_price,
                size,
                trading_config.safe_range_low,
                trading_config.safe_range_high,
                "No",
            ).await
        } else {
            Ok(())
        }
    };

    // Execute both orders concurrently
    let (buy_result, sell_result) = tokio::join!(buy_task, sell_task);

    if let Err(e) = buy_result {
        error!("Buy order task failed: {}", e);
    }
    if let Err(e) = sell_result {
        error!("Sell order task failed: {}", e);
    }

    info!("Trading cycle completed for market {}", token_id);
    info!("⏱️ Trading cycle took: {:?}", cycle_start.elapsed());
    Ok(())
}

/// Market info with token IDs
#[derive(Debug, Clone)]
struct MarketInfo {
    market: rs_clob_client::Market,
    up_token: String,
    #[allow(dead_code)]
    down_token: String,
}

/// Subscribe to WebSocket for a market and return token IDs
async fn subscribe_to_market_ws(
    condition_id: &str,
    ws_subscriber: Option<Arc<PolymarketWebSocket>>,
) -> Option<(String, String)> {
    if let Some(ref ws) = ws_subscriber {
        info!("📡 Fetching token IDs for market: {}", condition_id);
        
        // Get UP and DOWN token IDs from Gamma API
        match btc_market::get_market_token_ids(condition_id).await {
            Some((up_token, down_token)) => {
                info!("📡 Subscribing to UP: {}, DOWN: {}", 
                    &up_token[..20.min(up_token.len())], 
                    &down_token[..20.min(down_token.len())]);
                
                let token_ids = vec![up_token.clone(), down_token.clone()];
                ws.update_subscription(token_ids).await;
                
                // Set token labels
                let mut labels = std::collections::HashMap::new();
                labels.insert(up_token.clone(), "UP".to_string());
                labels.insert(down_token.clone(), "DOWN".to_string());
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
async fn get_market_price(
    _market: &rs_clob_client::Market,
    ws: Option<&PolymarketWebSocket>,
) -> Option<f64> {
    // Try WebSocket first - with retry for initial data
    if let Some(ws) = ws {
        // Wait up to 3 seconds for WebSocket data to arrive
        for _ in 0..30 {
            let all_prices = ws.get_all_prices().await;
            if !all_prices.is_empty() {
                // Calculate average of all bid/ask mid prices
                let mut total_mid = 0.0;
                let mut count = 0;
                for (_token, (bid, ask)) in &all_prices {
                    total_mid += (bid + ask) / 2.0;
                    count += 1;
                }
                if count > 0 {
                    let avg = total_mid / count as f64;
                    return Some(avg);
                }
            }
            // Wait 100ms before retry
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        warn!("⚠️ WebSocket prices not available after 3s");
    }

    // Fallback to API price
    let prices_str = _market.outcome_prices.as_ref()?;
    let prices: Vec<f64> = serde_json::from_str(prices_str).ok()?;
    prices.first().copied()
}

/// Get order book for a market (if available)
async fn get_order_book(
    executor: &TradeExecutor,
    token_id: &str,
) -> Option<(Vec<serde_json::Value>, Vec<serde_json::Value>)> {
    match executor.get_order_book(token_id).await {
        Ok((bids, asks)) => {
            let bids_vec: Vec<serde_json::Value> = serde_json::from_value(bids).ok()?;
            let asks_vec: Vec<serde_json::Value> = serde_json::from_value(asks).ok()?;
            Some((bids_vec, asks_vec))
        }
        Err(e) => {
            tracing::debug!("Failed to get order book: {}", e);
            None
        }
    }
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
    match executor.place_order_with_validation(
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
                order_id,
                format!("{:?}", side).to_uppercase(),
                price,
                size,
            );
            
            let _ = trade_history.add_trade(
                token_id.to_string(),
                "unknown".to_string(),
                format!("{:?}", side).to_uppercase(),
                outcome.to_string(),
                size,
                price,
            );
        }
        Ok(None) => {}
        Err(e) => {
            error!("{:?} order failed: {}", side, e);
            stats.write().await.record_error();
        }
    }
    Ok(())
}

/// Batch cancel orders for multiple tokens
/// Matches Python: batch_cancel_and_create() step 2
#[allow(dead_code)]
async fn batch_cancel_orders(
    executor: &TradeExecutor,
    token_ids: &[String],
) -> Vec<(String, bool)> {
    let mut results = Vec::new();
    
    for token_id in token_ids {
        match executor.cancel_orders_for_market(token_id).await {
            Ok(_) => {
                info!("✅ Cancelled orders for {}", token_id);
                results.push((token_id.clone(), true));
            }
            Err(e) => {
                error!("❌ Failed to cancel orders for {}: {}", token_id, e);
                results.push((token_id.clone(), false));
            }
        }
    }
    
    // Wait for cancellations to take effect (like Python's time.sleep(0.1))
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    results
}

/// Batch place orders for multiple sides
/// Matches Python: batch_cancel_and_create() step 3
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
async fn batch_place_orders(
    executor: &TradeExecutor,
    order_tracker: &RwLock<OrderTracker>,
    trade_history: &TradeHistory,
    stats: &RwLock<TradingStats>,
    orders: Vec<(String, Side, f64, f64, &'static str)>,  // (token_id, side, price, size, outcome)
    safe_low: f64,
    safe_high: f64,
) -> Vec<Result<(), Box<dyn std::error::Error>>> {
    let mut results = Vec::new();
    let mut success_count = 0;
    let total = orders.len();
    
    for (token_id, side, price, size, outcome) in orders {
        let result = place_side_order(
            executor,
            order_tracker,
            trade_history,
            stats,
            &token_id,
            side,
            price,
            size,
            safe_low,
            safe_high,
            outcome,
        ).await;
        
        if result.is_ok() {
            success_count += 1;
        }
        results.push(result);
    }
    
    info!("📊 Batch operation complete: {}/{} orders placed", success_count, total);
    
    results
}