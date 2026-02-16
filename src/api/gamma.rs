//! Gamma API client for market data

use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use crate::api::ApiError;

const GAMMA_API_URL: &str = "https://gamma-api.polymarket.com";

/// Gamma API client
pub struct GammaApiClient {
    client: Client,
    base_url: String,
}

impl GammaApiClient {
    /// Create new Gamma API client with timeout
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            client,
            base_url: GAMMA_API_URL.to_string(),
        }
    }
    
    /// Fetch active markets
    pub async fn fetch_active_markets(
        &self,
    ) -> Result<Vec<Market>, ApiError> {
        let url = format!("{}/markets", self.base_url);
        let response = self.client
            .get(&url)
            .query(&[("active", "true"), ("closed", "false")])
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(ApiError::ApiError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }
        
        let markets: Vec<Market> = response.json().await?;
        Ok(markets)
    }
    
    /// Fetch settled markets (已结算的市场)
    pub async fn fetch_settled_markets(
        &self,
    ) -> Result<Vec<Market>, ApiError> {
        let url = format!("{}/markets", self.base_url);
        let response = self.client
            .get(&url)
            .query(&[("resolved", "true"), ("closed", "true")])
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(ApiError::ApiError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }
        
        let markets: Vec<Market> = response.json().await?;
        Ok(markets)
    }
    
    /// Fetch markets by condition IDs
    pub async fn fetch_markets_by_ids(
        &self,
        condition_ids: &[String],
    ) -> Result<Vec<Market>, ApiError> {
        let url = format!("{}/markets", self.base_url);
        let ids = condition_ids.join(",");
        let response = self.client
            .get(&url)
            .query(&[("conditionIds", ids)])
            .send()
            .await?;
        
        let markets: Vec<Market> = response.json().await?;
        Ok(markets)
    }
}

impl Default for GammaApiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Market data from Gamma API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Market {
    #[serde(rename = "conditionId")]
    pub condition_id: String,
    pub question: String,
    pub slug: String,
    pub description: String,
    pub outcomes: Vec<String>,
    #[serde(rename = "outcomePrices", deserialize_with = "deserialize_string_f64_vec")]
    pub outcome_prices: Vec<f64>,
    #[serde(deserialize_with = "deserialize_string_f64")]
    pub volume: f64,
    #[serde(deserialize_with = "deserialize_string_f64")]
    pub liquidity: f64,
    #[serde(rename = "startDate")]
    pub start_date: String,
    #[serde(rename = "endDate")]
    pub end_date: String,
    pub resolved: bool,
    pub resolution: Option<String>,
}

/// Deserialize string to f64
fn deserialize_string_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    s.parse::<f64>().map_err(serde::de::Error::custom)
}

/// Deserialize Vec<String> to Vec<f64>
fn deserialize_string_f64_vec<'de, D>(deserializer: D) -> Result<Vec<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec: Vec<String> = Deserialize::deserialize(deserializer)?;
    vec.into_iter()
        .map(|s| s.parse::<f64>().map_err(serde::de::Error::custom))
        .collect()
}

impl Market {
    /// Check if market is active and not resolved
    pub fn is_active(&self) -> bool {
        !self.resolved
    }
    
    /// Get best price for outcome
    pub fn get_price(&self, outcome_index: usize) -> Option<f64> {
        self.outcome_prices.get(outcome_index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_is_active() {
        let market = Market {
            condition_id: "0x123".to_string(),
            question: "Test?".to_string(),
            slug: "test".to_string(),
            description: "Test market".to_string(),
            outcomes: vec!["Yes".to_string(), "No".to_string()],
            outcome_prices: vec![0.5, 0.5],
            volume: 1000.0,
            liquidity: 500.0,
            start_date: "2024-01-01".to_string(),
            end_date: "2024-12-31".to_string(),
            resolved: false,
            resolution: None,
        };
        assert!(market.is_active());
    }

    #[test]
    fn test_market_is_not_active_when_resolved() {
        let market = Market {
            condition_id: "0x123".to_string(),
            question: "Test?".to_string(),
            slug: "test".to_string(),
            description: "Test market".to_string(),
            outcomes: vec!["Yes".to_string(), "No".to_string()],
            outcome_prices: vec![0.5, 0.5],
            volume: 1000.0,
            liquidity: 500.0,
            start_date: "2024-01-01".to_string(),
            end_date: "2024-12-31".to_string(),
            resolved: true,
            resolution: Some("Yes".to_string()),
        };
        assert!(!market.is_active());
    }

    #[test]
    fn test_get_price() {
        let market = Market {
            condition_id: "0x123".to_string(),
            question: "Test?".to_string(),
            slug: "test".to_string(),
            description: "Test market".to_string(),
            outcomes: vec!["Yes".to_string(), "No".to_string()],
            outcome_prices: vec![0.7, 0.3],
            volume: 1000.0,
            liquidity: 500.0,
            start_date: "2024-01-01".to_string(),
            end_date: "2024-12-31".to_string(),
            resolved: false,
            resolution: None,
        };
        assert_eq!(market.get_price(0), Some(0.7));
        assert_eq!(market.get_price(1), Some(0.3));
        assert_eq!(market.get_price(2), None);
    }

    #[test]
    fn test_client_default() {
        let client = GammaApiClient::default();
        assert_eq!(client.base_url, GAMMA_API_URL);
    }

    #[test]
    fn test_deserialize_string_f64() {
        // Test the deserialize_string_f64 function via Market
        let json = r#"{
            "conditionId": "0x123",
            "question": "Test?",
            "slug": "test",
            "description": "Test market",
            "outcomes": ["Yes", "No"],
            "outcomePrices": ["0.7", "0.3"],
            "volume": "123.45",
            "liquidity": "500.0",
            "startDate": "2024-01-01",
            "endDate": "2024-12-31",
            "resolved": false,
            "resolution": null
        }"#;
        
        let market: Market = serde_json::from_str(json).unwrap();
        assert_eq!(market.volume, 123.45);
    }

    #[test]
    fn test_deserialize_market_with_string_prices() {
        let json = r#"{
            "conditionId": "0x123",
            "question": "Test?",
            "slug": "test",
            "description": "Test market",
            "outcomes": ["Yes", "No"],
            "outcomePrices": ["0.7", "0.3"],
            "volume": "1000.5",
            "liquidity": "500.25",
            "startDate": "2024-01-01",
            "endDate": "2024-12-31",
            "resolved": false,
            "resolution": null
        }"#;
        
        let market: Market = serde_json::from_str(json).unwrap();
        assert_eq!(market.condition_id, "0x123");
        assert_eq!(market.outcome_prices, vec![0.7, 0.3]);
        assert_eq!(market.volume, 1000.5);
        assert_eq!(market.liquidity, 500.25);
    }
}