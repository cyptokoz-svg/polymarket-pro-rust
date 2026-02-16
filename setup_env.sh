#!/bin/bash
# setup_env.sh - 从 Python 配置迁移到 Rust

set -e

echo "🔧 Setting up environment for Polymarket Pro Rust..."

# Python .env 文件路径
PYTHON_ENV="/root/.openclaw/workspace/poly-maker/.env"
RUST_ENV="/root/.openclaw/workspace/polymarket-pro-rust/.env"

# 检查 Python .env 是否存在
if [ ! -f "$PYTHON_ENV" ]; then
    echo "❌ Python .env not found at $PYTHON_ENV"
    echo "Please ensure Python bot is configured"
    exit 1
fi

echo "📄 Found Python .env file"

# 读取 Python 配置
read_var() {
    local file=$1
    local var=$2
    grep "^${var}=" "$file" 2>/dev/null | cut -d= -f2- | tr -d '"' || echo ""
}

PK=$(read_var "$PYTHON_ENV" "PK")
BROWSER_ADDRESS=$(read_var "$PYTHON_ENV" "BROWSER_ADDRESS")
BUILDER_KEY=$(read_var "$PYTHON_ENV" "POLY_BUILDER_API_KEY")
BUILDER_SECRET=$(read_var "$PYTHON_ENV" "POLY_BUILDER_API_SECRET")
BUILDER_PASS=$(read_var "$PYTHON_ENV" "POLY_BUILDER_API_PASSPHRASE")

# 验证必需变量
if [ -z "$PK" ]; then
    echo "❌ PK not found in Python .env"
    exit 1
fi

if [ -z "$BROWSER_ADDRESS" ]; then
    echo "❌ BROWSER_ADDRESS not found in Python .env"
    exit 1
fi

# 设置 Safe 地址 (如果没有则使用 BROWSER_ADDRESS)
SAFE_ADDRESS="${SAFE_ADDRESS:-$BROWSER_ADDRESS}"

echo ""
echo "✅ Configuration loaded:"
echo "  PK: ${PK:0:10}..."
echo "  BROWSER_ADDRESS: $BROWSER_ADDRESS"
echo "  SAFE_ADDRESS: $SAFE_ADDRESS"
if [ -n "$BUILDER_KEY" ]; then
    echo "  BUILDER_API: Configured"
else
    echo "  BUILDER_API: Not configured"
fi

# 创建 Rust .env 文件
cat > "$RUST_ENV" << EOF
# Polymarket Pro Rust Configuration
# Auto-generated from Python config

# Required
PK=$PK
BROWSER_ADDRESS=$BROWSER_ADDRESS
SAFE_ADDRESS=$SAFE_ADDRESS

# Builder API (optional)
POLY_BUILDER_API_KEY=${BUILDER_KEY:-}
POLY_BUILDER_API_SECRET=${BUILDER_SECRET:-}
POLY_BUILDER_API_PASSPHRASE=${BUILDER_PASS:-}

# Trading parameters (using defaults)
ORDER_SIZE=1.0
MAX_POSITION=5.0
MAX_TOTAL_POSITION=30.0
REFRESH_INTERVAL=45
EOF

echo ""
echo "✅ Rust .env file created at: $RUST_ENV"

# 设置文件权限 (只允许所有者读写)
chmod 600 "$RUST_ENV"
echo "🔒 File permissions set to 600 (owner only)"

echo ""
echo "🚀 Setup complete! You can now run:"
echo "  cd /root/.openclaw/workspace/polymarket-pro-rust"
echo "  source .env"
echo "  cargo run --release"
