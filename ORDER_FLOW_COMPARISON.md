# Rust vs Python 下单流程详细对比

## 分析时间
2026-02-16 20:39

## 1. 下单流程对比

### Python 版本 (market_maker_monitor.py)

```python
def _prepare_and_place_order(self, token, side, price, size, skip_balance_check=False):
    # 1. 模拟模式检查
    if not self.auto_trade:
        return "simulated"
    
    # 2. 风控检查（价格范围）
    if not self.strategy.is_price_in_safe_range(price):
        return None
    
    # 3. 余额检查（可选）
    if not skip_balance_check:
        balance = self._get_usdc_balance()
        need = size * price
        if balance < need:
            return None
    
    # 4. 查API获取现有订单
    open_orders = self._get_open_orders_from_api(token)
    
    # 5. 取消旧单
    if open_orders:
        self._cancel_orders_for_token(token)
    
    # 6. 强制取消跟踪的订单（防止单腿）
    self._cancel_all_tracked_for_token(token)
    
    # 7. API限流保护
    self._rate_limit()
    
    # 8. 下新单
    order = self.client.create_order(token_id=token, side=side, size=size, price=price)
    
    # 9. 验证订单ID
    if not self._is_valid_order_id(order_id):
        return None
    
    # 10. 检查状态
    if status in ['live', 'OPEN', 'PENDING', 'matched']:
        return order_id
```

### Rust 版本 (executor.rs)

```rust
pub async fn place_order_with_validation(
    &self,
    token_id: &str,
    side: Side,
    price: f64,
    size: f64,
    safe_low: f64,
    safe_high: f64,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    // 1. 价格检查
    if !self.is_price_in_safe_range(price, safe_low, safe_high) {
        return Ok(None);
    }
    
    // 2. 余额检查
    let balance = self.get_usdc_balance().await?;
    let need = size * price;
    if balance < need {
        return Ok(None);
    }
    
    // 3. 下单（没有查API和取消旧单！）
    let result = self.place_limit_order(token_id, side, price, size).await?;
    
    // 4. 提取订单ID
    let order_id = result.get("orderId")...;
    
    // 5. 验证订单ID
    if !self.is_valid_order_id(&order_id) {
        return Ok(None);
    }
    
    // 6. 检查状态
    if self.is_order_successful(status, has_error) {
        return Ok(Some(order_id));
    }
}
```

## 2. 关键差异

### 🔴 严重差异

| 步骤 | Python | Rust | 影响 |
|------|--------|------|------|
| **查API获取现有订单** | ✅ `_get_open_orders_from_api()` | ❌ **缺失** | 不知道是否有旧单 |
| **取消旧单** | ✅ `_cancel_orders_for_token()` | ❌ **在 main.rs 中** | 分离的逻辑 |
| **强制取消跟踪订单** | ✅ `_cancel_all_tracked_for_token()` | ❌ **缺失** | 可能单腿累积 |
| **API限流** | ✅ `_rate_limit()` | ❌ **缺失** | 可能触发限流 |
| **模拟模式** | ✅ `if not self.auto_trade` | ❌ **缺失** | 无法模拟测试 |

### 🟡 实现差异

| 功能 | Python | Rust | 说明 |
|------|--------|------|------|
| 余额检查 | 可选 (`skip_balance_check`) | 强制 | Python 更灵活 |
| 订单类型 | FOK 默认 | GTC/FOK/FAK 可选 | Rust 更灵活 |
| 重试机制 | 在 client 层 | `retry_with_backoff` | 都有重试 |
| 错误处理 | 返回 None | 返回 Result | Rust 更严格 |

## 3. 流程图对比

### Python 完整流程
```
开始
  ↓
模拟模式检查 ──是──→ 返回模拟ID
  ↓否
价格检查 ──失败──→ 返回 None
  ↓通过
余额检查(可选) ──失败──→ 返回 None
  ↓通过
查API获取现有订单
  ↓
有旧单? ──是──→ 取消旧单
  ↓
强制取消跟踪订单
  ↓
API限流保护
  ↓
下新单
  ↓
验证订单ID ──失败──→ 返回 None
  ↓通过
检查状态 ──失败──→ 返回 None
  ↓成功
返回订单ID
```

### Rust 当前流程
```
开始
  ↓
价格检查 ──失败──→ 返回 None
  ↓通过
余额检查(强制)
  ↓
下新单 (没有查API和取消旧单！)
  ↓
验证订单ID ──失败──→ 返回 None
  ↓通过
检查状态 ──失败──→ 返回 None
  ↓成功
返回订单ID
```

## 4. 风险评估

### 🔴 高风险

**1. 缺少查API和取消旧单**
- Python: 在 `_prepare_and_place_order` 内部完成
- Rust: 在 `main.rs` 的 `run_trading_cycle_single_market` 中完成
- **风险**: 如果调用者忘记取消，会重复下单

**2. 缺少强制取消跟踪订单**
- Python: `_cancel_all_tracked_for_token(token)` 防止单腿
- Rust: **缺失**
- **风险**: 单腿订单累积

**3. 缺少API限流**
- Python: `_rate_limit()` 保护
- Rust: **缺失**
- **风险**: 触发 API 限流

**4. 缺少模拟模式**
- Python: `if not self.auto_trade` 支持模拟
- Rust: **缺失**
- **风险**: 无法安全测试

## 5. 建议修复

### 立即修复

**1. 将取消逻辑移到 executor 内部**
```rust
pub async fn place_order_with_validation(
    &self,
    token_id: &str,
    side: Side,
    price: f64,
    size: f64,
    safe_low: f64,
    safe_high: f64,
    order_tracker: &mut OrderTracker,  // 添加跟踪器
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    // 1. 查API获取现有订单
    let open_orders = self.get_open_orders(token_id).await?;
    
    // 2. 取消旧单
    if !open_orders.is_empty() {
        self.cancel_orders_for_market(token_id).await?;
    }
    
    // 3. 强制取消跟踪订单
    order_tracker.clear_orders_for_token(token_id);
    
    // 4. API限流
    self.rate_limit().await;
    
    // 5. 下新单
    // ...
}
```

**2. 添加模拟模式支持**
```rust
pub struct TradeExecutor {
    clob: ClobClient,
    signer: PrivateKeySigner,
    simulation_mode: bool,  // 添加模拟模式
}
```

**3. 添加 API 限流**
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct RateLimiter {
    last_request: AtomicU64,
    min_interval_ms: u64,
}
```

## 6. 结论

| 维度 | Python | Rust | 差距 |
|------|--------|------|------|
| 功能完整性 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | 缺少关键步骤 |
| 安全性 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | 可能重复下单 |
| 可测试性 | ⭐⭐⭐⭐⭐ | ⭐⭐ | 缺少模拟模式 |

**当前状态**: Rust 版本的 `executor.rs` 下单流程**不完整**，缺少查API、强制取消、限流等关键步骤。虽然 `main.rs` 中有部分逻辑，但分散的实现增加了出错风险。

**建议**: 将完整的下单流程（查API-取消-下单）封装在 `executor.rs` 内部，与 Python 保持一致。
