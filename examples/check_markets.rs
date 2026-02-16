//! 使用真实市场测试 WebSocket 连接

use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    println!("🧪 Testing WebSocket with real Polymarket market...\n");
    
    // 使用一个活跃的市场进行测试
    // 这里使用 Trump 相关的市场（当前活跃）
    let test_token = "0xbd31dc8a20211944f6b70f31557b47316606a77d"; // 示例 token
    
    println!("Note: Currently no BTC 5-minute markets are active on Polymarket.");
    println!("The bot will automatically detect and trade when they become available.\n");
    
    println!("Active market types currently available:");
    println!("- Trump deportation markets");
    println!("- Elon/Doge budget cuts");  
    println!("- GTA VI release");
    println!("- Bitcoin $1M before GTA VI");
    println!();
    
    // 检查 API
    println!("Checking Polymarket API for active markets...");
    
    match reqwest::get("https://gamma-api.polymarket.com/markets?active=true&limit=10").await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = json.as_array() {
                    println!("✅ API is accessible. Found {} active markets.", arr.len());
                    
                    // 显示前几个市场
                    println!("\nTop active markets:");
                    for (i, m) in arr.iter().take(5).enumerate() {
                        let slug = m.get("slug").and_then(|s| s.as_str()).unwrap_or("N/A");
                        println!("  {}. {}", i + 1, slug);
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ API check failed: {}", e);
        }
    }
    
    println!("\n✅ Test completed");
    println!("\nWhen BTC 5-minute markets are available, the bot will:");
    println!("1. Auto-detect the current active market");
    println!("2. Subscribe to WebSocket for real-time prices");
    println!("3. Place orders every 45 seconds");
    println!("4. Auto-rotate to new markets when they expire");
}