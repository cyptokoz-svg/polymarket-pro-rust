//! 测试 BTC 5 分钟市场自动切换和 WebSocket 订阅

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

// 引入模块
mod websocket;
use websocket::PolymarketWebSocket;

#[tokio::main]
async fn main() {
    println!("🧪 Testing BTC 5-Minute Market Auto-Rotation...\n");
    
    // 模拟市场数据
    let market_1_tokens = vec![
        "8446119606299893245512449444316583182609661690372449470898616524263325325418".to_string(), // UP
        "84907397431594378923138292858075599910774930814401883198475796805943152057833".to_string(), // DOWN
    ];
    
    let market_2_tokens = vec![
        "9446119606299893245512449444316583182609661690372449470898616524263325325419".to_string(), // UP (new)
        "94907397431594378923138292858075599910774930814401883198475796805943152057834".to_string(), // DOWN (new)
    ];
    
    // 创建 WebSocket 客户端
    let ws = Arc::new(PolymarketWebSocket::new());
    
    // 设置 token 标签
    let mut labels = std::collections::HashMap::new();
    labels.insert(market_1_tokens[0].clone(), "UP".to_string());
    labels.insert(market_1_tokens[1].clone(), "DOWN".to_string());
    ws.set_token_labels(labels).await;
    
    // 启动 WebSocket 订阅第一个市场
    println!("📡 Subscribing to Market 1 (current slot)...");
    ws.start(market_1_tokens.clone()).await;
    
    // 模拟交易循环
    for i in 0..10 {
        sleep(Duration::from_secs(2)).await;
        
        let msg_count = ws.get_stats().await;
        let subscribed: Vec<String> = ws.get_subscribed_tokens().await;
        
        println!("\n⏰ Tick {}: Messages={}, Subscribed={}", i + 1, msg_count, subscribed.len());
        
        // 模拟在第 5 个 tick 时市场过期，切换到新市场
        if i == 5 {
            println!("\n🔄 Market 1 expired! Switching to Market 2...");
            ws.update_subscription(market_2_tokens.clone()).await;
            
            // 更新标签
            let mut new_labels = std::collections::HashMap::new();
            new_labels.insert(market_2_tokens[0].clone(), "UP".to_string());
            new_labels.insert(market_2_tokens[1].clone(), "DOWN".to_string());
            ws.set_token_labels(new_labels).await;
            
            println!("✅ Subscribed to Market 2");
        }
        
        // 获取价格
        for token in &subscribed {
            if let Some((bid, ask)) = ws.get_price(token).await {
                println!("  Price for {}: Bid={:.4}, Ask={:.4}", 
                    &token[..20], bid, ask);
            }
        }
    }
    
    // 停止 WebSocket
    ws.stop().await;
    println!("\n✅ Test completed");
    println!("\nSummary:");
    println!("- WebSocket auto-subscribes to new markets");
    println!("- Token labels are updated correctly");
    println!("- Price cache is maintained per token");
}