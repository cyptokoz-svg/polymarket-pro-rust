//! Configuration management
//! Supports TOML, YAML, JSON config files

pub mod manager;

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

pub use manager::{ConfigManager, ConfigUpdates};

/// Bot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Wallet private key
    pub pk: String,
    /// Safe wallet address
    pub safe_address: String,
    /// Builder API credentials
    pub api: ApiConfig,
    /// Trading parameters
    pub trading: TradingConfig,
    /// WebSocket settings
    pub websocket: WebSocketConfig,
    /// Logging level
    pub log_level: Option<String>,
}

/// API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
}

/// Trading configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    /// Order size in shares (default: 1.0)
    pub order_size: f64,
    /// Maximum position size per side (default: 5.0)
    pub max_position: f64,
    /// Maximum total position across all markets (default: 30.0)
    pub max_total_position: f64,
    /// Maximum spread (default: 0.02)
    pub max_spread: f64,
    /// Minimum spread (default: 0.005)
    pub min_spread: f64,
    /// Merge threshold for inventory merge (default: 0.5)
    pub merge_threshold: f64,
    /// Maximum hold time in seconds (default: 180)
    pub max_hold_time: u64,
    /// Exit before expiry in seconds (default: 120)
    pub exit_before_expiry: u64,
    /// Take profit percentage (default: 0.03)
    pub take_profit: f64,
    /// Stop loss percentage (default: 0.05)
    pub stop_loss: f64,
    /// Order book depth lookback (default: 5)
    pub depth_lookback: u64,
    /// Imbalance threshold (default: 0.3)
    pub imbalance_threshold: f64,
    /// Minimum price (default: 0.01)
    pub min_price: f64,
    /// Maximum price (default: 0.99)
    pub max_price: f64,
    /// Safe price range lower bound (default: 0.01)
    pub safe_range_low: f64,
    /// Safe price range upper bound (default: 0.99)
    pub safe_range_high: f64,
    /// Price warning cooldown in seconds (default: 60)
    pub price_warn_cooldown: u64,
    /// Order refresh interval in seconds (default: 45)
    pub refresh_interval: u64,
    /// Spread percentage (default: 0.02)
    pub spread: f64,
    /// Trading strategy mode: "market_maker" (4 orders) or "buy_hold" (2 orders)
    pub strategy_mode: String,
}

