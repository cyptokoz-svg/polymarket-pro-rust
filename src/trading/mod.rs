//! Trading module
//! Market maker, position tracking, and order management

pub mod market_maker;
pub mod position;
pub mod executor;
pub mod orderbook;
pub mod order_tracker;
pub mod trade_history;
pub mod stats;
pub mod balance;
pub mod simulation;
pub mod price_warning;
pub mod errors;
pub mod exit_manager;
pub mod callbacks;

pub use market_maker::{MarketMaker, MarketMakerConfig};
pub use position::{PositionTracker, Position, PositionEntry, InventoryStatus, Action, BalanceAdjustment};
pub use executor::TradeExecutor;
pub use orderbook::{OrderBookDepth, OrderBookLevel, analyze_order_book_depth_safe, calculate_mm_prices};
pub use order_tracker::{OrderTracker, ActiveOrder, FillStatus, wait_for_fill};
pub use trade_history::{TradeHistory, TradeRecord};
pub use stats::{TradingStats, PriceFreshness};
pub use balance::{AccountBalance, PositionInfo, get_usdc_balance, get_positions, get_total_position_size};
pub use simulation::{TradingMode, SimulationRecorder, SimulatedTrade};
pub use price_warning::PriceWarningTracker;
pub use errors::{TradingError, classify_error};
pub use exit_manager::{ExitManager, PositionExitTracker, TrackedPosition, ExitCheck};
pub use callbacks::{CallbackManager, OrderInfo};

