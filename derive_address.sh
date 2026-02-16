#!/bin/bash
# derive_address.sh - 从私钥推导 EOA 地址

echo "🔑 Deriving EOA address from private key..."
echo ""

# 私钥
PK="***REMOVED***"

echo "Private Key: 0x$PK"
echo ""

# 使用 Python 计算（如果可用）
if python3 -c "import eth_account" 2>/dev/null; then
    echo "Using Python eth_account..."
    python3 << EOF
from eth_account import Account
pk = "0x$PK"
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
    echo ""
    echo "Your private key: 0x$PK"
fi

echo ""
echo "Safe Address: 0x45dceb24119296fb57d06d83c1759cc191c3c96e"
