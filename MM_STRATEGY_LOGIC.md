# Rust 做市商策略逻辑详解

## 概述

Rust 版本的做市商策略完全复刻 Python `polymaker_5m.py` 的逻辑，包含以下核心模块：

1. **订单簿分析** (`orderbook.rs`)
2. **持仓管理** (`position.rs`)
3. **交易执行** (`main.rs` 中的 `run_trading_cycle_single_market`)

---

## 1. 订单簿分析 (`orderbook.rs`)

### 1.1 数据结构

```rust
pub struct OrderBookDepth {
    pub best_bid: OrderBookLevel,      // 最佳买价
    pub best_ask: OrderBookLevel,      // 最佳卖价
    pub second_bid: OrderBookLevel,    // 次佳买价
    pub second_ask: OrderBookLevel,    // 次佳卖价
    pub bid_depth: f64,                // 买盘深度
    pub ask_depth: f64,                // 卖盘深度
    pub imbalance: f64,                // 买卖盘不平衡度 (-1 到 1)
}
```

### 1.2 安全分析函数

```rust
pub fn analyze_order_book_depth_safe(
    bids: &[serde_json::Value],
    asks: &[serde_json::Value],
    min_size: f64,           // 最小订单量过滤
    depth_lookback: usize,   // 查看深度
) -> Option<OrderBookDepth>
```

**逻辑**：
1. 解析并过滤订单簿数据（过滤掉小于 `min_size` 的订单）
2. 检查是否有足够的数据（至少 2 个 bid 和 2 个 ask）
3. 如果数据不足，返回 `None`（避免使用默认价格）
4. 计算买盘/卖盘深度和不平衡度

### 1.3 做市价格计算

```rust
pub fn calculate_mm_prices(
    depth: &OrderBookDepth,
    inventory_skew: f64,     // 库存偏离 (-1 到 1)
    min_spread: f64,         // 最小价差
    max_spread: f64,         // 最大价差
) -> (f64, f64)            // (买价, 卖价)
```

**算法**：
```
中间价 = (最佳买价 + 最佳卖价) / 2
价差 = clamp(实际价差, min_spread, max_spread)
半价差 = 价差 / 2

库存调整 = inventory_skew * 0.01        // 1% 调整
不平衡调整 = -depth.imbalance * 0.005   // 0.5% 调整

买价 = 中间价 - 半价差 + 库存调整 + 不平衡调整
卖价 = 中间价 + 半价差 + 库存调整 + 不平衡调整

// 限制在有效范围内
买价 = clamp(买价, 0.01, 0.99)
卖价 = clamp(卖价, 0.01, 0.99)
```

**逻辑说明**：
- **库存偏离为正**（多头过多）：降低买/卖价，减少买入吸引卖出
- **买盘深度 > 卖盘深度**（imbalance 为负）：提高买价，吸引卖家
- **卖盘深度 > 买盘深度**（imbalance 为正）：降低卖价，吸引买家

---

## 2. 持仓管理 (`position.rs`)

### 2.1 库存偏离计算

```rust
pub async fn calculate_inventory_skew(&self) -> f64
```

**公式**：
```
UP 价值 = 所有买单持仓的总价值
DOWN 价值 = 所有卖单持仓的总价值
总价值 = UP 价值 + DOWN 价值

库存偏离 = (UP 价值 - DOWN 价值) / 总价值  // -1 到 1
```

- **偏离 > 0**: 多头过多（UP 持仓多）
- **偏离 < 0**: 空头过多（DOWN 持仓多）
- **偏离 = 0**: 完全平衡

### 2.2 跳过某边判断

```rust
pub async fn should_skip_side(
    &self,
    side: Side,
) -> (bool, String)
```

**逻辑**：
```rust
match side {
    Side::Buy => {
        // 如果已经多头过多，跳过买入
        if inventory_skew > 0.7 {  // 70% 阈值
            return (true, "Inventory too long".to_string());
        }
    }
    Side::Sell => {
        // 如果已经空头过多，跳过分卖出
        if inventory_skew < -0.7 {  // -70% 阈值
            return (true, "Inventory too short".to_string());
        }
    }
}
```

### 2.3 动态仓位限制

```rust
pub async fn get_position_limit(
    &self,
    side: Side,
    max_position: f64,
) -> f64
```

**算法**：
```
库存偏离 = calculate_inventory_skew()

对于买单:
    如果 偏离 > 0.5:    限制 = max_position * 0.2    // 大幅减少
    如果 偏离 > 0.3:    限制 = max_position * 0.5    // 中度减少
    否则:               限制 = max_position          // 正常

对于卖单:
    如果 偏离 < -0.5:   限制 = max_position * 0.2
    如果 偏离 < -0.3:   限制 = max_position * 0.5
    否则:               限制 = max_position
```