/// WebSocket configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Enable WebSocket price feed
    pub enabled: bool,
    /// Auto reconnect on disconnect
    pub auto_reconnect: bool,
    /// Max reconnect attempts
    pub max_reconnect: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pk: String::new(),
            safe_address: String::new(),
            api: ApiConfig {
                key: None,
                secret: None,
                passphrase: None,
            },
            trading: TradingConfig {
                order_size: 1.0,           // Changed from 5.0 to match Python
                max_position: 5.0,         // Changed from 10.0 to match Python
                max_total_position: 30.0,
                max_spread: 0.02,
                min_spread: 0.005,
                merge_threshold: 0.5,      // New: merge threshold
                max_hold_time: 180,        // New: max hold time in seconds
                exit_before_expiry: 120,   // New: exit before expiry in seconds
                take_profit: 0.03,         // New: take profit percentage
                stop_loss: 0.05,           // New: stop loss percentage
                depth_lookback: 5,         // New: order book depth lookback
                imbalance_threshold: 0.3,  // New: imbalance threshold
                min_price: 0.01,           // New: minimum price
                max_price: 0.99,           // New: maximum price
                safe_range_low: 0.01,
                safe_range_high: 0.99,
                price_warn_cooldown: 60,   // New: price warning cooldown in seconds
                refresh_interval: 45,
                spread: 0.02,
                strategy_mode: "market_maker".to_string(), // "market_maker" or "buy_hold"
            },
            websocket: WebSocketConfig {
                enabled: true,
                auto_reconnect: true,
                max_reconnect: 5,
            },
            log_level: Some("info".to_string()),
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        info!("Loading configuration from: {}", path.display());
        
        let content = std::fs::read_to_string(path)?;
        
        let config = if path.extension().map(|e| e == "toml").unwrap_or(false) {
            toml::from_str(&content)?
        } else if path.extension().map(|e| e == "yaml" || e == "yml").unwrap_or(false) {
            serde_yaml::from_str(&content)?
        } else if path.extension().map(|e| e == "json").unwrap_or(false) {
            serde_json::from_str(&content)?
        } else {
            // Try to auto-detect format
            if content.trim().starts_with('{') {
                serde_json::from_str(&content)?
            } else if content.contains("---") {
                serde_yaml::from_str(&content)?
            } else {
                toml::from_str(&content)?
            }
        };
        
        info!("Configuration loaded successfully");
        Ok(config)
    }
    
    /// Load from default locations
    pub fn load() -> anyhow::Result<Self> {
        // Try default locations
        let locations = vec![
            "polymarket-pro.toml",
            "polymarket-pro.yaml",
            "polymarket-pro.yml",
            "config.toml",
            "config.yaml",
            ".polymarket-pro.toml",
            ".polymarket-pro.yaml",
        ];
        
        for location in &locations {
            if std::path::Path::new(location).exists() {
                return Self::from_file(location);
            }
        }
        
        // Try config directory
        if let Some(config_dir) = dirs::config_dir() {
            let config_file = config_dir.join("polymarket-pro/config.toml");
            if config_file.exists() {
                return Self::from_file(config_file);
            }
        }
        
        anyhow::bail!("No configuration file found. Expected one of: {:?}", locations)
    }
    
    /// Save configuration to file (SECURITY: removes sensitive data before saving)
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let path = path.as_ref();
        
        // SECURITY: Create a safe copy with sensitive data redacted
        let mut safe_config = self.clone();
        safe_config.pk = "******REMOVED******".to_string();
        safe_config.api.key = None;
        safe_config.api.secret = None;
        safe_config.api.passphrase = None;
        
        let content = if path.extension().map(|e| e == "toml").unwrap_or(false) {
            toml::to_string_pretty(&safe_config)?
        } else if path.extension().map(|e| e == "yaml" || e == "yml").unwrap_or(false) {
            serde_yaml::to_string(&safe_config)?
        } else {
            serde_json::to_string_pretty(&safe_config)?
        };
        
        std::fs::write(path, content)?;
        info!("Configuration saved to: {} (sensitive data redacted)", path.display());
        Ok(())
    }
    
    /// Validate configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.pk.is_empty() {
            anyhow::bail!("Private key (pk) is required");
        }
        if self.safe_address.is_empty() {
            anyhow::bail!("Safe address is required");
        }
        // Check BROWSER_ADDRESS like Python
        let browser_addr = std::env::var("BROWSER_ADDRESS").unwrap_or_default();
        if browser_addr.is_empty() {
            anyhow::bail!("BROWSER_ADDRESS environment variable is required");
        }
        
        // Enhanced private key validation
        if !self.pk.starts_with("0x") {
            anyhow::bail!("Private key must start with 0x");
        }
        if self.pk.len() != 66 {
            anyhow::bail!("Private key must be 64 hex characters with 0x prefix (66 chars total)");
        }
        // Validate hex characters
        if hex::decode(&self.pk[2..]).is_err() {
            anyhow::bail!("Private key contains invalid hex characters");
        }
        
        // Enhanced Safe address validation
        if !self.safe_address.starts_with("0x") {
            anyhow::bail!("Safe address must start with 0x");
        }
        if self.safe_address.len() != 42 {
            anyhow::bail!("Safe address must be a valid Ethereum address (42 chars)");
        }
        // Validate address hex characters
        if hex::decode(&self.safe_address[2..]).is_err() {
            anyhow::bail!("Safe address contains invalid hex characters");
        }
        
        if self.trading.order_size <= 0.0 {
            anyhow::bail!("Order size must be positive");
        }
        if self.trading.safe_range_low >= self.trading.safe_range_high {
            anyhow::bail!("Safe range low must be less than high");
        }
        Ok(())
    }

    /// Check Builder API configuration
    pub fn check_builder_api(&self) -> BuilderApiStatus {
        let key = self.api.key.as_deref().unwrap_or("");
        let secret = self.api.secret.as_deref().unwrap_or("");
        let passphrase = self.api.passphrase.as_deref().unwrap_or("");

        let has_key = !key.is_empty();
        let has_secret = !secret.is_empty();
        let has_passphrase = !passphrase.is_empty();

        match (has_key, has_secret, has_passphrase) {
            (true, true, true) => BuilderApiStatus::Enabled,
            (false, false, false) => BuilderApiStatus::Disabled,
            _ => BuilderApiStatus::PartiallyConfigured(
                vec![
                    if has_key { None } else { Some("POLY_BUILDER_API_KEY") },
                    if has_secret { None } else { Some("POLY_BUILDER_API_SECRET") },
                    if has_passphrase { None } else { Some("POLY_BUILDER_API_PASSPHRASE") },
                ].into_iter().flatten().collect()
            ),
        }
    }
}

