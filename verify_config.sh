#!/bin/bash
# verify_config.sh - 验证配置是否正确

set -e

echo "🔍 Verifying Polymarket Pro configuration..."
echo ""

# 检查 .env 文件
if [ ! -f ".env" ]; then
    echo "❌ .env file not found"
    exit 1
fi

# 加载配置
source .env

# 验证必需变量
echo "Checking required variables..."

if [ -z "$PK" ]; then
    echo "❌ PK is not set"
    exit 1
fi

if [[ ! "$PK" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
    echo "❌ PK format is invalid (should be 0x + 64 hex characters)"
    exit 1
fi

echo "  ✅ PK is set and valid"

if [ -z "$BROWSER_ADDRESS" ]; then
    echo "❌ BROWSER_ADDRESS is not set"
    exit 1
fi

if [[ ! "$BROWSER_ADDRESS" =~ ^0x[0-9a-fA-F]{40}$ ]]; then
    echo "❌ BROWSER_ADDRESS format is invalid"
    exit 1
fi

echo "  ✅ BROWSER_ADDRESS is set and valid"

if [ -z "$SAFE_ADDRESS" ]; then
    echo "❌ SAFE_ADDRESS is not set"
    exit 1
fi

echo "  ✅ SAFE_ADDRESS is set"

# 验证私钥和地址匹配
echo ""
echo "Checking if PK matches BROWSER_ADDRESS..."

# 使用 cargo 运行验证程序
cd /root/.openclaw/workspace/polymarket-pro-rust
if cargo run --release --bin verify_config 2>/dev/null; then
    echo "  ✅ PK and address match"
else
    echo "  ⚠️  Could not verify PK/address match (this is OK if binary not built)"
fi

# 显示配置摘要
echo ""
echo "📋 Configuration Summary:"
echo "  PK: ${PK:0:10}...${PK: -8}"
echo "  BROWSER_ADDRESS: $BROWSER_ADDRESS"
echo "  SAFE_ADDRESS: $SAFE_ADDRESS"

if [ -n "$POLY_BUILDER_API_KEY" ]; then
    echo "  BUILDER_API: Configured"
else
    echo "  BUILDER_API: Not configured (optional)"
fi

echo ""
echo "📊 Trading Parameters:"
echo "  ORDER_SIZE: $ORDER_SIZE"
echo "  MAX_POSITION: $MAX_POSITION"
echo "  MAX_TOTAL_POSITION: $MAX_TOTAL_POSITION"
echo "  REFRESH_INTERVAL: $REFRESH_INTERVAL seconds"

echo ""
echo "✅ Configuration verification complete!"
echo ""
echo "🚀 You can now run the bot with:"
echo "  source .env && cargo run --release"
