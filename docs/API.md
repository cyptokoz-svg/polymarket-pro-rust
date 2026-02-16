# API 文档

## 核心模块

### TradeExecutor

交易执行器，封装 `rs-clob-client` 提供高级交易功能。

```rust
use polymarket_pro::trading::TradeExecutor;

let executor = TradeExecutor::new(
    private_key,
    Some(api_key),
    Some(api_secret),
    Some(api_passphrase),
)?;
```

#### 方法

- `place_order_with_validation()` - 带验证的订单放置
- `cancel_order()` - 取消订单
- `get_order_book()` - 获取订单簿
- `get_server_time()` - 获取服务器时间

### PositionTracker

持仓跟踪器，管理多市场持仓状态。

```rust
use polymarket_pro::trading::PositionTracker;

let tracker = PositionTracker::new();
tracker.update_position(market_id, side, size, price).await?;
let skew = tracker.calculate_inventory_skew().await;
```

### OrderTracker

订单跟踪器，管理活跃订单并自动清理过期订单。

```rust
use polymarket_pro::trading::OrderTracker;

let tracker = OrderTracker::new();
tracker.track_order(token_id, order_id, side, price, size);
let old_orders = tracker.get_old_orders(Duration::from_secs(120));
```

## 配置

### Config

配置管理，支持文件和环境变量。

```rust
use polymarket_pro::config;

// 从文件加载
let config = Config::load()?;

// 从环境变量加载
let config = config::from_env()?;

// 验证配置
config.validate()?;
```

#### 环境变量

| 变量 | 必需 | 说明 |
|------|------|------|
| `PK` | ✅ | 私钥（0x + 64位hex） |
| `SAFE_ADDRESS` | ✅ | Gnosis Safe地址 |
| `BROWSER_ADDRESS` | ✅ | 浏览器地址 |
| `POLY_BUILDER_API_KEY` | ❌ | Builder API密钥 |
| `POLY_BUILDER_API_SECRET` | ❌ | Builder API密钥 |
| `POLY_BUILDER_API_PASSPHRASE` | ❌ | Builder API口令 |

## WebSocket

### PolymarketWebSocket

实时价格推送客户端。

```rust
use polymarket_pro::websocket::PolymarketWebSocket;

let mut ws = PolymarketWebSocket::new();
ws.connect().await?;

let mut rx = ws.subscribe();
while let Ok(update) = rx.recv().await {
    println!("{} = {}", update.market_id, update.price);
}
```

## 错误处理

### TradingError

```rust
use polymarket_pro::trading::TradingError;

match result {
    Err(TradingError::RateLimited { message }) => {
        // 处理速率限制
    }
    Err(TradingError::InsufficientBalance { available, required }) => {
        // 处理余额不足
    }
    _ => {}
}
```

## 事件回调

### CallbackManager

```rust
use polymarket_pro::trading::CallbackManager;

let mut callbacks = CallbackManager::new();

callbacks.on_trade(|trade| {
    println!("Trade executed: {:?}", trade);
});

callbacks.on_error(|error| {
    eprintln!("Error: {:?}", error);
});
```