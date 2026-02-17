//! WebSocket client - 完全复刻 Python websocket_client.py

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

const WS_TIMEOUT_SECONDS: u64 = 30;
const WS_RECONNECT_DELAY: u64 = 5;
const WS_PING_INTERVAL: u64 = 5;
const DISPLAY_INTERVAL: f64 = 1.0; // 每1秒打印一次价格（实时更新）
#[allow(dead_code)]
const MAX_CACHE_SIZE: usize = 1000;

/// WebSocket URL
const MARKET_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

/// 订阅消息
#[derive(Debug, Clone, Serialize)]
struct SubscribeMessage {
    #[serde(rename = "assets_ids")]
    asset_ids: Vec<String>,
}

/// 订单簿条目
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct OrderBookEntry {
    #[allow(dead_code)]
    price: String,
    #[allow(dead_code)]
    size: String,
}

/// Book 事件格式
#[derive(Debug, Clone, Deserialize)]
struct BookEvent {
    #[serde(rename = "event_type")]
    event_type: String,
    #[serde(rename = "asset_id")]
    asset_id: String,
    bids: Vec<HashMap<String, String>>,
    asks: Vec<HashMap<String, String>>,
}

/// Price change 条目格式
#[derive(Debug, Clone, Deserialize)]
struct PriceChangeEntry {
    #[serde(rename = "asset_id")]
    asset_id: String,
    price: String,
    size: String,
}

/// Price changes 消息格式
#[derive(Debug, Clone, Deserialize)]
struct PriceChangesEvent {
    market: String,
    #[serde(rename = "price_changes")]
    price_changes: Vec<PriceChangeEntry>,
}

/// 价格更新
#[derive(Debug, Clone)]
pub struct PriceUpdate {
    pub token_id: String,
    pub bid: f64,
    pub ask: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Polymarket WebSocket 客户端 - 复刻 Python 版本
pub struct PolymarketWebSocket {
    /// 当前订阅的 token IDs
    subscribed_tokens: Arc<RwLock<Vec<String>>>,
    /// Token 标签映射 (token_id -> "UP"/"DOWN")
    token_labels: Arc<RwLock<HashMap<String, String>>>,
    /// 最新价格缓存 (token_id_bid/ask -> price)
    last_prices: Arc<RwLock<HashMap<String, f64>>>,
    /// 消息计数器（用于轮询 token）
    msg_counter: Arc<RwLock<usize>>,
    /// 运行状态
    running: Arc<RwLock<bool>>,
    /// 最后显示时间
    last_display_time: Arc<RwLock<f64>>,
    /// 统计
    messages_received: Arc<RwLock<u64>>,
    /// 重启标志 - 用于市场切换时重新连接
    restart_flag: Arc<RwLock<bool>>,
}

impl PolymarketWebSocket {
    /// 创建新的 WebSocket 客户端
    pub fn new() -> Self {
        Self {
            subscribed_tokens: Arc::new(RwLock::new(Vec::new())),
            token_labels: Arc::new(RwLock::new(HashMap::new())),
            last_prices: Arc::new(RwLock::new(HashMap::new())),
            msg_counter: Arc::new(RwLock::new(0)),
            running: Arc::new(RwLock::new(false)),
            last_display_time: Arc::new(RwLock::new(0.0)),
            messages_received: Arc::new(RwLock::new(0)),
            restart_flag: Arc::new(RwLock::new(false)),
        }
    }

    /// 设置 token 标签
    pub async fn set_token_labels(&self, labels: HashMap<String, String>) {
        let mut token_labels = self.token_labels.write().await;
        *token_labels = labels;
    }

    /// 启动 WebSocket 连接
    pub async fn start(&self, token_ids: Vec<String>) {
        {
            let mut running = self.running.write().await;
            *running = true;
        }
        {
            let mut tokens = self.subscribed_tokens.write().await;
            *tokens = token_ids.clone();
        }

        info!("🚀 WebSocket client starting ({} markets)", token_ids.len());

        // 克隆 Arc 用于任务
        let subscribed_tokens = self.subscribed_tokens.clone();
        let last_prices = self.last_prices.clone();
        let msg_counter = self.msg_counter.clone();
        let running = self.running.clone();
        let last_display_time = self.last_display_time.clone();
        let messages_received = self.messages_received.clone();
        let token_labels = self.token_labels.clone();

        // 启动连接任务
        tokio::spawn(async move {
            Self::connect_market(
                subscribed_tokens,
                last_prices,
                msg_counter,
                running,
                last_display_time,
                messages_received,
                token_labels,
            ).await;
        });
    }

