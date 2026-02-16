# Python vs Rust 配置对比

## 配置来源

### Python 配置
- **来源**: 环境变量 (`.env` 文件)
- **必需变量**:
  - `PK` - 私钥
  - `BROWSER_ADDRESS` - 钱包地址
- **可选变量** (Builder API):
  - `POLY_BUILDER_API_KEY`
  - `POLY_BUILDER_API_SECRET`
  - `POLY_BUILDER_API_PASSPHRASE`

### Rust 配置
- **来源**: 环境变量 + 配置文件
- **必需变量**:
  - `PK` - 私钥
  - `SAFE_ADDRESS` - 钱包地址
  - `BROWSER_ADDRESS` - 验证用
- **可选变量** (Builder API):
  - `POLY_BUILDER_API_KEY`
  - `POLY_BUILDER_API_SECRET`
  - `POLY_BUILDER_API_PASSPHRASE`

---

## 环境变量对照表

| 变量名 | Python | Rust | 说明 |
|--------|--------|------|------|
| `PK` | ✅ | ✅ | 私钥 (必需) |
| `BROWSER_ADDRESS` | ✅ | ✅ | 钱包地址 (必需) |
| `SAFE_ADDRESS` | ❌ | ✅ | Safe 地址 (Rust 需要) |
| `POLY_BUILDER_API_KEY` | ✅ | ✅ | Builder API Key |
| `POLY_BUILDER_API_SECRET` | ✅ | ✅ | Builder API Secret |
| `POLY_BUILDER_API_PASSPHRASE` | ✅ | ✅ | Builder API Passphrase |

---

## 交易参数对照

| 参数 | Python 默认值 | Rust 默认值 | 说明 |
|------|--------------|-------------|------|
| `ORDER_SIZE` | 1.0 | 1.0 | 每单大小 |
| `MAX_POSITION` | 5.0 | 5.0 | 最大持仓 |
| `MAX_TOTAL_POSITION` | 30.0 | 30.0 | 总持仓限制 |
| `MAX_SPREAD` | 0.02 | 0.02 | 最大价差 |
| `MIN_SPREAD` | 0.005 | 0.005 | 最小价差 |
| `MERGE_THRESHOLD` | 0.5 | 0.5 | 合并阈值 |
| `MAX_HOLD_TIME` | 180 | 180 | 最大持仓时间(秒) |
| `EXIT_BEFORE_EXPIRY` | 120 | 120 | 到期前退出(秒) |
| `TAKE_PROFIT` | 0.03 | 0.03 | 止盈比例 |
| `STOP_LOSS` | 0.05 | 0.05 | 止损比例 |
| `DEPTH_LOOKBACK` | 5 | 5 | 订单簿深度 |
| `IMBALANCE_THRESHOLD` | 0.3 | 0.3 | 不平衡阈值 |
| `SAFE_RANGE_LOW` | 0.01 | 0.01 | 安全范围下限 |
| `SAFE_RANGE_HIGH` | 0.99 | 0.99 | 安全范围上限 |
| `REFRESH_INTERVAL` | 45 | 45 | 刷新间隔(秒) |

---

## 配置迁移指南

### 步骤 1: 从 Python 复制必需变量

```bash
# 查看 Python 的 .env 文件
cat /root/.openclaw/workspace/poly-maker/.env

# 输出示例:
# PK=0x...
# BROWSER_ADDRESS=0x...
# POLY_BUILDER_API_KEY=...
# POLY_BUILDER_API_SECRET=...
# POLY_BUILDER_API_PASSPHRASE=...
```

### 步骤 2: 设置 Rust 环境变量

```bash
# 方法 1: 直接设置
export PK="0x..."
export BROWSER_ADDRESS="0x..."
export SAFE_ADDRESS="0x..."  # 如果没有 Safe，用 BROWSER_ADDRESS
export POLY_BUILDER_API_KEY="..."
export POLY_BUILDER_API_SECRET="..."
export POLY_BUILDER_API_PASSPHRASE="..."

# 方法 2: 创建 .env 文件
cat > /root/.openclaw/workspace/polymarket-pro-rust/.env << 'EOF'
PK=0x...
BROWSER_ADDRESS=0x...
SAFE_ADDRESS=0x...
POLY_BUILDER_API_KEY=...
POLY_BUILDER_API_SECRET=...
POLY_BUILDER_API_PASSPHRASE=...
EOF
```

### 步骤 3: 验证配置

```bash
cd /root/.openclaw/workspace/polymarket-pro-rust
source .env  # 如果使用 .env 文件
cargo run -- --check-config
```

---

## 配置文件 (可选)

Rust 还支持配置文件:

```toml
# polymarket-pro.toml
pk = "0x..."
safe_address = "0x..."
browser_address = "0x..."

[api]
key = "..."
secret = "..."
passphrase = "..."

[trading]
order_size = 1.0
max_position = 5.0
max_total_position = 30.0
# ... 其他参数
```

---

## 安全注意事项

### Python
- `.env` 文件在 `poly-maker/` 目录
- 需要确保文件权限安全

### Rust
- 支持 `.env` 文件或环境变量
- 配置文件保存时会**自动脱敏** (移除私钥)
- 建议: 使用环境变量而非配置文件存储私钥

---

## 快速启动脚本

```bash
#!/bin/bash
# setup_env.sh

# 从 Python 复制配置
export PK="${PK:-$(grep PK /root/.openclaw/workspace/poly-maker/.env 2>/dev/null | cut -d= -f2)}"
export BROWSER_ADDRESS="${BROWSER_ADDRESS:-$(grep BROWSER_ADDRESS /root/.openclaw/workspace/poly-maker/.env 2>/dev/null | cut -d= -f2)}"
export SAFE_ADDRESS="${SAFE_ADDRESS:-$BROWSER_ADDRESS}"
export POLY_BUILDER_API_KEY="${POLY_BUILDER_API_KEY:-$(grep POLY_BUILDER_API_KEY /root/.openclaw/workspace/poly-maker/.env 2>/dev/null | cut -d= -f2)}"
export POLY_BUILDER_API_SECRET="${POLY_BUILDER_API_SECRET:-$(grep POLY_BUILDER_API_SECRET /root/.openclaw/workspace/poly-maker/.env 2>/dev/null | cut -d= -f2)}"
export POLY_BUILDER_API_PASSPHRASE="${POLY_BUILDER_API_PASSPHRASE:-$(grep POLY_BUILDER_API_PASSPHRASE /root/.openclaw/workspace/poly-maker/.env 2>/dev/null | cut -d= -f2)}"

echo "✅ Environment configured"
echo "PK: ${PK:0:10}..."
echo "BROWSER_ADDRESS: $BROWSER_ADDRESS"
echo "SAFE_ADDRESS: $SAFE_ADDRESS"
```
