# Python vs Rust 详细逻辑对比

## 配置参数差异

### Python StrategyConfig
```python
order_size: float = 1.0           # 默认 1.0
max_position: float = 5.0          # 默认 5.0
max_total_position: float = 30.0   # 默认 30.0
max_spread: float = 0.02           # 默认 0.02
min_spread: float = 0.005          # 默认 0.005
merge_threshold: float = 0.5       # 默认 0.5
max_hold_time: int = 180           # 默认 180秒
exit_before_expiry: int = 120      # 默认 120秒
take_profit: float = 0.03          # 默认 3%
stop_loss: float = 0.05            # 默认 5%
depth_lookback: int = 5            # 默认 5档
imbalance_threshold: float = 0.3   # 默认 0.3
min_price: float = 0.01            # 默认 0.01
max_price: float = 0.99            # 默认 0.99
safe_range_low: float = 0.01       # 默认 0.01
safe_range_high: float = 0.99      # 默认 0.99
price_warn_cooldown: int = 60      # 默认 60秒
```

### Rust TradingConfig (当前)
```rust
order_size: f64 = 5.0              # ❌ 默认 5.0 (应为 1.0)
max_position: f64 = 10.0           # ❌ 默认 10.0 (应为 5.0)
max_total_position: f64 = 30.0     # ✅ 新增
max_spread: f64 = 0.02             # ✅
min_spread: f64 = 0.005            # ✅
merge_threshold: f64 = ???          # ❌ 缺失
max_hold_time: u64 = ???            # ❌ 缺失
exit_before_expiry: u64 = ???      # ❌ 缺失
take_profit: f64 = ???              # ❌ 缺失
stop_loss: f64 = ???                # ❌ 缺失
depth_lookback: u64 = 5            # ✅ (硬编码)
imbalance_threshold: f64 = 0.3      # ❌ 缺失
min_price: f64 = 0.01               # ❌ 缺失 (使用 safe_range)
max_price: f64 = 0.99               # ❌ 缺失 (使用 safe_range)
safe_range_low: f64 = 0.01          # ✅
safe_range_high: f64 = 0.99         # ✅
price_warn_cooldown: u64 = ???      # ❌ 缺失
```

## 功能差异

### 1. ✅ 已一致
- [x] 订单簿深度分析
- [x] 库存偏离计算
- [x] 按 token 取消订单
- [x] 价格范围 0.01-0.99
- [x] 总持仓限制

### 2. ⚠️ 默认参数不一致
- [ ] `order_size`: 5.0 → 1.0
- [ ] `max_position`: 10.0 → 5.0

### 3. ❌ 缺失功能
- [ ] **库存合并策略** (merge_threshold)
- [ ] **持仓时间限制** (max_hold_time, exit_before_expiry)
- [ ] **止盈止损** (take_profit, stop_loss)
- [ ] **价格警告冷却** (price_warn_cooldown)
- [ ] **不平衡度阈值** (imbalance_threshold)
- [ ] **动态仓位限制** (get_position_limit)
- [ ] **跳过某边交易** (should_skip_side)
- [ ] **库存平衡调整** (calculate_balance_adjustment)
- [ ] **价格验证** (validate_price)
- [ ] **安全创建订单** (safe_create_order)

### 4. ❌ 主循环逻辑差异

**Python:**
```python
# 1. 检查库存合并机会
merge_amount = check_merge_opportunity(up_pos, down_pos)
if merge_amount:
    execute_merge(merge_amount, market_id)

# 2. 检查是否应该跳过某边
should_skip_side('UP')  # 库存过高时跳过

# 3. 获取动态仓位限制
get_position_limit('UP')  # 根据库存偏离调整

# 4. 检查是否需要平仓 (止盈止损/时间)
should_exit_position(position, price, time_to_expiry)

# 5. 安全创建订单 (先取消再创建)
safe_create_order(token, side, price, size)
```

**Rust (当前):**
```rust
// 1. 直接取消订单
executor.cancel_orders_for_market(&token_id).await;

// 2. 直接下单
executor.buy(&tid, bid_price, size).await;
executor.sell(&tid, ask_price, size).await;

// ❌ 缺少:
// - 库存合并检查
// - 跳过某边交易
// - 动态仓位限制
// - 止盈止损检查
// - 安全订单创建
```

## 需要修复的详细清单

### 高优先级 (核心交易逻辑)
1. **默认参数对齐** - order_size, max_position
2. **库存合并策略** - 同时持有多空时释放资金
3. **动态仓位限制** - 根据库存偏离调整单边限制
4. **跳过某边交易** - 库存过高时跳过买入

### 中优先级 (风险控制)
5. **止盈止损** - 自动平仓保护
6. **持仓时间限制** - 5分钟市场专用
7. **安全订单创建** - 完整的先取消再创建流程

### 低优先级 (优化)
8. **价格警告冷却** - 避免日志刷屏
9. **不平衡度阈值** - 配置化阈值
10. **库存平衡调整** - 自动平衡建议