    /// 停止 WebSocket
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("WebSocket client stopping...");
    }
    
    /// 重启 WebSocket - 用于市场切换时重新订阅
    pub async fn restart(&self) {
        info!("🔄 Restarting WebSocket for new subscription...");
        // Stop current connection
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        // Wait for connection to close (like Python's time.sleep(1))
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // Clear prices
        {
            let mut prices = self.last_prices.write().await;
            prices.clear();
        }
        
        // Get current tokens
        let token_ids = {
            self.subscribed_tokens.read().await.clone()
        };
        
        // Start again - restart the connection task
        {
            let mut running = self.running.write().await;
            *running = true;
        }
        
        // Clone Arc for new task
        let subscribed_tokens = self.subscribed_tokens.clone();
        let last_prices = self.last_prices.clone();
        let msg_counter = self.msg_counter.clone();
        let running = self.running.clone();
        let last_display_time = self.last_display_time.clone();
        let messages_received = self.messages_received.clone();
        let token_labels = self.token_labels.clone();
        
        // Spawn new connection task
        tokio::spawn(async move {
            Self::connect_market(
                subscribed_tokens,
                last_prices,
                msg_counter,
                running,
                last_display_time,
                messages_received,
                token_labels,
            ).await;
        });
        
        info!("✅ WebSocket restarted with {} markets", token_ids.len());
    }

    /// 更新订阅的 token - 完全重启 WebSocket（像 Python 一样）
    pub async fn update_subscription(&self, token_ids: Vec<String>) {
        info!("📡 Updating subscription to {} tokens - restarting WebSocket", token_ids.len());
        
        // Stop current connection
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        
        // Wait for connection to close (like Python's time.sleep(1))
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // Update tokens
        {
            let mut tokens = self.subscribed_tokens.write().await;
            *tokens = token_ids.clone();
        }
        
        // Clear old prices (like Python)
        {
            let mut prices = self.last_prices.write().await;
            prices.clear();
            info!("🧹 Cleared old price cache for new market");
        }
        
        // Restart connection - MUST spawn new task
        {
            let mut running = self.running.write().await;
            *running = true;
        }
        
        // Clone Arc for new task
        let subscribed_tokens = self.subscribed_tokens.clone();
        let last_prices = self.last_prices.clone();
        let msg_counter = self.msg_counter.clone();
        let running = self.running.clone();
        let last_display_time = self.last_display_time.clone();
        let messages_received = self.messages_received.clone();
        let token_labels = self.token_labels.clone();
        
        // Spawn new connection task - CRITICAL FIX
        tokio::spawn(async move {
            Self::connect_market(
                subscribed_tokens,
                last_prices,
                msg_counter,
                running,
                last_display_time,
                messages_received,
                token_labels,
            ).await;
        });
        
        info!("✅ WebSocket restarted with {} markets", token_ids.len());
    }

    /// 获取最新价格
    pub async fn get_price(&self, token_id: &str) -> Option<(f64, f64)> {
        // Use FIRST 20 chars of token ID as key to match Gamma API behavior
        // Gamma API returns short IDs (20 chars), WebSocket returns long IDs (77+ chars)
        let short_token_id = if token_id.len() > 20 {
            &token_id[..20]
        } else {
            token_id
        };
        
        let prices = self.last_prices.read().await;
        let bid_key = format!("{}_bid", short_token_id);
        let ask_key = format!("{}_ask", short_token_id);
        let bid = prices.get(&bid_key).copied();
        let ask = prices.get(&ask_key).copied();
        
        // Debug: print available keys if not found
        if bid.is_none() || ask.is_none() {
            info!("DEBUG get_price: token_id={}, short={}, bid_key={}, ask_key={}, bid={:?}, ask={:?}", 
                token_id, short_token_id, bid_key, ask_key, bid, ask);
            info!("DEBUG available keys: {:?}", prices.keys().collect::<Vec<_>>());
        }
        
        match (bid, ask) {
            (Some(b), Some(a)) => Some((b, a)),
            _ => None,
        }
    }

    /// 获取所有订阅 token 的价格
    pub async fn get_all_prices(&self) -> HashMap<String, (f64, f64)> {
        let prices = self.last_prices.read().await;
        let tokens = self.subscribed_tokens.read().await;
        let mut result = HashMap::new();
        
        for token in tokens.iter() {
            // Use short token ID (first 20 chars) to match storage key
            let short_token = if token.len() > 20 { &token[..20] } else { token };
            let bid = prices.get(&format!("{}_bid", short_token)).copied();
            let ask = prices.get(&format!("{}_ask", short_token)).copied();
            if let (Some(b), Some(a)) = (bid, ask) {
                result.insert(token.clone(), (b, a));
            }
        }
        
        result
    }