### 2.4 合并机会检测

```rust
pub async fn check_merge_opportunity(
    &self,
    market_id: &str,
    threshold: f64,
) -> Option<f64>
```

**逻辑**：
- 检测是否可以合并 UP 和 DOWN 持仓
- 如果两边都有持仓且可以抵消，返回可合并数量

---

## 3. 交易循环 (`main.rs`)

### 3.1 主循环流程

```rust
async fn run_trading_cycle_single_market(
    // ... 参数
) -> Result<()>
```

**步骤**：

1. **检查总仓位限制**
   ```rust
   if status.total_value >= trading_config.max_total_position {
       return Ok(());  // 跳过交易
   }
   ```

2. **检测合并机会**
   ```rust
   if let Some(merge_amount) = position_tracker.check_merge_opportunity(...) {
       info!("Merge opportunity: {:.2} shares", merge_amount);
   }
   ```

3. **获取价格**
   - 优先使用 WebSocket 实时价格
   - 回退到 API 查询

4. **价格验证**
   ```rust
   if price < min_price || price > max_price {
       return Ok(());  // 价格无效，跳过
   }
   ```

5. **安全范围警告**
   ```rust
   if price < safe_range_low || price > safe_range_high {
       warn!("Price outside safe range");  // 记录警告但继续
   }
   ```

6. **取消旧订单**（关键步骤）
   ```rust
   match executor.cancel_orders_for_market(&token_id).await {
       Ok(_) => info!("Cancelled existing orders"),
       Err(e) => {
           error!("Failed to cancel, stopping cycle");
           return Ok(());  // 取消失败则停止，避免重复订单
       }
   }
   ```

7. **检查是否跳过某边**
   ```rust
   let (skip_buy, reason_buy) = position_tracker.should_skip_side(Side::Buy).await;
   let (skip_sell, reason_sell) = position_tracker.should_skip_side(Side::Sell).await;
   ```

8. **获取动态仓位限制**
   ```rust
   let buy_limit = position_tracker.get_position_limit(Side::Buy, max_position).await;
   let sell_limit = position_tracker.get_position_limit(Side::Sell, max_position).await;
   ```

9. **计算做市价格**
   ```rust
   let (bid_price, ask_price) = if let Some(depth) = analyze_order_book_depth_safe(...) {
       calculate_mm_prices(&depth, inventory_skew, min_spread, max_spread)
   } else {
       // 订单簿数据不足，使用 WebSocket 价格 + 固定价差
       let bid_price = price - spread / 2.0;
       let ask_price = price + spread / 2.0;
       (bid_price, ask_price)
   };
   ```

10. **并发下单**
    ```rust
    let (buy_result, sell_result) = tokio::join!(
        place_side_order(..., Side::Buy, bid_price, ...),
        place_side_order(..., Side::Sell, ask_price, ...)
    );
    ```

---

## 4. 配置参数

```rust
pub struct TradingConfig {
    pub order_size: f64,              // 默认: 1.0
    pub max_position: f64,            // 默认: 5.0
    pub max_total_position: f64,      // 默认: 30.0
    pub safe_range_low: f64,          // 默认: 0.01
    pub safe_range_high: f64,         // 默认: 0.99
    pub min_spread: f64,              // 默认: 0.002
    pub max_spread: f64,              // 默认: 0.02
    pub spread: f64,                  // 默认: 0.01
    pub depth_lookback: i32,          // 默认: 5
    pub merge_threshold: f64,         // 默认: 0.1
}
```

---

## 5. 与 Python 版本的差异

| 功能 | Python | Rust | 说明 |
|------|--------|------|------|
| 订单簿分析 | ✅ | ✅ | 完全一致 |
| 库存偏离 | ✅ | ✅ | 完全一致 |
| 动态仓位限制 | ✅ | ✅ | 完全一致 |
| 合并检测 | ✅ | ✅ | 完全一致 |
| 并发下单 | asyncio | tokio | 实现方式不同，效果相同 |
| 价格精度 | 4位小数 | 4位小数 | 一致 |

---

## 6. 总结

Rust 做市商策略完全复刻 Python 逻辑，核心特点：

1. **双边做市**：同时挂买单和卖单
2. **库存管理**：根据持仓偏离动态调整价格和仓位
3. **订单簿分析**：利用深度信息优化报价
4. **风险控制**：多重检查（价格范围、仓位限制、取消确认）
5. **并发执行**：使用 `tokio::join!` 同时下双边订单

**策略目标**：在提供流动性的同时，通过库存管理控制风险，赚取买卖价差。