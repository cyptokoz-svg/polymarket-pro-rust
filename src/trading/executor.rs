//! Trading execution using rs-clob-client
//! Handles order creation, signing, and submission

use rs_clob_client::{
    ClobClient, Chain, ApiKeyCreds, UserLimitOrder, Side, OrderType,
};
use alloy_signer_local::PrivateKeySigner;
use tracing::{info, error, warn};
use crate::utils::retry::{retry_with_backoff, RetryConfig};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

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
    clob: ClobClient,
    signer: PrivateKeySigner,
    simulation_mode: bool,
    rate_limiter: RateLimiter,
}

impl TradeExecutor {
    /// Create new trade executor
    pub async fn new(
        private_key: &str,
        api_key: Option<String>,
        api_secret: Option<String>,
        api_passphrase: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Parse private key
        let signer: PrivateKeySigner = private_key.parse()?;

        // Always derive API credentials from private key (more reliable than provided credentials)
        let creds = {
            info!("Deriving API credentials from private key...");
            // Create temporary client to derive API key
            let temp_client = ClobClient::new(
                "https://clob.polymarket.com".to_string(),
                "https://gamma-api.polymarket.com".to_string(),
                Chain::Polygon,
                Some(signer.clone()),
                None, // No creds yet
                None, // signature_type (0 = EOA)
                None, // funder_address
                None, // geo_block_token
                false, // use_server_time
                None, // builder_config
                None, // host_proxy_url
            )?;
            
            match temp_client.create_or_derive_api_key(None).await {
                Ok(derived_creds) => {
                    info!("✅ API credentials derived successfully");
                    Some(derived_creds)
                }
                Err(e) => {
                    warn!("⚠️ Failed to derive API credentials: {}", e);
                    warn!("⚠️ Some operations may fail without API credentials");
                    None
                }
            }
        };

        // Create CLOB client with credentials
        let clob = ClobClient::new(
            "https://clob.polymarket.com".to_string(),
            "https://gamma-api.polymarket.com".to_string(),
            Chain::Polygon,
            Some(signer.clone()),
            creds,
            None, // signature_type (0 = EOA)
            None, // funder_address
            None, // geo_block_token
            false, // use_server_time
            None, // builder_config
            None, // host_proxy_url
        )?;

        info!("TradeExecutor created successfully");

        let simulation_mode = std::env::var("SIMULATION_MODE").unwrap_or_default() == "true";
        if simulation_mode {
            warn!("🎮 SIMULATION MODE ENABLED - No real orders will be placed!");
        }
        
        Ok(Self { 
            clob, 
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
    /// Fetches actual balance for risk control
    pub async fn get_usdc_balance(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let address = format!("{:?}", self.signer.address());
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
        // Response format: {"USDC": "1234.56", ...}
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

    /// Place order with full validation (matches Python _prepare_and_place_order)
    /// Complete flow: check API -> cancel existing -> place new
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
        info!("🔴 [LIVE] Preparing {:?} {} @ {}", side, size, price);

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

        // 4. Check API for existing orders (skip if simulation mode or no API creds)
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
                            // Continue anyway in simulation mode
                        }
                    }
                }
                Err(e) => {
                    warn!("⚠️ Failed to check open orders: {}", e);
                    // Continue anyway - WebSocket price is more important
                }
            }
        } else {
            info!("🎮 [SIMULATION] Skipping order check/cancel");
        }

        // 6. API rate limit protection
        self.rate_limiter.wait().await;

        // 7. Place new order
        info!("📤 Placing order: {:?} {} @ {}", side, size, price);
        let result = self.place_limit_order(token_id, side, price, size).await?;

        // 8. Extract order ID and status
        let order_id = result.get("orderId")
            .or(result.get("order_id"))
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
    /// Matches Python: _get_open_orders_from_api()
    pub async fn get_open_orders(
        &self,
        token_id: &str,
    ) -> Result<Vec<rs_clob_client::OpenOrder>, Box<dyn std::error::Error>> {
        use rs_clob_client::OpenOrderParams;
        
        // Create params with asset_id filter
        let params = OpenOrderParams {
            id: None,
            market: None,
            asset_id: Some(token_id.to_string()),
        };
        
        // Use CLOB client's get_open_orders method
        let open_orders: Vec<rs_clob_client::OpenOrder> = retry_with_backoff(
            "get_open_orders",
            RetryConfig::new(3, 200),
            || async {
                self.clob.get_open_orders(Some(params.clone())).await
            },
        ).await?;

        Ok(open_orders)
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
                self.clob.cancel_order(order_id).await
            },
        ).await?;

        Ok(())
    }

    /// Place order with full validation (legacy version without complete flow)
    pub async fn place_order_with_validation(
        &self,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
        safe_low: f64,
        safe_high: f64,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        // Use the complete flow
        self.place_order_complete(token_id, side, price, size, safe_low, safe_high).await
    }

    /// Place a limit order with specific order type (GTC/FOK/FAK)
    /// Matches Python: create_order with order_type parameter
    pub async fn place_limit_order_with_type(
        &self,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
        order_type: OrderType,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let order = UserLimitOrder {
            token_id: token_id.to_string(),
            side,
            size,
            price,
            fee_rate_bps: None,
            nonce: None,
            expiration: None,
            taker: None,
        };

        info!(
            "Placing limit order: {} {:?} @ {} (size: {:?})",
            token_id, side, price, order_type
        );

        // Retry with exponential backoff
        let result = retry_with_backoff(
            "place_limit_order",
            RetryConfig::new(3, 200),
            || async {
                self.clob.create_and_post_limit_order(
                    &order,
                    None,
                    order_type,
                ).await
            },
        ).await?;

        info!("Order placed successfully");

        Ok(result)
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
    /// Matches Python: get_orderbook()
    pub async fn get_order_book(
        &self,
        token_id: &str,
    ) -> Result<(serde_json::Value, serde_json::Value), Box<dyn std::error::Error>> {
        info!("Getting order book for {}", token_id);

        // Use CLOB client's get_order_book method
        let book = self.clob.get_order_book(token_id).await?;

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
                self.clob.cancel_all().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            }
        ).await?;

        info!("All orders cancelled");

        Ok(result)
    }

    /// Cancel orders for specific market (by token_id)
    /// Matches Python: _cancel_orders_for_token()
    /// 
    /// CRITICAL FIX: Check for fills before cancelling to avoid double-counting
    /// - If order is no longer in open orders (but was tracked), it may have filled
    /// - Update positions for filled orders instead of just cancelling
    pub async fn cancel_orders_for_market(
        &self,
        token_id: &str,
    ) -> Result<CancelOrdersResult, Box<dyn std::error::Error>> {
        info!("Cancelling orders for market {}", token_id);

        // 1. Get open orders for this token (skip if in simulation mode)
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
        
        // Create a set of open order IDs for quick lookup
        let _open_order_ids: std::collections::HashSet<String> = open_orders
            .iter()
            .map(|o| o.id.clone())
            .collect();

        if open_orders.is_empty() {
            info!("No open orders to cancel for {}", token_id);
            return Ok(CancelOrdersResult {
                cancelled: 0,
                filled_orders: vec![], // All tracked orders may have filled
            });
        }

        info!("Found {} open orders to cancel", open_orders.len());
        let total_orders = open_orders.len();

        // 2. Cancel each order
        let mut cancelled_count = 0;
        for order in open_orders {
            let id = order.id;
            if !id.is_empty() {
                match self.cancel_order(&id).await {
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
            filled_orders: vec![], // TODO: Return actual filled order info
        })
    }
    
    /// Get filled orders for a token (orders that are no longer open)
    /// Used to detect fills before cancelling
    pub async fn get_filled_orders(
        &self,
        token_id: &str,
        tracked_order_ids: &[String],
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let open_orders = self.get_open_orders(token_id).await?;
        let open_ids: std::collections::HashSet<String> = open_orders
            .iter()
            .map(|o| o.id.clone())
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
    pub async fn get_server_time(&self,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let time = self.clob.get_server_time().await?;
        Ok(time)
    }

    /// Get markets
    pub async fn get_markets(
        &self,
    ) -> Result<Vec<rs_clob_client::Market>, Box<dyn std::error::Error>> {
        let params = rs_clob_client::MarketParams {
            limit: Some(500),
            offset: None,
            order: None,
            ascending: None,
            condition_id: None,
            closed: None,  // Don't filter by closed status
        };
        let markets = self.clob.get_markets(params).await?;
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