    /// 获取消息统计
    pub async fn get_stats(&self) -> u64 {
        *self.messages_received.read().await
    }

    /// 市场数据连接循环 - 复刻 Python _connect_market
    async fn connect_market(
        subscribed_tokens: Arc<RwLock<Vec<String>>>,
        last_prices: Arc<RwLock<HashMap<String, f64>>>,
        msg_counter: Arc<RwLock<usize>>,
        running: Arc<RwLock<bool>>,
        last_display_time: Arc<RwLock<f64>>,
        messages_received: Arc<RwLock<u64>>,
        token_labels: Arc<RwLock<HashMap<String, String>>>,
    ) {
        loop {
            // 检查是否停止
            if !*running.read().await {
                break;
            }

            info!("Connecting to WebSocket...");

            match Self::try_connect(
                subscribed_tokens.clone(),
                last_prices.clone(),
                msg_counter.clone(),
                running.clone(),
                last_display_time.clone(),
                messages_received.clone(),
                token_labels.clone(),
            ).await {
                Ok(()) => {
                    warn!("WebSocket closed, reconnecting...");
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                }
            }

            // 重连延迟
            tokio::time::sleep(Duration::from_secs(WS_RECONNECT_DELAY)).await;
        }

        info!("WebSocket connection manager stopped");
    }

    /// 尝试连接 - 复刻 Python 逻辑
    async fn try_connect(
        subscribed_tokens: Arc<RwLock<Vec<String>>>,
        last_prices: Arc<RwLock<HashMap<String, f64>>>,
        msg_counter: Arc<RwLock<usize>>,
        running: Arc<RwLock<bool>>,
        last_display_time: Arc<RwLock<f64>>,
        messages_received: Arc<RwLock<u64>>,
        token_labels: Arc<RwLock<HashMap<String, String>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut ws, _) = tokio::time::timeout(
            Duration::from_secs(WS_TIMEOUT_SECONDS),
            connect_async(MARKET_WS_URL)
        ).await
            .map_err(|_| "Connection timeout")?
            .map_err(|e| format!("Connection error: {}", e))?;

        info!("WebSocket connected");

        // 获取当前订阅的 token
        let tokens = subscribed_tokens.read().await.clone();
        if !tokens.is_empty() {
            let msg = SubscribeMessage { asset_ids: tokens };
            let msg_text = serde_json::to_string(&msg)?;
            ws.send(Message::Text(msg_text)).await?;
            info!("📡 Subscribed to {} markets", msg.asset_ids.len());
        }

        let mut ping_interval = interval(Duration::from_secs(WS_PING_INTERVAL));

        loop {
            tokio::select! {
                msg = tokio::time::timeout(
                    Duration::from_secs(WS_TIMEOUT_SECONDS),
                    ws.next()
                ) => {
                    match msg {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            // 更新统计
                            {
                                let mut counter = messages_received.write().await;
                                *counter += 1;
                            }

                            // 解析消息
                            Self::process_message(
                                &text,
                                &subscribed_tokens,
                                &last_prices,
                                &msg_counter,
                                &last_display_time,
                                &token_labels,
                            ).await;
                        }
                        Ok(Some(Ok(Message::Ping(data)))) => {
                            ws.send(Message::Pong(data)).await?;
                        }
                        Ok(Some(Ok(Message::Close(_)))) => {
                            warn!("WebSocket closed by server");
                            return Ok(());
                        }
                        Ok(Some(Err(e))) => {
                            error!("WebSocket error: {}", e);
                        }
                        Ok(None) => {
                            warn!("WebSocket stream ended");
                            return Ok(());
                        }
                        Err(_) => {
                            // 超时，发送 ping
                            ws.send(Message::Ping(vec![])).await?;
                        }
                        _ => {}
                    }
                }
                _ = ping_interval.tick() => {
                    ws.send(Message::Ping(vec![])).await?;
                }
            }

