#!/bin/bash
# test_derive.sh - 测试 API Key 自动推导

cd /root/.openclaw/workspace/polymarket-pro-rust

echo "🧪 Testing API key derivation from private key..."
echo ""

# 只设置基本环境变量，不设置 Builder API
export PK="***REMOVED***"
export BROWSER_ADDRESS="0xb18ec66081b444037f7c1b5ffee228693b854e7a"
export SAFE_ADDRESS="0x45dceb24119296fb57d06d83c1759cc191c3c96e"

# 不设置 Builder API，强制使用 derive
# export POLY_BUILDER_API_KEY=""
# export POLY_BUILDER_API_SECRET=""
# export POLY_BUILDER_API_PASSPHRASE=""

export ORDER_SIZE="1.0"
export MAX_POSITION="6.0"
export MAX_TOTAL_POSITION="36.0"
export REFRESH_INTERVAL="45"
export SIMULATION_MODE="true"

# 运行测试
./target/release/polymarket-pro 2>&1 | grep -E "Deriving|API|credentials|TradeExecutor" | head -20
