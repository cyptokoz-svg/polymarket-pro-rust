# Python vs Rust 完整差异清单

## 🔍 新发现的差异

### 1. API 限流保护
**Python (market_maker_monitor.py):**
```python
_last_api_call_time = 0
_min_api_delay = 0.2  # 200ms minimum delay between API calls

def _rate_limit_protect(self):
    elapsed = time.time() - self._last_api_call_time
    if elapsed < self._min_api_delay:
        sleep_time = self._min_api_delay - elapsed
        time.sleep(sleep_time)
    self._last_api_call_time = time.time()
```
**Rust:** ❌ 缺失

### 2. 订单成交等待
**Python:**
```python
def _wait_for_fill(self, order_id: str, token: str, max_wait: int = 10) -> float:
    """等待订单成交，返回实际成交数量"""
    for i in range(max_wait):
        time.sleep(1)
        orders = self.client.get_orders(status="ALL")
        # Check if FILLED, PARTIAL, CANCELLED, etc.
```
**Rust:** ❌ 缺失

### 3. 取消旧挂单 (>2分钟)
**Python:**
```python
def _cancel_old_pending_orders(self):
    """取消超过2分钟未成交的订单"""
    orders = self.client.get_orders(status="OPEN")
    for order in orders:
        if now - order_timestamp > 120:  # 2 minutes
            self.client.cancel_order(order_id=order_id)
```
**Rust:** ❌ 缺失

### 4. 活跃订单跟踪
**Python:**
```python
_active_orders: Dict[str, Dict] = {}  # token -> order info

def _track_order(self, token: str, order_id: str, side: str, price: float, size: float):
    """跟踪活跃订单"""
    self._active_orders[token] = {
        "order_id": order_id,
        "side": side,
        "price": price,
        "size": size,
        "timestamp": time.time()
    }
```
**Rust:** ❌ 缺失

### 5. 交易历史记录
**Python (auto_trader.py):**
```python
HISTORY_FILE = "/tmp/polymarket_trade_history.json"

def load_trade_history(self) -> List[Dict]:
    """加载交易历史"""
    if os.path.exists(HISTORY_FILE):
        with open(HISTORY_FILE, 'r') as f:
            return json.load(f)
    return []

def save_trade_history(self, history: List[Dict]):
    """保存交易历史"""
    with open(HISTORY_FILE, 'w') as f:
        json.dump(history, f, indent=2)
```
**Rust:** ❌ 缺失

### 6. 余额检查和等待
**Python (auto_trader.py):**
```python
def wait_for_balance_refresh(self, timeout=300) -> bool:
    """等待余额刷新"""
    while time.time() - start_time < timeout:
        balance = self.get_usdc_balance()
        if balance >= MIN_USDC_FOR_TRADING:
            return True
        time.sleep(check_interval)
```
**Rust:** ❌ 缺失

### 7. Discord 通知
**Python:**
```python
from polymarket.notifier import DiscordNotifier

self.notifier = DiscordNotifier()
self.notifier.send_message(f"Order placed: {side} {size} @ {price}")
```
**Rust:** ❌ 缺失 (用户明确排除)

### 8. 完整工作流编排
**Python (auto_trader.py):**
```python
def run_full_workflow(self):
    # 阶段1: 自动赎回
    redeemed = self.auto_redeem()
    
    # 阶段2: 等待余额刷新
    balance_ready = self.wait_for_balance_refresh()
    
    # 阶段3: 启动做市商
    if balance_ready:
        self.start_market_maker(force_restart=redeemed)
```
**Rust:** ❌ 缺失 (赎回已独立)

### 9. 市场特定处理 (5分钟市场)
**Python:**
```python
# 检查是否是5分钟市场
if self._is_5m_market(market):
    # 应用5分钟市场专用逻辑
    max_hold_time = 180
    exit_before_expiry = 120
```
**Rust:** ❌ 缺失 (通用处理)

### 10. 订单刷新时间跟踪
**Python:**
```python
_last_order_refresh: float = 0
ORDER_REFRESH_INTERVAL = 45

def _should_refresh_orders(self) -> bool:
    elapsed = time.time() - self._last_order_refresh
    return elapsed >= self.ORDER_REFRESH_INTERVAL
```
**Rust:** ✅ 使用 tokio::time::interval

### 11. WebSocket 价格时间戳跟踪
**Python:**
```python
_last_ws_update: Optional[datetime] = None

def _is_ws_price_fresh(self) -> bool:
    if not self._last_ws_update:
        return False
    elapsed = (datetime.now(timezone.utc) - self._last_ws_update).total_seconds()
    return elapsed < 5  # 5 seconds freshness
```
**Rust:** ❌ 缺失

### 12. 统计信息跟踪
**Python:**
```python
self.stats = {
    "start_time": datetime.now(timezone.utc).isoformat(),
    "orders_placed": 0,
    "orders_filled": 0,
    "orders_cancelled": 0,
    "errors": 0,
}
```
**Rust:** ❌ 缺失

### 13. 信号处理 (优雅退出)
**Python:**
```python
import signal

def _signal_handler(self, signum, frame):
    logger.info("Received signal %d, shutting down...", signum)
    self.running = False

signal.signal(signal.SIGTERM, self._signal_handler)
signal.signal(signal.SIGINT, self._signal_handler)
```
**Rust:** ✅ 使用 tokio::signal::ctrl_c

### 14. 日志轮转
**Python:**
```python
from logging.handlers import RotatingFileHandler

log_handler = RotatingFileHandler(log_file, maxBytes=10*1024*1024, backupCount=5)
```
**Rust:** ❌ 缺失 (使用 tracing，无轮转)

### 15. 检查间隔 vs 订单刷新间隔
**Python:**
```python
CHECK_INTERVAL = 3  # 每3秒检查一次
ORDER_REFRESH_INTERVAL = 45  # 每45秒刷新订单
```
**Rust:** ❌ 只有45秒刷新，没有3秒检查

## 关键缺失功能总结

### 高优先级 (影响交易安全)
1. **API 限流保护** - 防止被限流
2. **取消旧挂单** - 清理超过2分钟的订单
3. **订单成交等待** - 确认订单状态
4. **活跃订单跟踪** - 跟踪订单生命周期

### 中优先级 (影响功能完整)
5. **交易历史记录** - 记录和加载交易
6. **余额等待刷新** - 赎回后等待余额
7. **WebSocket 价格新鲜度** - 检查价格时效
8. **统计信息** - 交易统计

### 低优先级 (优化)
9. **3秒检查间隔** - 更频繁的检查
10. **日志轮转** - 日志文件管理
11. **5分钟市场专用逻辑** - 特定市场处理