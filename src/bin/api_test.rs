//! Direct API test for debugging

use reqwest;
use serde_json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test 1: Get server time
    let resp = reqwest::get("https://clob.polymarket.com/time").await?;
    let text = resp.text().await?;
    println!("Server time response: {}", text);
    
    // Test 2: Get markets
    let resp = reqwest::get("https://clob.polymarket.com/markets").await?;
    let text = resp.text().await?;
    println!("Markets response (first 200 chars): {}", &text[..200.min(text.len())]);
    
    Ok(())
}
