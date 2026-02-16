//! Market and Token types
//! Matches Python market structure with tokens

use serde::{Deserialize, Serialize};

/// Market token info (UP/DOWN)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketToken {
    pub token_id: String,
    pub outcome: String,  // "UP" or "DOWN"
    pub price: Option<f64>,
}

/// Market with token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub condition_id: String,
    pub slug: String,
    pub question: String,
    pub tokens: Vec<MarketToken>,
    pub outcome_prices: Option<Vec<f64>>,
}

impl MarketInfo {
    /// Get UP token
    pub fn up_token(&self) -> Option<&MarketToken> {
        self.tokens.iter().find(|t| t.outcome == "UP" || t.outcome == "Yes")
    }
    
    /// Get DOWN token
    pub fn down_token(&self) -> Option<&MarketToken> {
        self.tokens.iter().find(|t| t.outcome == "DOWN" || t.outcome == "No")
    }
    
    /// Get token by outcome
    pub fn get_token(&self, outcome: &str) -> Option<&MarketToken> {
        self.tokens.iter().find(|t| t.outcome == outcome)
    }
}

/// Convert from rs_clob_client::Market to MarketInfo
pub fn convert_market(market: &rs_clob_client::Market) -> MarketInfo {
    let mut tokens = Vec::new();
    
    // Use condition_id as token_id (fallback behavior)
    if let Some(condition_id) = &market.condition_id {
        // For binary markets, create UP and DOWN tokens
        tokens.push(MarketToken {
            token_id: condition_id.clone(),
            outcome: "UP".to_string(),
            price: None,
        });
        // Note: In real implementation, you'd get the actual token IDs from the API
    }
    
    MarketInfo {
        condition_id: market.condition_id.clone().unwrap_or_default(),
        slug: market.slug.clone().unwrap_or_default(),
        question: market.question.clone().unwrap_or_default(),
        tokens,
        outcome_prices: market.outcome_prices.as_ref()
            .and_then(|p| serde_json::from_str(p).ok()),
    }
}