            // 检查是否停止
            if !*running.read().await {
                let _ = ws.close(None).await;
                return Ok(());
            }
        }
    }

    /// 处理消息 - 复刻 Python _process_market_data
    async fn process_message(
        text: &str,
        _subscribed_tokens: &Arc<RwLock<Vec<String>>>,
        last_prices: &Arc<RwLock<HashMap<String, f64>>>,
        _msg_counter: &Arc<RwLock<usize>>,
        last_display_time: &Arc<RwLock<f64>>,
        token_labels: &Arc<RwLock<HashMap<String, String>>>,
    ) {
        // 打印原始消息前200字符用于调试
        info!("WebSocket raw message: {}...", &text[..text.len().min(200)]);
        
        // 尝试解析为对象（book 事件）- 这是主要的数据来源
        match serde_json::from_str::<BookEvent>(text) {
            Ok(event) => {
                info!("Received book event for asset: {}, event_type: {}, bids: {}, asks: {}", 
                    event.asset_id, event.event_type, event.bids.len(), event.asks.len());
                
                if event.event_type == "book" {
                    Self::process_book_event(
                        event,
                        last_prices,
                        last_display_time,
                        token_labels,
                    ).await;
                }
                return;
            }
            Err(e) => {
                info!("Failed to parse as BookEvent: {}", e);
            }
        }

        // 尝试解析为数组（订单簿快照）- 复刻 Python List 格式处理
        match serde_json::from_str::<Vec<OrderBookEntry>>(text) {
            Ok(entries) => {
                info!("Received orderbook snapshot with {} entries", entries.len());
                // 处理快照 - 使用轮询 token 方式复刻 Python 逻辑
                Self::process_orderbook_snapshot(
                    entries,
                    _subscribed_tokens,
                    last_prices,
                    _msg_counter,
                    last_display_time,
                    token_labels,
                ).await;
                return;
            }
            Err(e) => {
                info!("Failed to parse as Vec<OrderBookEntry>: {}", e);
            }
        }

        // 尝试解析为 price_changes 事件
        match serde_json::from_str::<PriceChangesEvent>(text) {
            Ok(event) => {
                info!("Received price_changes event with {} entries", event.price_changes.len());
                for change in event.price_changes {
                    if let (Ok(price), Ok(_size)) = (change.price.parse::<f64>(), change.size.parse::<f64>()) {
                        let short_token_id = if change.asset_id.len() > 20 {
                            &change.asset_id[..20]
                        } else {
                            &change.asset_id
                        };
                        // 存储为 bid 和 ask（price_changes 通常是最新成交价）
                        let mut prices = last_prices.write().await;
                        prices.insert(format!("{}_bid", short_token_id), price);
                        prices.insert(format!("{}_ask", short_token_id), price);
                        info!("DEBUG storing price_change: key={}_bid/ask, price={}", short_token_id, price);
                    }
                }
                return;
            }
            Err(e) => {
                info!("Failed to parse as PriceChangesEvent: {}", e);
            }
        }

        // 尝试解析为通用 JSON
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(text) {
            debug!("Received other message: {:?}", data);
        }
    }

    /// 处理订单簿快照 - 复刻 Python List 格式处理
    async fn process_orderbook_snapshot(
        entries: Vec<OrderBookEntry>,
        subscribed_tokens: &Arc<RwLock<Vec<String>>>,
        last_prices: &Arc<RwLock<HashMap<String, f64>>>,
        msg_counter: &Arc<RwLock<usize>>,
        last_display_time: &Arc<RwLock<f64>>,
        token_labels: &Arc<RwLock<HashMap<String, String>>>,
    ) {
        let tokens = subscribed_tokens.read().await;
        if tokens.is_empty() {
            return;
        }

        // 轮询 token: token_idx = msg_counter % len(tokens)
        let token_idx = {
            let mut counter = msg_counter.write().await;
            let idx = *counter % tokens.len();
            *counter += 1;
            idx
        };
        let token_id = tokens[token_idx].clone();

        // 解析价格/数量
        let mut prices_sizes: Vec<(f64, f64)> = Vec::new();
        for entry in entries {
            if let (Ok(price), Ok(size)) = (entry.price.parse::<f64>(), entry.size.parse::<f64>()) {
                if price > 0.0 && size > 0.0 {
                    prices_sizes.push((price, size));
                }
            }
        }

        if prices_sizes.len() >= 2 {
            // 按价格降序排序 - 安全处理 NaN
            prices_sizes.sort_by(|a, b| {
                match b.0.partial_cmp(&a.0) {
                    Some(ordering) => ordering,
                    None => {
                        // Handle NaN cases
                        if a.0.is_nan() && b.0.is_nan() {
                            Ordering::Equal
                        } else if a.0.is_nan() {
                            Ordering::Greater // NaN goes to the end
                        } else {
                            Ordering::Less
                        }
                    }
                }
            });

            // Best bid = 最高价格（买盘）
            let best_bid = prices_sizes[0].0;
            // Best ask = 最低价格（卖盘）
            let best_ask = prices_sizes[prices_sizes.len() - 1].0;

            // 更新缓存
            {
                let mut prices = last_prices.write().await;
                prices.insert(format!("{}_bid", token_id), best_bid);
                prices.insert(format!("{}_ask", token_id), best_ask);
            }

            // 每 5 秒打印一次
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            let should_display = {
                let mut last_display = last_display_time.write().await;
                if now - *last_display >= DISPLAY_INTERVAL {
                    *last_display = now;
                    true
                } else {
                    false
                }
            };

            if should_display {
                let labels = token_labels.read().await;
                let label = labels.get(&token_id).cloned().unwrap_or_else(|| {
                    token_id.chars().take(6).collect()
                });
                info!("{}: 买{:.4}/卖{:.4}", label, best_bid, best_ask);
            }
        }
    }

    /// 处理 book 事件 - 复刻 Python Dict 格式处理
    async fn process_book_event(
        event: BookEvent,
        last_prices: &Arc<RwLock<HashMap<String, f64>>>,
        last_display_time: &Arc<RwLock<f64>>,
        token_labels: &Arc<RwLock<HashMap<String, String>>>,
    ) {
        // Use FIRST 20 chars of token ID as key to match Gamma API behavior
        // Gamma API returns short IDs (20 chars), WebSocket returns long IDs (77+ chars)
        // The first 20 chars of WebSocket asset_id match the Gamma API token_id
        let full_token_id = event.asset_id;
        let short_token_id = if full_token_id.len() > 20 {
            full_token_id[..20].to_string()
        } else {
            full_token_id.clone()
        };

        // 解析 bids - best bid = 最高价格 = 第一个元素 (matches Python: parsed_bids[0])
        let best_bid = event.bids.first()
            .and_then(|b| b.get("price"))
            .and_then(|p| p.parse::<f64>().ok());

        // 解析 asks - best ask = 最低价格 = 第一个元素 (matches Python: parsed_asks[0])
        let best_ask = event.asks.first()
            .and_then(|a| a.get("price"))
            .and_then(|p| p.parse::<f64>().ok());
        
        info!("DEBUG price parsing: bids_count={}, asks_count={}, best_bid={:?}, best_ask={:?}", 
            event.bids.len(), event.asks.len(), best_bid, best_ask);
        if let Some(first_bid) = event.bids.first() {
            info!("DEBUG first bid: {:?}", first_bid);
        }
        if let Some(first_ask) = event.asks.first() {
            info!("DEBUG first ask: {:?}", first_ask);
        }

        // 更新缓存 - use short token ID as key
        {
            let mut prices = last_prices.write().await;
            if let Some(bid) = best_bid {
                let key = format!("{}_bid", short_token_id);
                info!("DEBUG storing: key={}, bid={}", key, bid);
                prices.insert(key, bid);
            }
            if let Some(ask) = best_ask {
                let key = format!("{}_ask", short_token_id);
                info!("DEBUG storing: key={}, ask={}", key, ask);
                prices.insert(key, ask);
            }
            info!("DEBUG total keys in cache: {}", prices.len());
        }

        // 每 5 秒打印一次
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let should_display = {
            let mut last_display = last_display_time.write().await;
            if now - *last_display >= DISPLAY_INTERVAL {
                *last_display = now;
                true
            } else {
                false
            }
        };

        if should_display && (best_bid.is_some() || best_ask.is_some()) {
            let labels = token_labels.read().await;
            let _label = labels.get(&short_token_id).cloned().unwrap_or_else(|| {
                short_token_id.chars().take(6).collect()
            });
            let _bid_str = best_bid.map(|b| format!("{:.4}", b)).unwrap_or_else(|| "--".to_string());
            let _ask_str = best_ask.map(|a| format!("{:.4}", a)).unwrap_or_else(|| "--".to_string());
            
            // Print all token prices, not just the current one
            let all_prices = last_prices.read().await;
            let mut price_msgs: Vec<String> = Vec::new();
            for (key, value) in all_prices.iter() {
                if key.ends_with("_bid") {
                    let token = &key[..key.len()-4];
                    let label_short = labels.get(token).cloned().unwrap_or_else(|| {
                        token.chars().take(4).collect()
                    });
                    let ask = all_prices.get(&format!("{}_ask", token)).copied().unwrap_or(0.0);
                    price_msgs.push(format!("{}:买{:.2}/卖{:.2}", label_short, *value, ask));
                }
            }
            if !price_msgs.is_empty() {
                info!("{}", price_msgs.join(" | "));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_websocket_new() {
        let ws = PolymarketWebSocket::new();
        assert_eq!(ws.get_stats().await, 0);
    }
}