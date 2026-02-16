//! Trading statistics
//! Matches Python: self.stats

use serde::{Deserialize, Serialize};

/// Trading statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TradingStats {
    pub start_time: String,
    pub orders_placed: u64,
    pub orders_filled: u64,
    pub orders_cancelled: u64,
    pub orders_expired: u64,
    pub errors: u64,
    pub total_volume: f64,
    pub total_pnl: f64,
    pub merge_count: u64,
    pub last_update: String,
}

impl TradingStats {
    /// Create new stats with current time
    pub fn new() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            start_time: now.clone(),
            orders_placed: 0,
            orders_filled: 0,
            orders_cancelled: 0,
            orders_expired: 0,
            errors: 0,
            total_volume: 0.0,
            total_pnl: 0.0,
            merge_count: 0,
            last_update: now,
        }
    }
    
    /// Record order placed
    pub fn record_order_placed(&mut self,
        size: f64,
    ) {
        self.orders_placed += 1;
        self.total_volume += size;
        self.update_time();
    }
    
    /// Record order filled
    pub fn record_order_filled(&mut self,
        _size: f64,
    ) {
        self.orders_filled += 1;
        self.update_time();
    }
    
    /// Record order cancelled
    pub fn record_order_cancelled(&mut self,
    ) {
        self.orders_cancelled += 1;
        self.update_time();
    }
    
    /// Record order expired (>2 min)
    pub fn record_order_expired(&mut self,
    ) {
        self.orders_expired += 1;
        self.update_time();
    }
    
    /// Record error
    pub fn record_error(&mut self,
    ) {
        self.errors += 1;
        self.update_time();
    }
    
    /// Record merge
    pub fn record_merge(&mut self,
    ) {
        self.merge_count += 1;
        self.update_time();
    }
    
    /// Update PnL
    pub fn update_pnl(&mut self,
        pnl: f64,
    ) {
        self.total_pnl += pnl;
        self.update_time();
    }
    
    /// Save stats to file with secure permissions
    /// SECURITY: Uses data directory instead of /tmp
    pub fn save_to_file(&self) -> anyhow::Result<()> {
        let filepath = Self::get_data_path("polymarket_stats.json");
        
        // Create parent directory if needed
        if let Some(parent) = filepath.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&filepath, content)?;
        
        // Set file permissions to owner-only (0o600) on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&filepath)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(&filepath, permissions)?;
        }
        
        Ok(())
    }
    
    /// Load stats from file
    /// Matches Python: load_stats()
    pub fn load_from_file() -> anyhow::Result<Self> {
        let filepath = Self::get_data_path("polymarket_stats.json");
        let content = std::fs::read_to_string(&filepath)?;
        let stats: TradingStats = serde_json::from_str(&content)?;
        Ok(stats)
    }
    
    /// Get secure data path
    fn get_data_path(filename: &str) -> std::path::PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join("polymarket-pro")
            .join(filename)
    }
    
    /// Load stats or create new
    pub fn load_or_new() -> Self {
        match Self::load_from_file() {
            Ok(stats) => {
                tracing::info!("Loaded stats from data directory");
                stats
            }
            Err(_) => {
                tracing::info!("Creating new stats");
                Self::new()
            }
        }
    }
    
    /// Get summary
    pub fn summary(&self) -> String {
        format!(
            "📊 Stats: Orders placed={}, filled={}, cancelled={}, expired={}, errors={}, volume={:.2}, PnL={:.2}, merges={}",
            self.orders_placed,
            self.orders_filled,
            self.orders_cancelled,
            self.orders_expired,
            self.errors,
            self.total_volume,
            self.total_pnl,
            self.merge_count
        )
    }
    
    fn update_time(&mut self,
    ) {
        self.last_update = chrono::Utc::now().to_rfc3339();
    }
}

/// WebSocket price freshness tracker
#[derive(Debug, Clone)]
pub struct PriceFreshness {
    last_update: Option<chrono::DateTime<chrono::Utc>>,
    max_age_secs: u64,
}

impl PriceFreshness {
    /// Create new freshness tracker
    pub fn new(max_age_secs: u64) -> Self {
        Self {
            last_update: None,
            max_age_secs,
        }
    }
    
    /// Record price update
    pub fn record_update(&mut self,
    ) {
        self.last_update = Some(chrono::Utc::now());
    }
    
    /// Check if price is fresh
    pub fn is_fresh(&self,
    ) -> bool {
        match self.last_update {
            Some(last) => {
                let elapsed = chrono::Utc::now().signed_duration_since(last).num_seconds();
                elapsed >= 0 && (elapsed as u64) < self.max_age_secs
            }
            None => false,
        }
    }
    
    /// Get age in seconds
    pub fn age_secs(&self) -> Option<u64> {
        self.last_update.map(|last| {
            chrono::Utc::now().signed_duration_since(last).num_seconds() as u64
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_trading_stats() {
        let mut stats = TradingStats::new();
        
        stats.record_order_placed(10.0);
        stats.record_order_filled(10.0);
        stats.record_order_cancelled();
        stats.record_error();
        
        assert_eq!(stats.orders_placed, 1);
        assert_eq!(stats.orders_filled, 1);
        assert_eq!(stats.orders_cancelled, 1);
        assert_eq!(stats.errors, 1);
        assert_eq!(stats.total_volume, 10.0);
    }
    
    #[test]
    fn test_price_freshness() {
        let mut freshness = PriceFreshness::new(5);
        
        assert!(!freshness.is_fresh());
        
        freshness.record_update();
        assert!(freshness.is_fresh());
    }
}