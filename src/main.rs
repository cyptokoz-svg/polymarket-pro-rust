//! Polymarket Pro - Main entry point (Simplified)

use anyhow::Result;
use polymarket_pro::*;
use polymarket_pro::trading::PriceWarningTracker;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn, Level};

mod btc_market;
use btc_market::find_btc_5min_market;

#[tokio::main]
async fn main() -> Result<()> {
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

    let executor = Arc::new(
        TradeExecutor::new(
            &config.pk,
            config.api.key.clone(),
            config.api.secret.clone(),
            config.api.passphrase.clone(),
        ).await.map_err(|e| anyhow::anyhow!("Failed to create trade executor: {}", e))?
    );
    
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
    let stats = Arc::new(RwLock::new(TradingStats::load_or_new()));

    let rate_limiter = Arc::new(utils::rate_limiter::RateLimiter::new_default());
    let price_warning_tracker = Arc::new(RwLock::new(PriceWarningTracker::new(
        config.trading.price_warn_cooldown
    )));

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

    // Main event loop
    let trading_interval = Duration::from_secs(config.trading.refresh_interval);
    let market_check_interval = Duration::from_secs(60);
    let mut trading_tick = interval(trading_interval);
    let mut market_check_tick = interval(market_check_interval);
    
    let mut current_market: Option<serde_json::Value> = None;
    
    loop {
        tokio::select! {
            // Check for new market periodically
            _ = market_check_tick.tick() => {
                if current_market.is_none() {
                    info!("🔍 Looking for BTC updown 5m market...");
                    match find_btc_5min_market(&executor).await {
                        Some(market) => {
                            info!("✅ Found BTC 5-minute market");
                            current_market = Some(market);
                        }
                        None => {
                            warn!("⚠️ No 5-minute market found");
                        }
                    }
                }
            }
            
            // Trading cycle
            _ = trading_tick.tick() => {
                if let Some(ref market) = current_market {
                    rate_limiter.wait().await;
                    
                    // Get token IDs
                    let condition_id = market.get("conditionId").and_then(|v| v.as_str()).unwrap_or("");
                    if condition_id.is_empty() {
                        warn!("No condition_id in market");
                        continue;
                    }
                    
                    // Simple trading logic
                    info!("Running trading cycle on market: {}", condition_id);
                    
                    // Check balance
                    match executor.get_usdc_balance().await {
                        Ok(balance) => {
                            info!("Balance: ${:.2}", balance);
                            
                            // Place simulated order
                            if simulation_mode {
                                info!("🎮 [SIMULATION] Would place orders here");
                            } else {
                                info!("🔴 [LIVE] Would place real orders here");
                            }
                        }
                        Err(e) => {
                            error!("Failed to get balance: {}", e);
                        }
                    }
                } else {
                    warn!("⚠️ No active market, skipping trading cycle");
                }
            }
            
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, saving state...");
                
                if let Err(e) = stats.read().await.save_to_file() {
                    error!("Failed to save stats: {}", e);
                }
                
                info!("State saved, shutting down...");
                break;
            }
        }
    }

    info!("Shutting down...");
    Ok(())
}

async fn load_config() -> Result<Config> {
    let mut config = match Config::load() {
        Ok(cfg) => cfg,
        Err(_) => Config::default(),
    };
    
    let env_config = config::from_env()?;
    
    if !env_config.pk.is_empty() {
        config.pk = env_config.pk;
    }
    if !env_config.safe_address.is_empty() {
        config.safe_address = env_config.safe_address;
    }
    
    config.validate()?;
    Ok(config)
}

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