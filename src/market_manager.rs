//! SimpleMarketManager - 复刻 Python poly_data.market_discovery
//! 
//! 管理 BTC 5 分钟市场的自动检测和轮换

use tracing::info;

/// 市场信息
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MarketInfo {
    #[allow(dead_code)]
    pub token1: String,           // YES token
    #[allow(dead_code)]
    pub token2: String,           // NO token
    #[allow(dead_code)]
    pub question: String,
    #[allow(dead_code)]
    pub slug: String,
    #[allow(dead_code)]
    pub market_slug: String,
    #[allow(dead_code)]
    pub condition_id: String,
    #[allow(dead_code)]
    pub end_time: i64,            // Unix timestamp
}

/// 简单市场管理器 - 复刻 Python SimpleMarketManager
#[allow(dead_code)]
pub struct SimpleMarketManager {
    market_series: String,        // e.g., "btc-updown-5m"
    market_type: String,          // e.g., "high_freq"
    cached_markets: Vec<MarketInfo>,
    last_update: i64,
}

impl SimpleMarketManager {
    /// 创建新的市场管理器
    #[allow(dead_code)]
    pub fn new(market_series: &str, market_type: &str) -> Self {
        info!("📊 SimpleMarketManager created: {} ({})", market_series, market_type);
        Self {
            market_series: market_series.to_string(),
            market_type: market_type.to_string(),
            cached_markets: Vec::new(),
            last_update: 0,
        }
    }

    /// 获取当前活跃市场 - 复刻 Python get_current_market()
    #[allow(dead_code)]
    pub fn get_current_market(&self,
        all_markets: &[rs_clob_client::Market]
    ) -> Option<MarketInfo> {
        let now = chrono::Utc::now().timestamp();
        let current_slot = (now / 300) * 300;  // 5分钟时间槽

        // 查找匹配的市场
        for market in all_markets {
            let slug = market.slug.as_deref().unwrap_or("").to_lowercase();
            
            // 匹配市场系列 (btc-updown-5m)
            if !slug.contains(&self.market_series) {
                continue;
            }

            // 尝试从 slug 提取时间槽
            let time_slot = slug.rfind('-')
                .and_then(|idx| slug[idx+1..].parse::<i64>().ok());

            // 检查是否在当前时间槽
            if let Some(slot) = time_slot {
                if slot == current_slot {
                    // 检查市场是否仍然活跃
                    if let Some(ref end_date) = market.end_date {
                        if let Some(end_time) = parse_end_time(end_date) {
                            let time_to_expiry = end_time - now;
                            if time_to_expiry > 30 {  // 至少30秒剩余
                                return Some(self.convert_to_market_info(market));
                            }
                        }
                    }
                }
            }
        }

        // 如果没有找到当前槽的市场，返回最近的市场
        self.find_nearest_market(all_markets, now)
    }

    /// 转换 rs_clob_client::Market 到 MarketInfo
    #[allow(dead_code)]
    fn convert_to_market_info(&self,
        market: &rs_clob_client::Market
    ) -> MarketInfo {
        // 从市场数据中提取 token IDs
        // 注意：实际实现需要根据 Polymarket API 响应格式调整
        let condition_id = market.condition_id.clone().unwrap_or_default();
        
        MarketInfo {
            token1: condition_id.clone(),  // 实际应该是 YES token
            token2: format!("{}_no", condition_id),  // 实际应该是 NO token
            question: market.question.clone().unwrap_or_default(),
            slug: market.slug.clone().unwrap_or_default(),
            market_slug: market.slug.clone().unwrap_or_default(),
            condition_id,
            end_time: market.end_date.as_ref()
                .and_then(|d| parse_end_time(d))
                .unwrap_or(0),
        }
    }

    /// 找到最近的市场（用于回退）
    #[allow(dead_code)]
    fn find_nearest_market(
        &self,
        all_markets: &[rs_clob_client::Market],
        now: i64
    ) -> Option<MarketInfo> {
        let mut best_market: Option<(&rs_clob_client::Market, i64)> = None;

        for market in all_markets {
            let slug = market.slug.as_deref().unwrap_or("").to_lowercase();
            
            if !slug.contains(&self.market_series) {
                continue;
            }

            if let Some(ref end_date) = market.end_date {
                if let Some(end_time) = parse_end_time(end_date) {
                    let time_to_expiry = end_time - now;
                    if time_to_expiry > 30 {
                        match best_market {
                            None => best_market = Some((market, time_to_expiry)),
                            Some((_, best_expiry)) if time_to_expiry > best_expiry => {
                                best_market = Some((market, time_to_expiry));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        best_market.map(|(m, _)| self.convert_to_market_info(m))
    }
}

/// 解析结束时间
#[allow(dead_code)]
fn parse_end_time(end_date: &str) -> Option<i64> {
    // Try RFC3339 format first
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(end_date) {
        return Some(dt.timestamp());
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
            return Some(dt.and_utc().timestamp());
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_market_manager() {
        let manager = SimpleMarketManager::new("btc-updown-5m", "high_freq");
        assert_eq!(manager.market_series, "btc-updown-5m");
        assert_eq!(manager.market_type, "high_freq");
    }
}