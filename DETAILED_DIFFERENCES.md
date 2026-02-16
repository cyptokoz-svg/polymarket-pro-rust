# Python vs Rust 详细差异分析报告

## 检查时间
2026-02-16 20:32

## 详细对比结果

### ✅ 完全一致的部分

| 功能 | Python | Rust | 状态 |
|------|--------|------|------|
| 订单簿深度分析 | `analyze_orderbook_depth()` | `analyze_order_book_depth_safe()` | ✅ 逻辑相同 |
| 价格计算 | `calculate_mm_prices()` | `calculate_mm_prices()` | ✅ 公式相同 |
| 库存偏离计算 | `calculate_inventory_skew()` | `calculate_inventory_skew()` | ✅ 相同 |
| should_skip_side | skew > 0.7 / skew < -0.7 | skew > 0.7 / skew < -0.7 | ✅ 阈值相同 |
| get_position_limit | base_limit * (1 ± skew) | base_limit * (1 ± skew) | ✅ 公式相同 |
| 止盈止损 | `should_exit_position()` | `ExitManager::check_exit()` | ✅ 4种情况一致 |

### 🔴 发现的差异

#### 1. 订单取消逻辑 - 重要差异

**Python** (`safe_create_order`):
```python
def safe_create_order(self, token, side, price, size, cancel_callback=None, create_callback=None):
    # 第1步：准备（取消旧单）- 必须成功才能继续
    if not self.prepare_order(token, cancel_callback):
        logger.error(f"❌ Order preparation failed...")
        return None  # 取消失败，直接返回
    
    # 第2步：创建新单
    result = create_callback(token, side, price, size)
```

**Rust** (`run_trading_cycle_single_market`):
```rust
// 取消订单 - 失败只记录警告，继续执行
if let Err(e) = executor.cancel_orders_for_market(&token_id).await {
    warn!("Failed to cancel orders for {}: {}", token_id, e);  // 只警告，不返回
}

// 继续下单...
```

**差异**: Python 取消失败会停止下单，Rust 会继续下单！

#### 2. 订单簿排序 - 潜在差异

**Python**:
```python
# bids 已经是降序，asks 已经是升序（来自API）
parsed_bids = []
for b in bids[:self.config.depth_lookback]:  # 直接取前N个
```

**Rust**:
```rust
// 显式排序
parsed_bids.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap_or(...));  // 降序
parsed_asks.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap_or(...));  // 升序
```

**差异**: Rust 显式排序，Python 依赖 API 返回顺序

#### 3. 并发下单错误处理

**Python**:
```python
# 使用回调，错误在回调内部处理
result = create_callback(token, side, price, size)
if result:  # 检查返回值
    self.trade_count += 1
```

**Rust**:
```rust
// 使用 tokio::join，错误单独处理
let (buy_result, sell_result) = tokio::join!(buy_task, sell_task);
if let Err(e) = buy_result {
    error!("Buy order task failed: {}", e);
}
```

**差异**: 错误处理方式不同，但逻辑相似

#### 4. 价格警告冷却

**Python**:
```python
def _should_log_price_warning(self, price: float, side: str) -> bool:
    key = f"{side}_{price:.2f}"
    now = time.time()
    cooldown = self.config.price_warn_cooldown
    
    last_warn = self._last_price_warnings.get(key, 0)
    if now - last_warn > cooldown:
        self._last_price_warnings[key] = now
        return True
    return False
```

**Rust**:
```rust
// PriceWarningTracker 实现了类似的冷却逻辑
pub fn log_price_warning(&mut self, price: f64, side: &str, ...) {
    // 有冷却机制
}
```

**状态**: ✅ 都有冷却机制

#### 5. 批量取消 vs 单个取消

**Python**:
```python
def batch_cancel_and_create(self, orders: List[Dict], ...):
    # 第2步：批量取消这些token的所有订单
    for token in tokens:
        cancel_all_callback(token)
    
    # 等待取消生效
    time.sleep(0.1)
    
    # 第3步：批量创建新订单
```

**Rust**:
```rust
// 只取消一个 token 的订单
if let Err(e) = executor.cancel_orders_for_market(&token_id).await {
    warn!("Failed to cancel orders...");
}

// 没有批量操作
```

**差异**: Python 支持批量操作，Rust 是单个操作

### 🟡 实现方式不同但逻辑相同

| 功能 | Python | Rust | 说明 |
|------|--------|------|------|
| 持仓存储 | Dict[str, Position] | HashMap<String, Position> | 相同 |
| 订单跟踪 | 内存 + 可选持久化 | OrderTracker | Rust 更完善 |
| 统计 | 简单计数 | TradingStats | Rust 更详细 |
| 配置 | dataclass | Struct | 相同 |

### 🔴 严重缺失

#### 1. 自动赎回 - Python 有，Rust 无
**Python**: `auto_trader.py` 自动检查结算并赎回
**Rust**: 赎回模块独立，未集成到主循环

#### 2. 订单类型支持
**Python**: 支持 GTC/FOK/FAK
**Rust**: 支持 GTC/FOK/FAK ✅ (已实现)

#### 3. Builder API 优先级执行
**Python**: 自动检测并使用
**Rust**: 支持但需配置

## 关键风险点

### 🔴 高风险
1. **取消失败继续下单**: Rust 取消失败只警告，会继续下单，可能导致重复订单
2. **自动赎回缺失**: 需要手动赎回，可能错过最佳赎回时机

### 🟡 中风险
1. **订单簿排序**: 如果 API 返回顺序不一致，可能影响价格计算
2. **错误处理**: Rust 的错误处理更分散，可能遗漏某些错误

## 建议修复

### 立即修复
1. **修复取消逻辑**: 取消失败应该停止下单或重试
2. **集成自动赎回**: 添加定时检查并赎回功能

### 后续优化
1. **添加批量操作**: 支持批量取消和创建订单
2. **完善测试**: 对比测试 Python 和 Rust 的下单行为

## 最终结论

| 维度 | 评分 | 说明 |
|------|------|------|
| 核心逻辑一致性 | 85% | 大部分逻辑一致，但取消处理有差异 |
| 功能完整性 | 70% | 缺少自动赎回 |
| 风险等级 | 🟡 中 | 取消逻辑差异需要修复 |

**建议**: 修复取消逻辑和集成自动赎回后，Rust 版本可以投入使用。
