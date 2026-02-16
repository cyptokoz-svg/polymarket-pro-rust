//! Trade history tracking
//! Matches Python: load_trade_history(), save_trade_history()

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

/// Trade record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub condition_id: String,
    pub market_slug: String,
    pub side: String,
    pub outcome: String,
    pub size: f64,
    pub price: f64,
    pub timestamp: String,
    pub redeemed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redeemed_at: Option<String>,
}

/// Trade history manager
pub struct TradeHistory {
    file_path: String,
}

impl TradeHistory {
    /// Create new trade history manager
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }
}

impl Default for TradeHistory {
    fn default() -> Self {
        Self::new("/tmp/polymarket_trade_history.json")
    }
}

impl TradeHistory {
    /// Load trade history
    /// Matches Python: load_trade_history()
    pub fn load(&self,
    ) -> Vec<TradeRecord> {
        let path = Path::new(&self.file_path);
        
        if !path.exists() {
            info!("No trade history file found, starting fresh");
            return Vec::new();
        }
        
        match std::fs::read_to_string(path) {
            Ok(content) => {
                match serde_json::from_str::<Vec<TradeRecord>>(&content) {
                    Ok(records) => {
                        info!("Loaded {} trade records", records.len());
                        records
                    }
                    Err(e) => {
                        warn!("Failed to parse trade history: {}", e);
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read trade history: {}", e);
                Vec::new()
            }
        }
    }
    
    /// Save trade history with secure permissions
    pub fn save(
        &self,
        records: &[TradeRecord],
    ) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(records)?;
        std::fs::write(&self.file_path, content)?;
        
        // Set file permissions to owner-only (0o600) on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&self.file_path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(&self.file_path, permissions)?;
        }
        
        info!("Saved {} trade records", records.len());
        Ok(())
    }
    
    /// Add a new trade record
    pub fn add_trade(
        &self,
        condition_id: String,
        market_slug: String,
        side: String,
        outcome: String,
        size: f64,
        price: f64,
    ) -> anyhow::Result<()> {
        let mut records = self.load();
        
        let record = TradeRecord {
            condition_id,
            market_slug,
            side,
            outcome,
            size,
            price,
            timestamp: chrono::Utc::now().to_rfc3339(),
            redeemed: false,
            redeemed_at: None,
        };
        
        records.push(record);
        self.save(&records)?;
        
        Ok(())
    }
    
    /// Mark trade as redeemed
    pub fn mark_redeemed(
        &self,
        condition_id: &str,
    ) -> anyhow::Result<bool> {
        let mut records = self.load();
        let mut found = false;
        
        for record in &mut records {
            if record.condition_id == condition_id {
                record.redeemed = true;
                record.redeemed_at = Some(chrono::Utc::now().to_rfc3339());
                found = true;
                break;
            }
        }
        
        if found {
            self.save(&records)?;
            info!("Marked {} as redeemed", condition_id);
        }
        
        Ok(found)
    }
    
    /// Get pending (not redeemed) trades
    pub fn get_pending(&self,
    ) -> Vec<TradeRecord> {
        self.load()
            .into_iter()
            .filter(|r| !r.redeemed)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    
    fn create_temp_history() -> TradeHistory {
        let path = "/tmp/test_trade_history.json";
        let _ = fs::remove_file(path);
        TradeHistory::new(path)
    }
    
    #[test]
    fn test_load_empty() {
        let history = create_temp_history();
        let records = history.load();
        assert!(records.is_empty());
    }
    
    #[test]
    fn test_save_and_load() {
        let history = create_temp_history();
        
        let records = vec![
            TradeRecord {
                condition_id: "0x123".to_string(),
                market_slug: "test-market".to_string(),
                side: "BUY".to_string(),
                outcome: "Yes".to_string(),
                size: 10.0,
                price: 0.5,
                timestamp: chrono::Utc::now().to_rfc3339(),
                redeemed: false,
                redeemed_at: None,
            },
        ];
        
        history.save(&records).unwrap();
        
        let loaded = history.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].condition_id, "0x123");
        
        // Cleanup
        let _ = fs::remove_file("/tmp/test_trade_history.json");
    }
    
    #[test]
    fn test_mark_redeemed() {
        let history = create_temp_history();
        
        history.add_trade(
            "0x123".to_string(),
            "test".to_string(),
            "BUY".to_string(),
            "Yes".to_string(),
            10.0,
            0.5,
        ).unwrap();
        
        let found = history.mark_redeemed("0x123").unwrap();
        assert!(found);
        
        let pending = history.get_pending();
        assert!(pending.is_empty());
        
        // Cleanup
        let _ = fs::remove_file("/tmp/test_trade_history.json");
    }
}