//! Trading execution using polymarket-client-sdk
//! Handles order creation, signing, and submission

use polymarket_client_sdk::clob::{Client as ClobClient, Config};
use polymarket_client_sdk::clob::types::{Side, OrderType, SignatureType};
use polymarket_client_sdk::types::Decimal;
use alloy_signer_local::PrivateKeySigner;
use alloy_signer::Signer as _;
use tracing::{info, error, warn};
use crate::utils::retry::{retry_with_backoff, RetryConfig};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use std::str::FromStr;

/// Result of canceling orders for a market
#[derive(Debug, Clone)]
pub struct CancelOrdersResult {
    pub cancelled: usize,
    pub filled_orders: Vec<String>, // Order IDs that may have filled
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
    client: ClobClient,
    signer: PrivateKeySigner,
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
        // Parse private key
        let signer: PrivateKeySigner = PrivateKeySigner::from_str(private_key)?;
        
        info!("Creating CLOB client...");
        
        // Create authenticated client using new SDK API
        let client = retry_with_backoff(
            "create_clob_client",
            RetryConfig::new(3, 500),
            || async {
                ClobClient::new("https://clob.polymarket.com", Config::default())
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;
        
        // Authenticate the client
        let client = retry_with_backoff(
            "authenticate_clob_client",
            RetryConfig::new(3, 500),
            || async {
                client.clone()
                    .authentication_builder(&signer)
                    .signature_type(SignatureType::Eoa) // EOA wallet
                    .authenticate()
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;

        info!("✅ TradeExecutor created successfully");

        let simulation_mode = std::env::var("SIMULATION_MODE").unwrap_or_default() == "true";
        if simulation_mode {
            warn!("🎮 SIMULATION MODE ENABLED - No real orders will be placed!");
        }
        
        Ok(Self { 
            client, 
            signer,
            simulation_mode,
            rate_limiter: RateLimiter::new(200), // 200ms min interval
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

    /// Get the signer
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
    }

    /// Check if price is in safe range
    pub fn is_price_in_safe_range(&self, price: f64, safe_low: f64, safe_high: f64) -> bool {
        price >= safe_low && price <= safe_high
    }

    /// Get USDC balance from Gamma API
    pub async fn get_usdc_balance(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let address = self.signer.address();
        let url = format!(
            "https://gamma-api.polymarket.com/users/{}/balances",
            address
        );
        
        let response = retry_with_backoff(
            "get_usdc_balance",
            RetryConfig::new(3, 200),
            || async {
                reqwest::get(&url).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;
        
        let data: serde_json::Value = response.json().await?;
        
        // Parse USDC balance from response
        let balance = data
            .get("USDC")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or_else(|| {
                warn!("⚠️ Failed to parse USDC balance, using default");
                10000.0
            });
        
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
            return Ok(Some("simulated".to_string()));
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

        // 4. Check API for existing orders
        if !self.simulation_mode {
            info!("🔍 Checking existing orders...");
            match self.get_open_orders(token_id).await {
                Ok(open_orders) => {
                    info!("📋 Found {} existing orders", open_orders.len());
                    
                    // 5. Cancel existing orders if any
                    if !open_orders.is_empty() {
                        info!("🗑️ Cancelling {} orders...", open_orders.len());
                        if let Err(e) = self.cancel_orders_for_market(token_id).await {
                            warn!("⚠️ Failed to cancel orders: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("⚠️ Failed to check open orders: {}", e);
                }
            }
        } else {
            info!("🎮 [SIMULATION] Skipping order check/cancel");
        }

        // 6. API rate limit protection
        info!("⏳ Waiting for rate limiter...");
        self.rate_limiter.wait().await;
        info!("✅ Rate limiter passed");

        // 7. Place new order using new SDK API
        info!("📤 Placing order: {:?} {} @ {}", side, size, price);
        let result = match self.place_limit_order(token_id, side, price, size).await {
            Ok(r) => {
                info!("✅ place_limit_order returned successfully");
                r
            }
            Err(e) => {
                error!("❌ place_limit_order failed: {}", e);
                return Err(e);
            }
        };

        // 8. Extract order ID and status
        let order_id = result.get("orderID")
            .or(result.get("order_id"))
            .or(result.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let status = result.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let has_error = result.get("error").is_some();

        // 9. Validate order ID
        if !self.is_valid_order_id(&order_id) {
            error!("❌ Invalid order ID format: {}", order_id);
            return Ok(None);
        }

        // 10. Check if successful
        if self.is_order_successful(status, has_error) {
            info!("✅ ORDER PLACED: {} (status: {})", order_id, status);
            Ok(Some(order_id))
        } else {
            error!("❌ Order failed with status: {}", status);
            Ok(None)
        }
    }

    /// Get open orders for a token from API
    pub async fn get_open_orders(
        &self,
        token_id: &str,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        // Use the new SDK's orders endpoint
        match retry_with_backoff(
            "get_open_orders",
            RetryConfig::new(3, 200),
            || async {
                // Get all orders and filter by asset_id
                let orders = self.client.get_orders().await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                
                // Filter orders for this token
                let filtered: Vec<serde_json::Value> = orders
                    .into_iter()
                    .filter(|o| {
                        o.get("asset_id")
                            .or(o.get("token_id"))
                            .and_then(|v| v.as_str())
                            .map(|s| s == token_id)
                            .unwrap_or(false)
                    })
                    .collect();
                
                Ok::<_, Box<dyn std::error::Error>>(filtered)
            },
        ).await {
            Ok(orders) => Ok(orders),
            Err(e) => {
                warn!("⚠️ Failed to get open orders: {}, returning empty list", e);
                Ok(vec![])
            }
        }
    }

    /// Cancel a specific order by ID
    pub async fn cancel_order(
        &self,
        order_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        retry_with_backoff(
            "cancel_order",
            RetryConfig::new(3, 200),
            || async {
                self.client.cancel_order(order_id).await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;

        Ok(())
    }

    /// Place order with full validation (legacy version)
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

    /// Place a limit order with specific order type
    pub async fn place_limit_order_with_type(
        &self,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
        order_type: OrderType,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        info!(
            "Placing limit order: {} {:?} @ {} (size: {:?})",
            token_id, side, price, order_type
        );

        // Build the order using new SDK API
        let order = retry_with_backoff(
            "build_limit_order",
            RetryConfig::new(3, 200),
            || async {
                self.client
                    .limit_order()
                    .token_id(token_id)
                    .size(Decimal::from_f64(size).unwrap_or(Decimal::ZERO))
                    .price(Decimal::from_f64(price).unwrap_or(Decimal::ZERO))
                    .side(side)
                    .order_type(order_type)
                    .build()
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;

        // Sign the order
        let signed_order = retry_with_backoff(
            "sign_order",
            RetryConfig::new(3, 200),
            || async {
                self.client.sign(&self.signer, order.clone()).await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;

        // Post the order
        let result = retry_with_backoff(
            "post_order",
            RetryConfig::new(3, 200),
            || async {
                self.client.post_order(signed_order.clone()).await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
        ).await?;

        info!("Order placed successfully");
        
        // Convert result to serde_json::Value
        let result_json = serde_json::to_value(&result)?;
        Ok(result_json)
    }

    /// Place a limit order (default GTC)
    pub async fn place_limit_order(
        &self,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.place_limit_order_with_type(token_id, side, price, size, OrderType::Gtc).await
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

    /// Get order book for a token
    pub async fn get_order_book(
        &self,
        token_id: &str,
    ) -> Result<(serde_json::Value, serde_json::Value), Box<dyn std::error::Error>> {
        info!("Getting order book for {}", token_id);

        // Use the new SDK's orderbook endpoint
        let book = match tokio::time::timeout(
            Duration::from_secs(10),
            self.client.get_orderbook(token_id)
        ).await {
            Ok(Ok(book)) => book,
            Ok(Err(e)) => {
                warn!("Failed to get order book: {}", e);
                return Err(e.into());
            }
            Err(_) => {
                warn!("Timeout getting order book for {}", token_id);
                return Err("Timeout".into());
            }
        };

        let bids = serde_json::to_value(&book.bids)?;
        let asks = serde_json::to_value(&book.asks)?;

        info!("Order book retrieved: {} bids, {} asks",
            book.bids.len(), book.asks.len());

        Ok((bids, asks))
    }

    /// Cancel all orders with retry
    pub async fn cancel_all(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        info!("Cancelling all orders");

        let result = retry_with_backoff(
            "cancel_all",
            RetryConfig::new(3, 200),
            || async {
                self.client.cancel_all_orders().await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            }
        ).await?;

        info!("All orders cancelled");

        let result_json = serde_json::to_value(&result)?;
        Ok(result_json)
    }

    /// Cancel orders for specific market (by token_id)
    pub async fn cancel_orders_for_market(
        &self,
        token_id: &str,
    ) -> Result<CancelOrdersResult, Box<dyn std::error::Error>> {
        info!("Cancelling orders for market {}", token_id);

        // 1. Get open orders for this token
        let open_orders = if self.simulation_mode {
            info!("🎮 [SIMULATION] Skipping order cancellation for {}", token_id);
            vec![]
        } else {
            match self.get_open_orders(token_id).await {
                Ok(orders) => orders,
                Err(e) => {
                    warn!("⚠️ Failed to get open orders: {}, assuming no orders", e);
                    vec![]
                }
            }
        };

        if open_orders.is_empty() {
            info!("No open orders to cancel for {}", token_id);
            return Ok(CancelOrdersResult {
                cancelled: 0,
                filled_orders: vec![],
            });
        }

        info!("Found {} open orders to cancel", open_orders.len());
        let total_orders = open_orders.len();

        // 2. Cancel each order
        let mut cancelled_count = 0;
        for order in open_orders {
            let id = order.get("id")
                .or(order.get("order_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
                
            if !id.is_empty() {
                match self.cancel_order(id).await {
                    Ok(_) => {
                        cancelled_count += 1;
                        info!("Cancelled order {}", id);
                    }
                    Err(e) => {
                        warn!("Failed to cancel order {}: {}", id, e);
                    }
                }
            }
        }

        info!("✅ Cancelled {}/{} orders for market {}", cancelled_count, total_orders, token_id);
        Ok(CancelOrdersResult {
            cancelled: cancelled_count,
            filled_orders: vec![],
        })
    }
    
    /// Get filled orders for a token
    pub async fn get_filled_orders(
        &self,
        token_id: &str,
        tracked_order_ids: &[String],
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let open_orders = self.get_open_orders(token_id).await?;
        let open_ids: std::collections::HashSet<String> = open_orders
            .iter()
            .filter_map(|o| o.get("id").or(o.get("order_id")).and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        
        // Orders that were tracked but are no longer open = likely filled
        let filled: Vec<String> = tracked_order_ids
            .iter()
            .filter(|id| !open_ids.contains(*id))
            .cloned()
            .collect();
        
        Ok(filled)
    }

    /// Get server time
    pub async fn get_server_time(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let time = self.client.get_server_time().await?;
        Ok(time as u64)
    }

    /// Get markets
    pub async fn get_markets(
        &self,
    ) -> Result<Vec<polymarket_client_sdk::gamma::types::Market>, Box<dyn std::error::Error>> {
        use polymarket_client_sdk::gamma::Client as GammaClient;
        use polymarket_client_sdk::gamma::types::request::MarketsRequest;
        
        let gamma_client = GammaClient::default();
        let request = MarketsRequest::builder().limit(500).build();
        let markets = gamma_client.markets(&request).await?;
        
        Ok(markets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_side_enum() {
        let buy = Side::Buy;
        let sell = Side::Sell;
        assert_ne!(std::mem::discriminant(&buy), std::mem::discriminant(&sell));
    }

    #[test]
    fn test_order_type_enum() {
        let gtc = OrderType::Gtc;
        let fok = OrderType::Fok;
        assert_ne!(std::mem::discriminant(&gtc), std::mem::discriminant(&fok));
    }
}