#!/bin/bash
# start.sh - 启动 Polymarket Pro 交易机器人

set -e

cd /root/.openclaw/workspace/polymarket-pro-rust

echo "🚀 Starting Polymarket Pro..."

# 加载环境变量
export PK="***REMOVED***"
export BROWSER_ADDRESS="0xb18ec66081b444037f7c1b5ffee228693b854e7a"
export SAFE_ADDRESS="0x45dceb24119296fb57d06d83c1759cc191c3c96e"
export POLY_BUILDER_API_KEY="019c66b3-05bf-7987-85e3-7f11dce4be4b"
export POLY_BUILDER_API_SECRET="8SK8Q8ZtV00fR6P5N9chTbU1slGjSaA0wtrWgQBCpoY="
export POLY_BUILDER_API_PASSPHRASE="ad439f8b134a22af52a1e2b04162fa5819aacd8af70c4f153a56a4b3866d28fb"

# 交易参数
export ORDER_SIZE="1.0"
export MAX_POSITION="6.0"
export MAX_TOTAL_POSITION="36.0"
export REFRESH_INTERVAL="45"

echo "📊 Configuration:"
echo "  Order Size: $ORDER_SIZE"
echo "  Max Position: $MAX_POSITION"
echo "  Max Total Position: $MAX_TOTAL_POSITION"
echo ""

# 检查是否已有实例在运行
if pgrep -f "polymarket-pro" > /dev/null; then
    echo "⚠️  Polymarket Pro is already running!"
    echo "   Use ./stop.sh to stop it first."
    exit 1
fi

# 启动机器人
./target/release/polymarket-pro
