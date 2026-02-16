//! BTC 5-minute market finder - Simplified version

use polymarket_pro::trading::TradeExecutor;
use tracing::{info, warn};

/// Find BTC up/down 5-minute market by time slot
/// Simplified implementation using JSON directly
pub async fn find_btc_5min_market(
    _executor: &TradeExecutor,
) -> Option<serde_json::Value> {
    let now = chrono::Utc::now();
    let now_timestamp = now.timestamp();
    let current_slot = (now_timestamp / 300) * 300;
    
    info!("🔍 Looking for BTC 5-min market at slot: {}", current_slot);
    
    // Try to fetch specific market by slug pattern
    let slug_patterns = vec![
        format!("btc-updown-5m-{}", current_slot),
        format!("btc-updown-5m-{}", current_slot - 300),
        format!("btc-updown-5m-{}", current_slot + 300),
    ];
    
    for slug in &slug_patterns {
        let url = format!("https://gamma-api.polymarket.com/markets?slug={}", slug);
        if let Ok(resp) = reqwest::get(&url).await {
            if let Ok(json) = resp.json::<Vec<serde_json::Value>>().await {
                if let Some(market_data) = json.first() {
                    let active = market_data.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
                    let closed = market_data.get("closed").and_then(|v| v.as_bool()).unwrap_or(true);
                    
                    if active && !closed {
                        info!("✅ Found active BTC market: {}", slug);
                        return Some(market_data.clone());
                    }
                }
            }
        }
    }
    
    warn!("⚠️ No active BTC updown 5m market found");
    None
}

/// Get token IDs for a BTC market from Gamma API
pub async fn get_market_token_ids(condition_id: &str) -> Option<(String, String)> {
    let now = chrono::Utc::now().timestamp();
    let current_slot = (now / 300) * 300;
    let slug = format!("btc-updown-5m-{}", current_slot);
    
    let url = format!("https://gamma-api.polymarket.com/markets?slug={}", slug);
    
    match reqwest::get(&url).await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<Vec<serde_json::Value>>().await {
                if let Some(market) = json.first() {
                    // Check if this is the correct market
                    let market_condition_id = market.get("conditionId").and_then(|v| v.as_str());
                    if market_condition_id != Some(condition_id) {
                        warn!("Condition ID mismatch: expected {}, got {:?}", 
                            condition_id, market_condition_id);
                        return None;
                    }
                    
                    // Extract clobTokenIds
                    if let Some(token_ids_str) = market.get("clobTokenIds").and_then(|v| v.as_str()) {
                        if let Ok(token_ids) = serde_json::from_str::<Vec<String>>(token_ids_str) {
                            if token_ids.len() >= 2 {
                                let up_token = token_ids[0].clone();
                                let down_token = token_ids[1].clone();
                                info!("🎯 Got token IDs: UP={}, DOWN={}", 
                                    &up_token[..20.min(up_token.len())], 
                                    &down_token[..20.min(down_token.len())]);
                                return Some((up_token, down_token));
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to fetch token IDs: {}", e);
        }
    }
    
    None
}