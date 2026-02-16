//! Polymarket Pro - High-performance trading bot in Rust
//! 
//! Features:
//! - Asynchronous order execution
//! - WebSocket real-time data
//! - Gnosis Safe integration
//! - Builder Relayer gasless transactions
//! - Automated market making

pub mod api;
pub mod trading;
pub mod wallet;
pub mod websocket;
pub mod redeem;
pub mod utils;
pub mod config;

// Re-export commonly used types
pub use api::{GammaApiClient, ClobClient, ClobApiClient, Order, Side, OrderStatus, OrderResponse};
pub use trading::{
    MarketMaker, MarketMakerConfig, 
    PositionTracker, Position, PositionEntry, InventoryStatus, Action, BalanceAdjustment,
    TradeExecutor,
    OrderBookDepth, OrderBookLevel, analyze_order_book_depth_safe, calculate_mm_prices,
    OrderTracker, ActiveOrder, FillStatus, wait_for_fill,
    TradeHistory, TradeRecord,
    TradingStats, PriceFreshness,
};
pub use wallet::{PrivateKeyWallet, SafeWallet, Wallet, RedeemTypedData};
pub use websocket::{PolymarketWebSocket, PriceUpdate};
pub use redeem::{BuilderRelayer, AutoRedeemService, SettledMarket, RedeemResult};
pub use config::{Config, ApiConfig, TradingConfig, WebSocketConfig};
pub use utils::{retry, rate_limiter};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PolymarketError {
    #[error("Wallet error: {0}")]
    Wallet(#[from] wallet::WalletError),
    #[error("API error: {0}")]
    Api(#[from] api::ApiError),
    #[error("Trading error: {0}")]
    Trading(#[from] trading::TradingError),
}

/// Bot version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");