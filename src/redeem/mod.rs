//! Builder Relayer integration for gasless redemption

use serde::{Deserialize, Serialize};
use crate::wallet::SafeWallet;
use std::sync::Arc;

const BUILDER_RELAYER_URL: &str = "https://relayer.polymarket.com";

/// Builder Relayer client for gasless transactions
pub struct BuilderRelayer {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    api_secret: String,
    api_passphrase: String,
}

impl BuilderRelayer {
    /// Create new Builder Relayer client
    pub fn new(
        api_key: String,
        api_secret: String,
        api_passphrase: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: BUILDER_RELAYER_URL.to_string(),
            api_key,
            api_secret,
            api_passphrase,
        }
    }
    
    /// Get authentication headers
    fn get_headers(&self) -> anyhow::Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "POLYMARKET_API_KEY",
            self.api_key.parse()
                .map_err(|_| anyhow::anyhow!("Invalid API key format"))?,
        );
        headers.insert(
            "POLYMARKET_API_SECRET",
            self.api_secret.parse()
                .map_err(|_| anyhow::anyhow!("Invalid API secret format"))?,
        );
        headers.insert(
            "POLYMARKET_API_PASSPHRASE",
            self.api_passphrase.parse()
                .map_err(|_| anyhow::anyhow!("Invalid API passphrase format"))?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse()
                .map_err(|_| anyhow::anyhow!("Invalid content type"))?,
        );
        Ok(headers)
    }
    
    /// Submit redemption transaction to Builder Relayer
    pub async fn submit_redeem(
        &self,
        safe: &SafeWallet,
        condition_id: &str,
        amount: u64,
        signature: &str,
    ) -> Result<RedeemResponse, RelayerError> {
        let url = format!("{}/redeem", self.base_url);
        
        let request = RedeemRequest {
            safe_address: safe.address().to_string(),
            condition_id: condition_id.to_string(),
            amount,
            signature: signature.to_string(),
            nonce: safe.nonce(),
        };
        
        let response = self.client
            .post(&url)
            .headers(self.get_headers().map_err(|e| RelayerError::ApiError {
                status: 400,
                message: e.to_string(),
            })?)
            .json(&request)
            .send()
            .await
            .map_err(|e| RelayerError::HttpError(e.to_string()))?;
        
        let status_code = response.status();
        if !status_code.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(RelayerError::ApiError {
                status: status_code.as_u16(),
                message: error_text,
            });
        }
        
        let redeem_response: RedeemResponse = response
            .json()
            .await
            .map_err(|e| RelayerError::ParseError(e.to_string()))?;
        
        Ok(redeem_response)
    }
    
    /// Check redemption status
    pub async fn get_redeem_status(
        &self,
        tx_hash: &str,
    ) -> Result<RedeemStatus, RelayerError> {
        let url = format!("{}/redeem/status/{}", self.base_url, tx_hash);
        
        let response = self.client
            .get(&url)
            .headers(self.get_headers().map_err(|e| RelayerError::ApiError {
                status: 400,
                message: e.to_string(),
            })?)
            .send()
            .await
            .map_err(|e| RelayerError::HttpError(e.to_string()))?;
        
        let status_code = response.status();
        if !status_code.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(RelayerError::ApiError {
                status: status_code.as_u16(),
                message: error_text,
            });
        }
        
        let status: RedeemStatus = response
            .json()
            .await
            .map_err(|e| RelayerError::ParseError(e.to_string()))?;
        
        Ok(status)
    }
}

/// Redemption request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemRequest {
    #[serde(rename = "safeAddress")]
    pub safe_address: String,
    #[serde(rename = "conditionId")]
    pub condition_id: String,
    pub amount: u64,
    pub signature: String,
    pub nonce: u64,
}

/// Redemption response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemResponse {
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    pub status: String,
}

/// Redemption status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemStatus {
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    pub status: String,
    #[serde(rename = "blockNumber")]
    pub block_number: Option<u64>,
    #[serde(rename = "gasUsed")]
    pub gas_used: Option<u64>,
}

/// Auto-redemption service
pub struct AutoRedeemService {
    relayer: BuilderRelayer,
    safe: SafeWallet,
}

impl AutoRedeemService {
    /// Create new auto-redeem service
    pub fn new(relayer: BuilderRelayer, safe: SafeWallet) -> Self {
        Self { relayer, safe }
    }
    
