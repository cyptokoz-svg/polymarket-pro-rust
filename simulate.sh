#!/bin/bash
# simulate.sh - 模拟模式运行（不实际下单）

cd /root/.openclaw/workspace/polymarket-pro-rust

echo "🎮 Starting Polymarket Pro in SIMULATION MODE..."
echo "📊 No real orders will be placed"
echo ""

# 加载环境变量
export PK="***REMOVED***"
export BROWSER_ADDRESS="0xb18ec66081b444037f7c1b5ffee228693b854e7a"
export SAFE_ADDRESS="0x45dceb24119296fb57d06d83c1759cc191c3c96e"
export POLY_BUILDER_API_KEY="019c66b3-05bf-7987-85e3-7f11dce4be4b"
export POLY_BUILDER_API_SECRET="8SK8Q8ZtV00fR6P5N9chTbU1slGjSaA0wtrWgQBCpoY="
export POLY_BUILDER_API_PASSPHRASE="ad439f8b134a22af52a1e2b04162fa5819aacd8af70c4f153a56a4b3866d28fb"
export ORDER_SIZE="1.0"
export MAX_POSITION="6.0"
export MAX_TOTAL_POSITION="36.0"
export REFRESH_INTERVAL="45"

# 模拟模式标志
export SIMULATION_MODE="true"

# 运行（前台运行，方便查看日志）
./target/release/polymarket-pro
