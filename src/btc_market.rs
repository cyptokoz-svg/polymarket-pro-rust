//! BTC 5-minute market finder
//! Matches Python SimpleMarketManager logic

use polymarket_pro::trading::TradeExecutor;
use tracing::{error, info, warn};
use polymarket_client_sdk::gamma::types::Market;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_time_slot_calculation() {
        // Python: current_slot = floor(now / 300) * 300
        let test_cases = vec![
            (1771215121i64, 1771215000i64),  // 现在
            (1771215300i64, 1771215300i64),  // 正好在槽点
            (1771214999i64, 1771214700i64),  // 槽点前1秒
        ];
        
        for (now, expected) in test_cases {
            let slot = (now / 300) * 300;
            assert_eq!(slot, expected, "Failed for now={}", now);
        }
    }

    #[test]
    fn test_slug_parsing() {
        // 测试 slug 解析
        let slug = "btc-updown-5m-1771215000";
        let time_slot: Option<i64> = slug.rfind('-')
            .and_then(|idx| slug[idx+1..].parse().ok());
        assert_eq!(time_slot, Some(1771215000));
        
        // 测试市场匹配逻辑
        let is_btc_updown = slug.contains("btc-updown") || slug.contains("btc");
        assert!(is_btc_updown);
    }
}

/// Parse market end time from string
fn parse_market_end_time(end_date: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Try RFC3339 format first
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(end_date) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    
    // Try other common formats
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.3fZ",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d",
    ];
    
    for format in &formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(end_date, format) {
            return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
        }
    }
    
    None
}

/// Find BTC up/down 5-minute market by time slot
/// Market URL pattern: btc-updown-5m-{unix_timestamp}
/// Each market lasts 5 minutes (300 seconds)
pub async fn find_btc_5min_market(
    executor: &TradeExecutor,
) -> Option<Market> {
    // Try direct Gamma API first for BTC markets
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
                        // Convert to Market type
                        if let Some(market) = convert_to_market(market_data) {
                            return Some(market);
                        }
                    }
                }
            }
        }
    }
    
    // Fallback: scan all markets from executor
    match executor.get_markets().await {
        Ok(markets) => {
            info!("🔍 Scanning {} markets from executor...", markets.len());
            
            let mut btc_markets: Vec<(Market, i64, Option<i64>)> = Vec::new();
            
            for m in markets {
                let slug = m.slug.as_deref().unwrap_or("").to_lowercase();
                let question = m.question.as_deref().unwrap_or("").to_lowercase();
                
                let is_btc_updown = slug.contains("btc-updown")
                    || slug.contains("btc")
                    || question.contains("btc updown")
                    || question.contains("btc");
                
                if !is_btc_updown {
                    continue;
                }
                
                let time_slot: Option<i64> = slug.rfind('-')
                    .and_then(|idx| slug[idx+1..].parse().ok());
                
                if let Some(ref end_date_str) = m.end_date {
                    if let Some(end_date) = parse_market_end_time(end_date_str) {
                        let time_to_expiry = end_date.signed_duration_since(now).num_seconds();
                        if time_to_expiry > 30 {
                            info!("🎯 Found BTC updown: slug='{}', expires in {}s", 
                                m.slug.as_deref().unwrap_or("N/A"),
                                time_to_expiry);
                            btc_markets.push((m, time_to_expiry, time_slot));
                        }
                    }
                }
            }
            
            info!("📊 Found {} BTC updown markets from executor", btc_markets.len());
            
            // Select best market
            for (m, expiry, slot) in &btc_markets {
                if slot.map(|s| s == current_slot).unwrap_or(false) {
                    info!("✅ Selected current slot market: '{}' (expires in {}s)", 
                        m.slug.as_deref().unwrap_or("N/A"), expiry);
                    return Some(m.clone());
                }
            }
            
            if let Some((m, expiry, _)) = btc_markets.into_iter().max_by_key(|(_, expiry, _)| *expiry) {
                info!("✅ Selected fallback market: '{}' (expires in {}s)", 
                    m.slug.as_deref().unwrap_or("N/A"), expiry);
                return Some(m);
            }
        }
        Err(e) => {
            error!("❌ Failed to get markets from executor: {}", e);
        }
    }
    
    warn!("⚠️ No active BTC updown 5m market found");
    None
}

/// Convert Gamma API market data to Market type
fn convert_to_market(data: &serde_json::Value) -> Option<Market> {
    serde_json::from_value(data.clone()).ok()
}

/// Get token IDs for a BTC market from Gamma API
pub async fn get_market_token_ids(condition_id: &str) -> Option<(String, String)> {
    // Try to find market by slug pattern first
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
                    
                    // Extract clobTokenIds - it's a JSON string, not an array
                    if let Some(token_ids_str) = market.get("clobTokenIds").and_then(|v| v.as_str()) {
                        // Parse the JSON string into an array
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