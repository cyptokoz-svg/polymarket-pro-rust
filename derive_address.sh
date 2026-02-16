#!/bin/bash
# derive_address.sh - 从私钥推导 EOA 地址
# ⚠️ 安全提醒: 从环境变量读取私钥，不要硬编码

echo "🔑 Deriving EOA address from private key..."
echo ""

# 从环境变量读取私钥
PK="${PK:-}"

if [ -z "$PK" ]; then
    echo "❌ Error: PK environment variable not set"
    echo "Usage: PK=0x... ./derive_address.sh"
    exit 1
fi

echo "Private Key: ${PK:0:6}...${PK: -4}"
echo ""

# 使用 Python 计算（如果可用）
if python3 -c "import eth_account" 2>/dev/null; then
    echo "Using Python eth_account..."
    python3 << EOF
from eth_account import Account
pk = "$PK"
account = Account.from_key(pk)
print(f"EOA Address: {account.address}")
EOF
else
    echo "Python eth_account not available"
    echo ""
    echo "Alternative methods:"
    echo "1. Use online tool: https://www.privatekeyfinder.io/"
    echo "2. Import into MetaMask"
    echo "3. Use MyEtherWallet"
fi

echo ""
echo "Safe Address: ${SAFE_ADDRESS:-Not set}"
