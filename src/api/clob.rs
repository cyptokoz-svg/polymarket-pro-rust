//! CLOB (Central Limit Order Book) API client
//! Handles order placement, cancellation, and position tracking

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::api::ApiError;

// Re-export Side from polymarket_client_sdk for consistency
pub use polymarket_client_sdk::clob::types::Side;

const CLOB_API_URL: &str = "https://clob.polymarket.com";

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Open,
    Filled,
    PartiallyFilled,
    Cancelled,
    Expired,
}

/// Order request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub market_id: String,
    pub side: Side,
    pub size: f64,
    pub price: f64,
}

/// Order response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub order_id: String,
    pub status: OrderStatus,
}

/// CLOB client trait for trading operations
#[async_trait]
pub trait ClobClient: Send + Sync {
    /// Place an order
    async fn place_order(&self,
        order: Order,
    ) -> Result<OrderResponse, ApiError>;
    
    /// Cancel an order
    async fn cancel_order(
        &self,
        order_id: &str,
    ) -> Result<(), ApiError>;
    
    /// Cancel all orders for a market
    async fn cancel_all_orders(
        &self,
        market_id: &str,
    ) -> Result<(), ApiError>;
    
    /// Get open orders
    async fn get_open_orders(
        &self,
    ) -> Result<Vec<Order>, ApiError>;
    
    /// Get positions
    async fn get_positions(
        &self,
    ) -> Result<Vec<Position>, ApiError>;
}

/// CLOB API client implementation
pub struct ClobApiClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl ClobApiClient {
    /// Create new CLOB API client with timeout
    pub fn new(api_key: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            client,
            base_url: CLOB_API_URL.to_string(),
            api_key,
        }
    }
    
    /// Get headers with API key if available
    fn get_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        if let Some(key) = &self.api_key {
            headers.insert(
                "POLYMARKET_API_KEY",
                key.parse().unwrap(),
            );
        }
        headers
    }
}

#[async_trait]
impl ClobClient for ClobApiClient {
    async fn place_order(
        &self,
        order: Order,
    ) -> Result<OrderResponse, ApiError> {
        let url = format!("{}/order", self.base_url);
        
        let response = self.client
            .post(&url)
            .headers(self.get_headers())
            .json(&order)
            .send()
            .await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::api::sanitize_api_error(status.as_u16(), error_text));
        }
        
        let order_response: OrderResponse = response.json().await?;
        Ok(order_response)
    }
    
    async fn cancel_order(
        &self,
        order_id: &str,
    ) -> Result<(), ApiError> {
        let url = format!("{}/order/{}", self.base_url, order_id);
        
        let response = self.client
            .delete(&url)
            .headers(self.get_headers())
            .send()
            .await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::api::sanitize_api_error(status.as_u16(), error_text));
        }
        
        Ok(())
    }
    
    async fn cancel_all_orders(
        &self,
        market_id: &str,
    ) -> Result<(), ApiError> {
        let url = format!("{}/orders/market/{}", self.base_url, market_id);
        
        let response = self.client
            .delete(&url)
            .headers(self.get_headers())
            .send()
            .await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::api::sanitize_api_error(status.as_u16(), error_text));
        }
        
        Ok(())
    }
    
    async fn get_open_orders(
        &self,
    ) -> Result<Vec<Order>, ApiError> {
        let url = format!("{}/orders", self.base_url);
        
        let response = self.client
            .get(&url)
            .headers(self.get_headers())
            .send()
            .await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::api::sanitize_api_error(status.as_u16(), error_text));
        }
        
        let orders: Vec<Order> = response.json().await?;
        Ok(orders)
    }
    
    async fn get_positions(
        &self,
    ) -> Result<Vec<Position>, ApiError> {
        let url = format!("{}/positions", self.base_url);
        
        let response = self.client
            .get(&url)
            .headers(self.get_headers())
            .send()
            .await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::api::sanitize_api_error(status.as_u16(), error_text));
        }
        
        let positions: Vec<Position> = response.json().await?;
        Ok(positions)
    }
}

/// Position data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub market_id: String,
    pub side: Side,
    pub size: f64,
    pub avg_price: f64,
}

/// Mock CLOB client for testing
#[cfg(test)]
pub struct MockClobClient {
    place_order_fn: Option<Box<dyn Fn(Order) -> Result<OrderResponse, ApiError> + Send + Sync>>,
}

#[cfg(test)]
impl MockClobClient {
    pub fn new() -> Self {
        Self { place_order_fn: None }
    }
    
    pub fn expect_place_order<F>(&mut self,
        f: F,
    ) where
        F: Fn(Order) -> Result<OrderResponse, ApiError> + Send + Sync + 'static,
    {
        self.place_order_fn = Some(Box::new(f));
    }
}

#[cfg(test)]
#[async_trait]
impl ClobClient for MockClobClient {
    async fn place_order(
        &self,
        order: Order,
    ) -> Result<OrderResponse, ApiError> {
        match &self.place_order_fn {
            Some(f) => f(order),
            None => Ok(OrderResponse {
                order_id: "mock_order".to_string(),
                status: OrderStatus::Open,
            }),
        }
    }
    
    async fn cancel_order(
        &self,
        _order_id: &str,
    ) -> Result<(), ApiError> {
        Ok(())
    }
    
    async fn cancel_all_orders(
        &self,
        _market_id: &str,
    ) -> Result<(), ApiError> {
        Ok(())
    }
    
    async fn get_open_orders(
        &self,
    ) -> Result<Vec<Order>, ApiError> {
        Ok(vec![])
    }
    
    async fn get_positions(
        &self,
    ) -> Result<Vec<Position>, ApiError> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_side_buy() {
        let side = Side::Buy;
        assert!(matches!(side, Side::Buy));
    }

    #[test]
    fn test_order_creation() {
        let order = Order {
            market_id: "0xabc".to_string(),
            side: Side::Buy,
            size: 5.0,
            price: 0.5,
        };
        assert_eq!(order.market_id, "0xabc");
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.size, 5.0);
        assert_eq!(order.price, 0.5);
    }

    #[test]
    fn test_order_response() {
        let response = OrderResponse {
            order_id: "order_123".to_string(),
            status: OrderStatus::Open,
        };
        assert_eq!(response.order_id, "order_123");
        assert_eq!(response.status, OrderStatus::Open);
    }

    #[test]
    fn test_position_creation() {
        let position = Position {
            market_id: "0xabc".to_string(),
            side: Side::Buy,
            size: 10.0,
            avg_price: 0.5,
        };
        assert_eq!(position.market_id, "0xabc");
        assert_eq!(position.size, 10.0);
        assert_eq!(position.avg_price, 0.5);
    }

    #[tokio::test]
    async fn test_mock_clob_client_place_order() {
        let client = MockClobClient::new();
        let order = Order {
            market_id: "0xabc".to_string(),
            side: Side::Buy,
            size: 5.0,
            price: 0.5,
        };
        
        let result = client.place_order(order).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.status, OrderStatus::Open);
    }

    #[tokio::test]
    async fn test_mock_clob_client_cancel_order() {
        let client = MockClobClient::new();
        let result = client.cancel_order("order_123").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_clob_client_get_positions() {
        let client = MockClobClient::new();
        let positions = client.get_positions().await.unwrap();
        assert!(positions.is_empty());
    }
}