    /// Check and redeem settled markets
    pub async fn redeem_settled_markets(
        &mut self,
        markets: Vec<SettledMarket>,
        wallet: Arc<dyn crate::wallet::Wallet>,
    ) -> Result<Vec<RedeemResult>, RelayerError> {
        let mut results = Vec::new();
        
        for market in markets {
            match self.redeem_market(&market, wallet.clone()).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    tracing::error!("Failed to redeem market {}: {}", market.condition_id, e);
                    results.push(RedeemResult {
                        condition_id: market.condition_id,
                        success: false,
                        transaction_hash: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
        
        Ok(results)
    }
    
    /// Redeem a single market
    async fn redeem_market(
        &mut self,
        market: &SettledMarket,
        wallet: Arc<dyn crate::wallet::Wallet>,
    ) -> Result<RedeemResult, RelayerError> {
        // Create redemption message
        let message = format!(
            "Redeem {} for market {}",
            market.amount, market.condition_id
        );
        
        // Sign the redemption message
        let signature = wallet
            .sign_message(message.as_bytes())
            .await
            .map_err(|e| RelayerError::WalletError(e.to_string()))?;
        
        // Submit to relayer
        let response = self.relayer.submit_redeem(
            &self.safe,
            &market.condition_id,
            market.amount,
            &hex::encode(signature),
        ).await?;
        
        // Increment nonce for next transaction
        self.safe.increment_nonce();
        
        Ok(RedeemResult {
            condition_id: market.condition_id.clone(),
            success: true,
            transaction_hash: Some(response.transaction_hash),
            error: None,
        })
    }
}

/// Settled market info
#[derive(Debug, Clone)]
pub struct SettledMarket {
    pub condition_id: String,
    pub amount: u64,
    pub outcome: String,
}

/// Redemption result
#[derive(Debug, Clone)]
pub struct RedeemResult {
    pub condition_id: String,
    pub success: bool,
    pub transaction_hash: Option<String>,
    pub error: Option<String>,
}

/// Relayer errors
#[derive(thiserror::Error, Debug)]
pub enum RelayerError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Wallet error: {0}")]
    WalletError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redeem_request_serialization() {
        let request = RedeemRequest {
            safe_address: "0x123".to_string(),
            condition_id: "0xabc".to_string(),
            amount: 1000000,
            signature: "0xdef".to_string(),
            nonce: 5,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("safeAddress"));
        assert!(json.contains("conditionId"));
        assert!(json.contains("1000000"));
    }

    #[test]
    fn test_redeem_response_deserialization() {
        let json = r#"{"transactionHash":"0x123","status":"pending"}"#;
        let response: RedeemResponse = serde_json::from_str(json).unwrap();
        
        assert_eq!(response.transaction_hash, "0x123");
        assert_eq!(response.status, "pending");
    }

    #[test]
    fn test_redeem_status_deserialization() {
        let json = r#"{"transactionHash":"0x123","status":"confirmed","blockNumber":12345,"gasUsed":50000}"#;
        let status: RedeemStatus = serde_json::from_str(json).unwrap();
        
        assert_eq!(status.transaction_hash, "0x123");
        assert_eq!(status.status, "confirmed");
        assert_eq!(status.block_number, Some(12345));
        assert_eq!(status.gas_used, Some(50000));
    }

    #[test]
    fn test_redeem_result() {
        let result = RedeemResult {
            condition_id: "0xabc".to_string(),
            success: true,
            transaction_hash: Some("0x123".to_string()),
            error: None,
        };
        
        assert!(result.success);
        assert!(result.transaction_hash.is_some());
        assert_eq!(result.transaction_hash.unwrap(), "0x123");
    }

    #[test]
    fn test_redeem_result_failure() {
        let result = RedeemResult {
            condition_id: "0xabc".to_string(),
            success: false,
            transaction_hash: None,
            error: Some("Insufficient balance".to_string()),
        };
        
        assert!(!result.success);
        assert!(result.transaction_hash.is_none());
        assert_eq!(result.error.unwrap(), "Insufficient balance");
    }

    #[test]
    fn test_settled_market() {
        let market = SettledMarket {
            condition_id: "0xabc".to_string(),
            amount: 1000000,
            outcome: "Yes".to_string(),
        };
        
        assert_eq!(market.condition_id, "0xabc");
        assert_eq!(market.amount, 1000000);
        assert_eq!(market.outcome, "Yes");
    }

    #[test]
    fn test_relayer_error_display() {
        let err = RelayerError::HttpError("Connection refused".to_string());
        assert!(err.to_string().contains("Connection refused"));
        
        let err = RelayerError::ApiError {
            status: 400,
            message: "Bad request".to_string(),
        };
        assert!(err.to_string().contains("400"));
        assert!(err.to_string().contains("Bad request"));
    }
}