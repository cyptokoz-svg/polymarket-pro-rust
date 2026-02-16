//! Simulation mode for testing
//! Matches Python: auto_trade flag

use serde::{Deserialize, Serialize};
use tracing::info;

/// Trading mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TradingMode {
    /// Live trading with real orders
    #[default]
    Live,
    /// Simulation mode - log only, no real orders
    Simulation,
}

impl TradingMode {
    /// Check if live trading
    pub fn is_live(&self) -> bool {
        matches!(self, TradingMode::Live)
    }
    
    /// Check if simulation mode
    pub fn is_simulation(&self) -> bool {
        matches!(self, TradingMode::Simulation)
    }
}

/// Simulation trade recorder
#[derive(Debug, Clone)]
pub struct SimulationRecorder {
    trades: Vec<SimulatedTrade>,
}

/// Simulated trade record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedTrade {
    pub token_id: String,
    pub side: String,
    pub price: f64,
    pub size: f64,
    pub timestamp: String,
}

impl SimulationRecorder {
    /// Create new recorder
    pub fn new() -> Self {
        Self {
            trades: Vec::new(),
        }
    }
    
    /// Record a simulated trade
    pub fn record_trade(
        &mut self,
        token_id: String,
        side: String,
        price: f64,
        size: f64,
    ) {
        let trade = SimulatedTrade {
            token_id,
            side,
            price,
            size,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        
        info!(
            "[SIMULATION] {} {} @ {} for {}",
            trade.side, trade.size, trade.price, trade.token_id
        );
        
        self.trades.push(trade);
    }
    
    /// Get all recorded trades
    pub fn get_trades(&self,
    ) -> &[SimulatedTrade] {
        &self.trades
    }
    
    /// Get trade count
    pub fn count(&self) -> usize {
        self.trades.len()
    }
    
    /// Clear all trades
    pub fn clear(&mut self,
    ) {
        self.trades.clear();
    }
    
    /// Save trades to file
    pub fn save_to_file(
        &self,
        filepath: &str,
    ) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(&self.trades)?;
        std::fs::write(filepath, content)?;
        Ok(())
    }
}

impl Default for SimulationRecorder {
    fn default() -> Self {
        Self::new()
    }
}