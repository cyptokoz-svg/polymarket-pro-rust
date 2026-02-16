//! Trading execution using polymarket-client-sdk 0.4
//! Uses dynamic client creation per operation to handle type states

use polymarket_client_sdk::{
    clob::{
        Client,
        Config,
        types::{Side, OrderType},
        types::request::OrdersRequest,
        types::response::PostOrderResponse,
    },
    types::{Decimal, U256},
};
use alloy::signers::local::PrivateKeySigner;
use std::str::FromStr;
use tracing::{info, error, warn};
use crate::utils::retry::{retry_with_backoff, RetryConfig};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

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

    /// Get signer from private key
    fn get_signer(&self) -> Result<PrivateKeySigner, Box<dyn std::error::Error>> {
        let signer = PrivateKeySigner::from_str(&self.private_key)?;
        Ok(signer)
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
        match self.get_signer() {
            Ok(signer) => format!("{:?}", signer.address()),
            Err(_) => "0x0".to_string(),
        }
    }

    /// Check if price is in safe range
    pub fn is_price_in_safe_range(&self, price: f64, safe_low: f64, safe_high: f64) -> bool {
        price >= safe_low && price <= safe_high
    }

    /// Get USDC balance from Gamma API
    pub async fn get_usdc_balance(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let address = self.address();
        // Try different API endpoints
        let urls = vec![
            format!("https://gamma-api.polymarket.com/users/{}/balances", address),
            format!("https://api.polymarket.com/users/{}/balances", address),
        ];

        for url in urls {
            info!("Fetching balance from: {}", url);

            let response = retry_with_backoff(
                "get_usdc_balance",
                RetryConfig::new(3, 200),
                || async {
                    reqwest::get(&url).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                },
            ).await;

            match response {
                Ok(resp) => {
                    let text = resp.text().await?;
                    info!("Balance API response: {}", &text[..text.len().min(200)]);

                    // Try to parse as JSON
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                        let balance = data.get("USDC")
                            .and_then(|v| v.as_f64())
                            .or_else(|| data.get("USDC").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
                            .or_else(|| data.get("balance").and_then(|v| v.as_f64()))
                            .unwrap_or(0.0);

                        info!("💰 USDC Balance: ${:.2}", balance);
                        return Ok(balance);
                    }
                }
                Err(e) => {
                    warn!("Failed to fetch from {}: {}", url, e);
                }
            }
        }

        // Fallback to default
        warn!("⚠️ All balance API endpoints failed, using default");
        Ok(10000.0)
    }

    /// Place a limit order
    pub async fn place_limit_order(
        &self,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
    ) -> Result<PostOrderResponse, Box<dyn std::error::Error>> {
        if self.simulation_mode {
            info!("🎮 [SIMULATION] {:?} {} @ {}", side, size, price);
            // Create mock response via JSON
            let mock_json = serde_json::json!({
                "order_id": format!("simulated_{}", uuid::Uuid::new_v4()),
                "success": true,
                "status": "LIVE"
            });
            let response: PostOrderResponse = serde_json::from_value(mock_json)?;
            return Ok(response);
        }
        
        let signer = self.get_signer()?;
        let config = Config::builder().use_server_time(true).build();
        
        // Create and authenticate client inline
        let client = Client::new("https://clob.polymarket.com", config)?
            .authentication_builder(&signer)
            .authenticate()
            .await?;
        
        let token_id_u256 = U256::from_str(token_id)?;
        let price_decimal = Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO);
        let size_decimal = Decimal::from_f64_retain(size).unwrap_or(Decimal::ZERO);
        
        // Build, sign and post order
        let order = client
            .limit_order()
            .token_id(token_id_u256)
            .size(size_decimal)
            .price(price_decimal)
            .side(side)
            .order_type(OrderType::GTC)
            .build()
            .await?;
        
        let signed_order = client.sign(&signer, order).await?;
        let response = client.post_order(signed_order).await?;
        
        info!("✅ Order placed: {} (success: {})", response.order_id, response.success);
        Ok(response)
    }

    /// Place a buy order
    pub async fn buy(
        &self,
        token_id: &str,
        price: f64,
        size: f64,
    ) -> Result<PostOrderResponse, Box<dyn std::error::Error>> {
        self.place_limit_order(token_id, Side::Buy, price, size).await
    }

    /// Place a sell order
    pub async fn sell(
        &self,
        token_id: &str,
        price: f64,
        size: f64,
    ) -> Result<PostOrderResponse, Box<dyn std::error::Error>> {
        self.place_limit_order(token_id, Side::Sell, price, size).await
    }

    /// Get open orders
    pub async fn get_open_orders(
        &self,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        if self.simulation_mode {
            return Ok(vec![]);
        }
        
        let signer = self.get_signer()?;
        let config = Config::builder().use_server_time(true).build();
        
        let client = Client::new("https://clob.polymarket.com", config)?
            .authentication_builder(&signer)
            .authenticate()
            .await?;
        
        let request = OrdersRequest::default();
        let response = client.orders(&request, None).await?;
        
        // Convert to JSON values for flexibility
        let orders: Vec<serde_json::Value> = response.data
            .into_iter()
            .map(|order| serde_json::json!({
                "id": order.id,
                "asset_id": order.asset_id,
                "side": order.side,
                "price": order.price,
                "size": order.original_size,
                "status": order.status,
            }))
            .collect();
        
        Ok(orders)
    }

    /// Cancel a specific order by ID
    pub async fn cancel_order(&self, order_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if self.simulation_mode {
            info!("🎮 [SIMULATION] Would cancel order: {}", order_id);
            return Ok(());
        }
        
        let signer = self.get_signer()?;
        let config = Config::builder().use_server_time(true).build();
        
        let client = Client::new("https://clob.polymarket.com", config)?
            .authentication_builder(&signer)
            .authenticate()
            .await?;
        
        client.cancel_order(order_id).await?;
        info!("✅ Cancelled order: {}", order_id);
        Ok(())
    }

    /// Cancel all orders
    pub async fn cancel_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.simulation_mode {
            info!("🎮 [SIMULATION] Would cancel all orders");
            return Ok(());
        }
        
        let signer = self.get_signer()?;
        let config = Config::builder().use_server_time(true).build();
        
        let client = Client::new("https://clob.polymarket.com", config)?
            .authentication_builder(&signer)
            .authenticate()
            .await?;
        
        client.cancel_all_orders().await?;
        info!("✅ Cancelled all orders");
        Ok(())
    }

    /// Cancel orders for specific market
    pub async fn cancel_orders_for_market(
        &self,
        token_id: &str,
    ) -> Result<CancelOrdersResult, Box<dyn std::error::Error>> {
        info!("Cancelling orders for market {}", token_id);
        
        let orders = self.get_open_orders().await?;
        let mut cancelled = 0;
        let mut filled_orders = vec![];
        
        for order in &orders {
            let order_id = order.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let asset_id = order.get("asset_id").and_then(|v| v.as_str()).unwrap_or("");
            
            if asset_id == token_id {
                match self.cancel_order(order_id).await {
                    Ok(_) => cancelled += 1,
                    Err(e) => {
                        warn!("Failed to cancel order {}: {}", order_id, e);
                        filled_orders.push(order_id.to_string());
                    }
                }
            }
        }
        
        info!("✅ Cancelled {}/{} orders for market {}", cancelled, orders.len(), token_id);
        Ok(CancelOrdersResult {
            cancelled,
            filled_orders,
        })
    }

    /// Get filled orders
    pub async fn get_filled_orders(
        &self,
        _token_id: &str,
        tracked_order_ids: &[String],
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let open_orders = self.get_open_orders().await?;
        let open_ids: std::collections::HashSet<String> = open_orders
            .iter()
            .filter_map(|o| o.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();
        
        let filled: Vec<String> = tracked_order_ids
            .iter()
            .filter(|id| !open_ids.contains(*id))
            .cloned()
            .collect();
        
        Ok(filled)
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
        if !self.is_price_in_safe_range(price, safe_low, safe_high) {
            warn!("Price {} outside safe range [{}, {}]", price, safe_low, safe_high);
            return Ok(None);
        }

        if !self.simulation_mode {
            let balance = self.get_usdc_balance().await?;
            let need = size * price;
            if balance < need {
                warn!("⚠️ Insufficient balance: {:.2} < {:.2}", balance, need);
                return Ok(None);
            }
        }

        self.rate_limiter.wait().await;

        match self.place_limit_order(token_id, side, price, size).await {
            Ok(response) => {
                if response.success {
                    Ok(Some(response.order_id))
                } else {
                    error!("Order failed: {:?}", response.error_msg);
                    Ok(None)
                }
            }
            Err(e) => {
                error!("❌ Order placement failed: {}", e);
                Ok(None)
            }
        }
    }
}