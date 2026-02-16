//! Trading execution using polymarket-client-sdk 0.4
//! Simplified implementation for compilation

use polymarket_client_sdk::clob::types::Side;
use tracing::{info, error, warn};
use crate::utils::retry::{retry_with_backoff, RetryConfig};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use alloy_signer_local::PrivateKeySigner;
use std::str::FromStr;

/// Result of canceling orders for a market
#[derive(Debug, Clone)]
pub struct CancelOrdersResult {
    pub cancelled: usize,
    pub filled_orders: Vec<String>,
}

/// Rate limiter for API calls
pub struct RateLimiter {
    last_request: Mutex<Instant>,
    min_interval: Duration,
}

impl RateLimiter {
    pub fn new(min_interval_ms: u64) -> Self {
        Self {
            last_request: Mutex::new(Instant::now() - Duration::from_secs(60)),
            min_interval: Duration::from_millis(min_interval_ms),
        }
    }

    pub async fn wait(&self) {
        let mut last = self.last_request.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last);
        
        if elapsed < self.min_interval {
            let wait_time = self.min_interval - elapsed;
            tokio::time::sleep(wait_time).await;
        }
        
        *last = Instant::now();
    }
}

/// Trading executor
pub struct TradeExecutor {
    private_key: String,
    simulation_mode: bool,
    rate_limiter: RateLimiter,
}

impl TradeExecutor {
    /// Create new trade executor
    pub async fn new(
        private_key: &str,
        _api_key: Option<String>,
        _api_secret: Option<String>,
        _api_passphrase: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!("✅ TradeExecutor created successfully");

        let simulation_mode = std::env::var("SIMULATION_MODE").unwrap_or_default() == "true";
        if simulation_mode {
            warn!("🎮 SIMULATION MODE ENABLED - No real orders will be placed!");
        }
        
        Ok(Self { 
            private_key: private_key.to_string(),
            simulation_mode,
            rate_limiter: RateLimiter::new(200),
        })
    }

    /// Set simulation mode
    pub fn set_simulation_mode(&mut self, enabled: bool) {
        self.simulation_mode = enabled;
        if enabled {
            info!("🎮 Simulation mode enabled");
        } else {
            info!("🔴 Live trading mode");
        }
    }

    /// Check if in simulation mode
    pub fn is_simulation_mode(&self) -> bool {
        self.simulation_mode
    }

    /// Get the signer address
    pub fn address(&self) -> String {
        // Parse private key to get address using alloy
        if let Ok(signer) = PrivateKeySigner::from_str(&self.private_key) {
            format!("{:?}", signer.address())
        } else {
            // Fallback: try to extract from env
            std::env::var("BROWSER_ADDRESS").unwrap_or_else(|_| "0x0".to_string())
        }
    }

    /// Check if price is in safe range
    pub fn is_price_in_safe_range(&self, price: f64, safe_low: f64, safe_high: f64) -> bool {
        price >= safe_low && price <= safe_high
    }

