# Python vs Rust 最终差异复查清单

## 复查日期: 2026-02-16

---

## 🔴 关键差异（需要修复）

### 1. 订单类型支持
**Python (clob_client.py):**
```python
def create_order(self, token_id, side, price, size, order_type="GTC"):
    # Supports: 'GTC', 'FOK', 'FAK'
    result = self._bot_client.create_order(
        marketId=token_id,
        action=side,
        price=price,
        size=size,
        order_type=order_type  # <-- 支持多种订单类型
    )
```

**Rust (executor.rs):**
```rust
// Only uses OrderType::Gtc
let result = self.clob.create_and_post_limit_order(
    &order,
    None,
    OrderType::Gtc,  // <-- 硬编码GTC
).await?;
```

**差异:** Python支持GTC/FOK/FAK，Rust只支持GTC

---

### 2. 订单簿获取实现
**Python (clob_client.py):**
```python
def get_orderbook(self, token_id: str) -> Dict:
    bids_df, asks_df = self._bot_client.get_order_book(token_id)
    return {
        "bids": bids_df.to_dict('records'),
        "asks": asks_df.to_dict('records'),
    }
```

**Rust (main.rs):**
```rust
async fn get_order_book(_market: &Market) -> Option<(Vec<Value>, Vec<Value>)> {
    // TODO: Implement order book fetching
    None  // <-- 返回None，使用简单价格计算
}
```

**差异:** Python有完整实现，Rust是占位符

---

### 3. 市场ID vs Token ID
**Python (market_maker_monitor.py):**
```python
# Uses condition_id for market, token_id for orders
self._token_up = market.get("tokens", {}).get("UP", {}).get("token_id")
self._token_down = market.get("tokens", {}).get("DOWN", {}).get("token_id")
```

**Rust (main.rs):**
```rust
// Uses condition_id as token_id directly
let token_id = match &market.condition_id {
    Some(id) => id.clone(),
    None => continue,
};
```

**差异:** Python区分market ID和token ID，Rust混用

---

### 4. 结果格式标准化
**Python (clob_client.py):**
```python
# Normalizes result format
return {
    "orderID": result.get("orderID") or result.get("id"),
    "status": result.get("status", "UNKNOWN"),
    "transactionHash": result.get("transactionHash") or result.get("hash"),
    "original_response": result
}
```

**Rust (executor.rs):**
```rust
// Returns raw result without normalization
Ok(result)
```

**差异:** Python标准化返回格式，Rust返回原始结果

---

### 5. 钱包认证检查
**Python (clob_client.py):**
```python
def place_order(self, ...):
    if not self.wallet.is_authenticated():
        raise PermissionError("Wallet not authenticated for trading")
```

**Rust (executor.rs):**
```rust
// No explicit authentication check in place_order
pub async fn place_limit_order(...) -> Result<...> {
    // Directly places order without checking auth
}
```

**差异:** Python显式检查认证，Rust依赖底层库

---

### 6. Builder API 配置检查
**Python (clob_client.py):**
```python
BUILDER_ENV = ["POLY_BUILDER_API_KEY", "POLY_BUILDER_API_SECRET", "POLY_BUILDER_API_PASSPHRASE"]

def _init_bot_client(self):
    builder_vars = [var for var in self.BUILDER_ENV if os.getenv(var)]
    if len(builder_vars) == 3:
        self.builder_enabled = True
    elif builder_vars:
        print(f"⚠️ Builder API partially configured: {builder_vars}")
```

**Rust:**
```rust
// No explicit Builder API configuration check
// Just passes credentials to ClobClient::new
```

**差异:** Python检查Builder API配置完整性并警告

---

### 7. 环境变量验证
**Python (clob_client.py):**
```python
REQUIRED_ENV = ["PK", "BROWSER_ADDRESS"]

def _validate_env(self):
    missing = [var for var in self.REQUIRED_ENV if not os.getenv(var)]
    if missing:
        raise RuntimeError(f"Missing required env vars: {missing}")
```

**Rust (config/mod.rs):**
```rust
// Validates pk and safe_address are not empty
if self.pk.is_empty() { bail!("Private key is required"); }
if self.safe_address.is_empty() { bail!("Safe address is required"); }
// No check for BROWSER_ADDRESS
```

**差异:** Python检查BROWSER_ADDRESS，Rust没有

---

### 8. 错误处理粒度
**Python (clob_client.py):**
```python
try:
    result = self._bot_client.create_order(...)
except RateLimitError as e:
    raise RuntimeError(f"Rate limited: {e}")
except InsufficientBalanceError as e:
    raise RuntimeError(f"Insufficient balance: {e}")
except Exception as e:
    raise RuntimeError(f"Failed to place order: {e}")
```

**Rust:**
```rust
// Generic error handling
let result = retry_with_backoff(...).await?;
```

**差异:** Python有细粒度错误分类，Rust统一处理

---

### 9. 市场信息获取
**Python (market_maker_monitor.py):**
```python
def _get_full_market(self, condition_id: str) -> Dict:
    """获取市场完整信息包括tokens"""
    market = self.client.get_full_market(condition_id)
    return market
```

**Rust:**
```rust
// No equivalent function
// Uses market list from get_markets() directly
```

**差异:** Python可以获取单个市场完整信息

---

### 10. Token 解析
**Python:**
```python
# Gets specific token IDs for UP/DOWN outcomes
token_up = market["tokens"]["UP"]["token_id"]
token_down = market["tokens"]["DOWN"]["token_id"]
```

**Rust:**
```rust
// Uses condition_id directly as token
let token_id = market.condition_id.clone();
```

**差异:** Python解析市场token，Rust直接使用condition_id

---

## 🟡 次要差异（可接受）

| # | 功能 | Python | Rust | 说明 |
|---|------|--------|------|------|
| 11 | 日志格式 | 自定义格式 | tracing默认 | 非关键 |
| 12 | 配置热重载 | 支持 | 不支持 | 启动时加载 |
| 13 | 信号处理 | SIGTERM/SIGINT | 仅SIGINT | 覆盖主要场景 |
| 14 | 测试覆盖 | 单元+集成 | 主要功能 | 核心逻辑测试 |
| 15 | 文档字符串 | 详细 | 简洁 | 不影响功能 |

---

## ✅ 已确认一致

| # | 功能 | 验证状态 |
|---|------|---------|
| 1 | 配置参数（21个） | ✅ |
| 2 | 交易循环逻辑 | ✅ |
| 3 | 风险控制机制 | ✅ |
| 4 | 库存管理策略 | ✅ |
| 5 | 订单生命周期 | ✅ |
| 6 | API限流保护 | ✅ |
| 7 | 统计信息跟踪 | ✅ |
| 8 | 交易历史记录 | ✅ |
| 9 | 后台任务处理 | ✅ |
| 10 | 错误重试机制 | ✅ |

---

## 📊 差异统计

```
关键差异（需修复）: 10项 🔴
次要差异（可接受）:  5项 🟡
已确认一致:         10项 ✅

总计: 25项对比
```

---

## 🎯 关键问题总结

### 最高优先级
1. **Token ID处理** - Python区分market ID和token ID，Rust混用可能导致下单错误
2. **订单簿获取** - Rust返回None，无法使用订单簿深度分析
3. **订单类型** - Rust只支持GTC，Python支持FOK/FAK

### 中优先级
4. **结果标准化** - Rust返回原始结果，没有统一格式
5. **环境变量验证** - Rust缺少BROWSER_ADDRESS检查
6. **Builder API检查** - Rust没有配置完整性警告

### 低优先级
7-10. 错误处理、认证检查、市场信息获取等