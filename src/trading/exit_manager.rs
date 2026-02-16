//! Position exit manager for take profit and stop loss
//! Matches Python: should_exit_position()

use std::time::{Duration, Instant};
use tracing::info;

/// Position with tracking info
#[derive(Debug, Clone)]
pub struct TrackedPosition {
    pub token_id: String,
    pub side: String,  // "UP" or "DOWN"
    pub size: f64,
    pub avg_price: f64,
    pub entry_time: Instant,
}

/// Exit check result
#[derive(Debug, Clone)]
pub struct ExitCheck {
    pub should_exit: bool,
    pub reason: String,
    pub pnl: f64,
}

/// Position exit manager
pub struct ExitManager {
    max_hold_time: Duration,
    exit_before_expiry: Duration,
    take_profit: f64,
    stop_loss: f64,
}

impl ExitManager {
    /// Create new exit manager
    pub fn new(
        max_hold_time_secs: u64,
        exit_before_expiry_secs: u64,
        take_profit: f64,
        stop_loss: f64,
    ) -> Self {
        Self {
            max_hold_time: Duration::from_secs(max_hold_time_secs),
            exit_before_expiry: Duration::from_secs(exit_before_expiry_secs),
            take_profit,
            stop_loss,
        }
    }
    
    /// Check if should exit position
    /// Matches Python: should_exit_position()
    pub fn check_exit(
        &self,
        position: &TrackedPosition,
        current_price: f64,
        time_to_expiry: Option<Duration>,
    ) -> ExitCheck {
        let now = Instant::now();
        let hold_time = now.duration_since(position.entry_time);
        
        // Calculate PnL
        let pnl = if position.avg_price > 0.0 {
            (current_price - position.avg_price) / position.avg_price
        } else {
            0.0
        };
        
        // 1. Time stop
        if hold_time > self.max_hold_time {
            return ExitCheck {
                should_exit: true,
                reason: format!("Time stop ({:.0}s > {:.0}s)", 
                    hold_time.as_secs(), self.max_hold_time.as_secs()),
                pnl,
            };
        }
        
        // 2. Expiry approaching
        if let Some(expiry) = time_to_expiry {
            if expiry < self.exit_before_expiry {
                return ExitCheck {
                    should_exit: true,
                    reason: format!("Expiry approaching ({:.0}s left)", expiry.as_secs()),
                    pnl,
                };
            }
        }
        
        // 3. Take profit
        if pnl >= self.take_profit {
            return ExitCheck {
                should_exit: true,
                reason: format!("Take profit (+{:.1}%)", pnl * 100.0),
                pnl,
            };
        }
        
        // 4. Stop loss
        if pnl <= -self.stop_loss {
            return ExitCheck {
                should_exit: true,
                reason: format!("Stop loss ({:.1}%)", pnl * 100.0),
                pnl,
            };
        }
        
        // Hold position
        ExitCheck {
            should_exit: false,
            reason: "Hold".to_string(),
            pnl,
        }
    }
    
    /// Log exit check
    pub fn log_check(
        &self,
        token_id: &str,
        check: &ExitCheck,
    ) {
        if check.should_exit {
            info!("📤 EXIT SIGNAL for {}: {} (PnL: {:.2}%)", 
                token_id, check.reason, check.pnl * 100.0);
        } else {
            info!("📊 HOLD {}: PnL {:.2}%", token_id, check.pnl * 100.0);
        }
    }
}

/// Position tracker with exit management
pub struct PositionExitTracker {
    positions: std::collections::HashMap<String, TrackedPosition>,
    exit_manager: ExitManager,
}

impl PositionExitTracker {
    /// Create new tracker
    pub fn new(exit_manager: ExitManager) -> Self {
        Self {
            positions: std::collections::HashMap::new(),
            exit_manager,
        }
    }
    
    /// Add or update position
    pub fn update_position(
        &mut self,
        token_id: String,
        side: String,
        size: f64,
        avg_price: f64,
    ) {
        self.positions.insert(token_id.clone(), TrackedPosition {
            token_id,
            side,
            size,
            avg_price,
            entry_time: Instant::now(),
        });
    }
    
    /// Check all positions for exit
    pub fn check_all_positions(
        &self,
        prices: &std::collections::HashMap<String, f64>,
    ) -> Vec<(String, ExitCheck)> {
        let mut exits = Vec::new();
        
        for (token_id, position) in &self.positions {
            if let Some(&price) = prices.get(token_id) {
                let check = self.exit_manager.check_exit(position, price, None);
                if check.should_exit {
                    exits.push((token_id.clone(), check));
                }
            }
        }
        
        exits
    }
    
    /// Remove position after exit
    pub fn remove_position(
        &mut self,
        token_id: &str,
    ) {
        self.positions.remove(token_id);
    }
    
    /// Get position count
    pub fn count(&self,
    ) -> usize {
        self.positions.len()
    }
}