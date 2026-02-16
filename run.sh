#!/bin/bash
# run.sh - 后台运行 Polymarket Pro

cd /root/.openclaw/workspace/polymarket-pro-rust

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

# 创建日志目录
mkdir -p ~/.polymarket-pro

# 后台运行
nohup ./target/release/polymarket-pro > ~/.polymarket-pro/trading.log 2>&1 &
echo $! > ~/.polymarket-pro/pid

echo "🚀 Polymarket Pro started in background"
echo "📊 Configuration: Order=1.0, MaxPos=6.0, MaxTotal=36.0"
echo "📝 Logs: ~/.polymarket-pro/trading.log"
echo "🔍 View logs: tail -f ~/.polymarket-pro/trading.log"