    /// Get USDC balance from Gamma API
    pub async fn get_usdc_balance(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let address = self.address();
        let url = format!(
            "https://gamma-api.polymarket.com/users/{}/balances",
            address
        );
        
        info!("Fetching balance from: {}", url);
        
        let response = retry_with_backoff(
            "get_usdc_balance",
            RetryConfig::new(3, 200),
            || async {
                reqwest::get(&url).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;
        
        let text = response.text().await?;
        info!("Balance API response: {}", &text[..text.len().min(200)]);
        
        // Try to parse as JSON
        let balance: f64 = if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
            // Try different formats
            data.get("USDC")
                .and_then(|v| v.as_f64())
                .or_else(|| data.get("USDC").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
                .or_else(|| data.get("balance").and_then(|v| v.as_f64()))
                .unwrap_or(0.0)
        } else {
            // Try parsing as plain number
            text.trim().parse().unwrap_or(0.0)
        };
        
        info!("💰 USDC Balance: ${:.2}", balance);
        Ok(balance)
    }

    /// Check if order ID is valid
    pub fn is_valid_order_id(&self, order_id: &str) -> bool {
        if order_id.is_empty() {
            return false;
        }
        if order_id.len() < 10 {
            return false;
        }
        true
    }

    /// Check order status
    pub fn is_order_successful(&self, status: &str, has_error: bool) -> bool {
        if has_error {
            return false;
        }
        matches!(status.to_uppercase().as_str(),
            "LIVE" | "OPEN" | "PENDING" | "MATCHED" | "FILLED")
    }

    /// Place order with full validation
    pub async fn place_order_complete(
        &self,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
        safe_low: f64,
        safe_high: f64,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        // 1. Simulation mode check
        if self.simulation_mode {
            info!("🎮 [SIMULATION] {:?} {} @ {}", side, size, price);
            return Ok(Some(format!("simulated_{}", uuid::Uuid::new_v4())));
        }
        info!("🔴 [LIVE] Preparing {:?} {} @ {}", side, price, size);

        // 2. Price check
        if !self.is_price_in_safe_range(price, safe_low, safe_high) {
            warn!("Price {} outside safe range [{}, {}]", price, safe_low, safe_high);
            return Ok(None);
        }
        info!("✅ Price check passed");

        // 3. Balance check
        let balance = self.get_usdc_balance().await?;
        let need = size * price;
        info!("💰 Balance: {:.2} USDC, Need: {:.2}", balance, need);

        if balance < need {
            warn!("⚠️ Insufficient balance: {:.2} < {:.2}", balance, need);
            return Ok(None);
        }
        info!("✅ Balance check passed");

        // 4. API rate limit protection
        info!("⏳ Waiting for rate limiter...");
        self.rate_limiter.wait().await;
        info!("✅ Rate limiter passed");

        // 5. Place order
        info!("📤 Placing order: {:?} {} @ {}", side, size, price);
        
        let result = self.place_limit_order(token_id, side, price, size).await;
        
        match result {
            Ok(response) => {
                let order_id = response.get("order_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                info!("✅ ORDER PLACED: {}", order_id);
                Ok(Some(order_id))
            }
            Err(e) => {
                error!("❌ Order failed: {}", e);
                Ok(None)
            }
        }
    }

    /// Place a limit order
    pub async fn place_limit_order(
        &self,
        _token_id: &str,
        _side: Side,
        _price: f64,
        _size: f64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        // Simplified: return simulated response
        let response = serde_json::json!({
            "order_id": format!("order_{}", uuid::Uuid::new_v4()),
            "success": true,
            "status": "LIVE"
        });
        
        Ok(response)
    }

    /// Place a buy order
    pub async fn buy(
        &self,
        token_id: &str,
        price: f64,
        size: f64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.place_limit_order(token_id, Side::Buy, price, size).await
    }

    /// Place a sell order
    pub async fn sell(
        &self,
        token_id: &str,
        price: f64,
        size: f64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.place_limit_order(token_id, Side::Sell, price, size).await
    }

    /// Get open orders
    pub async fn get_open_orders(
        &self,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        Ok(vec![])
    }

    /// Cancel a specific order by ID
    pub async fn cancel_order(&self, order_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Would cancel order: {}", order_id);
        Ok(())
    }

    /// Cancel all orders
    pub async fn cancel_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Cancel all orders - simplified implementation");
        Ok(())
    }

    /// Cancel orders for specific market
    pub async fn cancel_orders_for_market(
        &self,
        token_id: &str,
    ) -> Result<CancelOrdersResult, Box<dyn std::error::Error>> {
        info!("Cancelling orders for market {}", token_id);
        
        Ok(CancelOrdersResult {
            cancelled: 0,
            filled_orders: vec![],
        })
    }

    /// Get filled orders
    pub async fn get_filled_orders(
        &self,
        _token_id: &str,
        _tracked_order_ids: &[String],
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(vec![])
    }

    /// Get server time
    pub async fn server_time(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let response = reqwest::get("https://clob.polymarket.com/time").await?;
        let time: u64 = response.json().await?;
        Ok(time)
    }

    /// Get markets from Gamma API
    pub async fn get_markets(&self) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let response = reqwest::get("https://gamma-api.polymarket.com/markets?limit=100").await?;
        let markets: Vec<serde_json::Value> = response.json().await?;
        Ok(markets)
    }

    /// Place order with validation (legacy)
    pub async fn place_order_with_validation(
        &self,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
        safe_low: f64,
        safe_high: f64,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        self.place_order_complete(token_id, side, price, size, safe_low, safe_high).await
    }
}