//! Dynamic configuration reload
//! Matches Python: update_config()

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use crate::config::Config;

/// Configuration manager with hot reload support
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    config_path: String,
}

impl ConfigManager {
    /// Create new config manager
    pub fn new(
        config: Config,
        config_path: String,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        }
    }
    
    /// Get current config
    pub async fn get_config(&self,
    ) -> Config {
        self.config.read().await.clone()
    }
    
    /// Update specific config values
    /// Matches Python: update_config(**kwargs)
    pub async fn update(&self,
        updates: ConfigUpdates,
    ) -> anyhow::Result<()> {
        let mut config = self.config.write().await;
        
        // Update trading parameters
        if let Some(order_size) = updates.order_size {
            let old = config.trading.order_size;
            config.trading.order_size = order_size;
            info!("📝 Config updated: order_size = {} (was {})", order_size, old);
        }
        
        if let Some(max_position) = updates.max_position {
            let old = config.trading.max_position;
            config.trading.max_position = max_position;
            info!("📝 Config updated: max_position = {} (was {})", max_position, old);
        }
        
        if let Some(max_total_position) = updates.max_total_position {
            let old = config.trading.max_total_position;
            config.trading.max_total_position = max_total_position;
            info!("📝 Config updated: max_total_position = {} (was {})", max_total_position, old);
        }
        
        if let Some(safe_range_low) = updates.safe_range_low {
            let old = config.trading.safe_range_low;
            config.trading.safe_range_low = safe_range_low;
            info!("📝 Config updated: safe_range_low = {} (was {})", safe_range_low, old);
        }
        
        if let Some(safe_range_high) = updates.safe_range_high {
            let old = config.trading.safe_range_high;
            config.trading.safe_range_high = safe_range_high;
            info!("📝 Config updated: safe_range_high = {} (was {})", safe_range_high, old);
        }
        
        if let Some(take_profit) = updates.take_profit {
            let old = config.trading.take_profit;
            config.trading.take_profit = take_profit;
            info!("📝 Config updated: take_profit = {} (was {})", take_profit, old);
        }
        
        if let Some(stop_loss) = updates.stop_loss {
            let old = config.trading.stop_loss;
            config.trading.stop_loss = stop_loss;
            info!("📝 Config updated: stop_loss = {} (was {})", stop_loss, old);
        }
        
        if let Some(max_hold_time) = updates.max_hold_time {
            let old = config.trading.max_hold_time;
            config.trading.max_hold_time = max_hold_time;
            info!("📝 Config updated: max_hold_time = {} (was {})", max_hold_time, old);
        }
        
        // Validate after update
        config.validate()?;
        
        Ok(())
    }
    
    /// Reload config from file
    pub async fn reload(&self,
    ) -> anyhow::Result<()> {
        info!("🔄 Reloading configuration from {}", self.config_path);
        
        let new_config = Config::from_file(&self.config_path)?;
        new_config.validate()?;
        
        let mut config = self.config.write().await;
        *config = new_config;
        
        info!("✅ Configuration reloaded successfully");
        Ok(())
    }
    
    /// Save current config to file
    pub async fn save(&self,
    ) -> anyhow::Result<()> {
        let config = self.config.read().await;
        config.save_to_file(&self.config_path)?;
        info!("💾 Configuration saved to {}", self.config_path);
        Ok(())
    }
}

/// Config update parameters
#[derive(Debug, Clone, Default)]
pub struct ConfigUpdates {
    pub order_size: Option<f64>,
    pub max_position: Option<f64>,
    pub max_total_position: Option<f64>,
    pub safe_range_low: Option<f64>,
    pub safe_range_high: Option<f64>,
    pub take_profit: Option<f64>,
    pub stop_loss: Option<f64>,
    pub max_hold_time: Option<u64>,
}

impl ConfigUpdates {
    /// Create empty updates
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set order size
    pub fn with_order_size(
        mut self,
        value: f64,
    ) -> Self {
        self.order_size = Some(value);
        self
    }
    
    /// Set max position
    pub fn with_max_position(
        mut self,
        value: f64,
    ) -> Self {
        self.max_position = Some(value);
        self
    }
    
    /// Set take profit
    pub fn with_take_profit(
        mut self,
        value: f64,
    ) -> Self {
        self.take_profit = Some(value);
        self
    }
    
    /// Set stop loss
    pub fn with_stop_loss(
        mut self,
        value: f64,
    ) -> Self {
        self.stop_loss = Some(value);
        self
    }
}