/// Builder API configuration status
#[derive(Debug, Clone)]
pub enum BuilderApiStatus {
    Enabled,
    Disabled,
    PartiallyConfigured(Vec<&'static str>),
}

/// Load config from environment variables (fallback)
pub fn from_env() -> anyhow::Result<Config> {
    use std::env;
    
    let config = Config {
        pk: env::var("PK").unwrap_or_default(),
        safe_address: env::var("SAFE_ADDRESS").unwrap_or_default(),
        api: ApiConfig {
            key: env::var("POLY_BUILDER_API_KEY").ok(),
            secret: env::var("POLY_BUILDER_API_SECRET").ok(),
            passphrase: env::var("POLY_BUILDER_API_PASSPHRASE").ok(),
        },
        trading: TradingConfig {
            order_size: env::var("ORDER_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),  // Changed from 5.0
            max_position: env::var("MAX_POSITION")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5.0),  // Changed from 10.0
            max_total_position: env::var("MAX_TOTAL_POSITION")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30.0),
            max_spread: env::var("MAX_SPREAD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.02),
            min_spread: env::var("MIN_SPREAD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.005),
            merge_threshold: env::var("MERGE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.5),
            max_hold_time: env::var("MAX_HOLD_TIME")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(180),
            exit_before_expiry: env::var("EXIT_BEFORE_EXPIRY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            take_profit: env::var("TAKE_PROFIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.03),
            stop_loss: env::var("STOP_LOSS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.05),
            depth_lookback: env::var("DEPTH_LOOKBACK")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            imbalance_threshold: env::var("IMBALANCE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.3),
            min_price: env::var("MIN_PRICE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.01),
            max_price: env::var("MAX_PRICE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.99),
            safe_range_low: env::var("SAFE_RANGE_LOW")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.01),
            safe_range_high: env::var("SAFE_RANGE_HIGH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.99),
            price_warn_cooldown: env::var("PRICE_WARN_COOLDOWN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            refresh_interval: env::var("REFRESH_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(45),
            spread: env::var("SPREAD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.02),
            strategy_mode: env::var("STRATEGY_MODE")
                .ok()
                .unwrap_or_else(|| "buy_hold".to_string()),
        },
        websocket: WebSocketConfig {
            enabled: env::var("WS_ENABLED")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            auto_reconnect: env::var("WS_AUTO_RECONNECT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            max_reconnect: env::var("WS_MAX_RECONNECT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
        },
        log_level: env::var("LOG_LEVEL").ok(),
    };
    
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.trading.order_size, 1.0);  // Changed to match Python
        assert_eq!(config.trading.max_position, 5.0);  // Changed to match Python
        assert_eq!(config.trading.refresh_interval, 45);
    }

    #[test]
    fn test_validate_empty_pk() {
        let config = Config::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_range() {
        let mut config = Config::default();
        config.pk = "0x123".to_string();
        config.safe_address = "0x456".to_string();
        config.trading.safe_range_low = 0.9;
        config.trading.safe_range_high = 0.1;
        assert!(config.validate().is_err());
    }
}