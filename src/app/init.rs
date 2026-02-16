//! Application initialization
//! Handles config loading, logging setup, and component initialization

use anyhow::Result;
use polymarket_pro::*;
use polymarket_pro::trading::PriceWarningTracker;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn, Level};

/// Application components container
pub struct AppComponents {
    pub executor: Arc<TradeExecutor>,
    pub position_tracker: Arc<RwLock<PositionTracker>>,
    pub order_tracker: Arc<RwLock<OrderTracker>>,
    pub trade_history: Arc<TradeHistory>,
    pub stats: Arc<RwLock<TradingStats>>,
    pub rate_limiter: Arc<utils::rate_limiter::RateLimiter>,
    pub price_warning_tracker: Arc<RwLock<PriceWarningTracker>>,
    pub price_freshness: Arc<RwLock<PriceFreshness>>,
    pub ws_prices: Arc<RwLock<HashMap<String, f64>>>,
}

/// Initialize logging
pub fn init_logging(log_level: &str) {
    tracing_subscriber::fmt()
        .with_max_level(parse_log_level(log_level))
        .init();
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

/// Initialize trade executor
pub async fn init_executor(config: &Config) -> Result<Arc<TradeExecutor>> {
    let executor = Arc::new(
        TradeExecutor::new(
            &config.pk,
            config.api.key.clone(),
            config.api.secret.clone(),
            config.api.passphrase.clone(),
        ).map_err(|e| anyhow::anyhow!("Failed to create trade executor: {}", e))?
    );
    
    // Test connection
    match executor.get_server_time().await {
        Ok(time) => info!("Server time: {}", time),
        Err(e) => warn!("Failed to get server time: {}", e),
    }
    
    Ok(executor)
}

/// Initialize all components
pub async fn init_components(config: &Config) -> Result<AppComponents> {
    let executor = init_executor(config).await?;
    
    let position_tracker = Arc::new(RwLock::new(PositionTracker::new()));
    let order_tracker = Arc::new(RwLock::new(OrderTracker::new()));
    let trade_history = Arc::new(TradeHistory::default());
    let stats = Arc::new(RwLock::new(TradingStats::load_or_new("/tmp/polymarket_stats.json")));
    let rate_limiter = Arc::new(utils::rate_limiter::RateLimiter::new_default());
    let price_warning_tracker = Arc::new(RwLock::new(PriceWarningTracker::new(
        config.trading.price_warn_cooldown
    )));
    let price_freshness = Arc::new(RwLock::new(PriceFreshness::new(5)));
    let ws_prices = Arc::new(RwLock::new(HashMap::new()));
    
    Ok(AppComponents {
        executor,
        position_tracker,
        order_tracker,
        trade_history,
        stats,
        rate_limiter,
        price_warning_tracker,
        price_freshness,
        ws_prices,
    })
}