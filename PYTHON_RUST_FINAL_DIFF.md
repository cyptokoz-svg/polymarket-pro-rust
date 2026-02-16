# Python vs Rust 剩余差异清单

## 🔍 新发现的差异

### 1. 余额检查逻辑
**Python (market_maker_monitor.py):**
```python
def _prepare_and_place_order(self, token, side, price, size, skip_balance_check=False):
    # 风控检查
    if not self.strategy.is_price_in_safe_range(price):
        return None
    
    # 余额检查可选（防止重复检查导致单腿）
    if not skip_balance_check:
        balance = self._get_usdc_balance()
        need = size * price
        if balance < need:
            return None
```
**Rust:** ❌ 没有余额检查

### 2. 订单ID格式验证
**Python:**
```python
def _is_valid_order_id(self, order_id: str) -> bool:
    """验证订单ID格式有效性"""
    if not order_id:
        return False
    if len(order_id) < 10:  # 最小长度检查
        return False
    return True
```
**Rust:** ❌ 缺失

### 3. 订单状态处理
**Python:**
```python
status = order.get('status', '')
if status in ['live', 'OPEN', 'PENDING', 'matched'] or order.get('success') or not order.get('error'):
    self.stats["orders_placed"] += 1
    logger.info(f"✅ ORDER PLACED: {order_id} (status: {status})")
```
**Rust:** ❌ 没有状态检查

### 4. 强制取消跟踪订单
**Python:**
```python
# BUG FIX 39: 强制取消该token的所有跟踪订单（防止单腿累积）
self._cancel_all_tracked_for_token(token)
```
**Rust:** ❌ 缺失

### 5. 自动赎回集成
**Python:**
```python
# 记录交易到自动赎回模块
if self.auto_redeem:
    condition_id = market.get("conditionId", "")
    self._record_trade_for_redeem(condition_id, market_slug, side, outcome)
```
**Rust:** ❌ 赎回已独立，无集成

### 6. 模拟模式
**Python:**
```python
if not self.auto_trade:
    logger.info(f"[SIMULATION] {side} {size} @ {price}")
    return "simulated"
```
**Rust:** ❌ 无模拟模式

### 7. FOK (Fill or Kill) 订单类型
**Python:**
```python
order = self.client.create_order(
    token_id=token,
    side=side,
    size=size,
    price=price,
    order_type="FOK"  # Fill or Kill
)
```
**Rust:** ❌ 使用普通限价单

### 8. 实时余额获取
**Python:**
```python
def _get_usdc_balance(self) -> float:
    """实时获取USDC余额（自动刷新Token）"""
    try:
        # 自动刷新Token（如果过期）
        if self.wallet and hasattr(self.wallet, 'refresh_token'):
            self.wallet.refresh_token()
        
        balance = self.client.get_balance()
        return float(balance.get('USDC', 0))
    except Exception as e:
        logger.error(f"Get balance error: {e}")
        return 0.0
```
**Rust:** ❌ 没有实时余额获取

### 9. 总仓位大小获取
**Python:**
```python
def _get_total_position_size(self) -> float:
    """获取总仓位大小"""
    try:
        positions = self.client.get_positions()
        total = sum(float(p.get('size', 0)) for p in positions)
        return total
    except Exception as e:
        logger.error(f"Get positions error: {e}")
        return 0.0
```
**Rust:** ❌ 没有实时仓位获取

### 10. 市场完整信息获取
**Python:**
```python
def _get_full_market(self, condition_id: str) -> Dict:
    """获取市场完整信息"""
    try:
        market = self.client.get_full_market(condition_id)
        return market
    except Exception as e:
        logger.error(f"Get market error: {e}")
        return {}
```
**Rust:** ❌ 缺失

### 11. 交易记录到赎回队列
**Python:**
```python
def _record_trade_for_redeem(self, condition_id, market_slug, side, outcome):
    """记录交易到自动赎回队列"""
    trade_record = {
        "condition_id": condition_id,
        "market_slug": market_slug,
        "side": side,
        "outcome": outcome,
        "timestamp": datetime.now().isoformat(),
        "redeemed": False
    }
    # 保存到历史文件
    history = self.load_trade_history()
    history.append(trade_record)
    self.save_trade_history(history)
```
**Rust:** ✅ 已实现（TradeHistory）

### 12. 订单重试机制
**Python:**
```python
# 下单失败时重试
for attempt in range(3):
    try:
        order = self.client.create_order(...)
        if order:
            break
    except Exception as e:
        if attempt < 2:
            time.sleep(0.5)
            continue
```
**Rust:** ✅ 已有重试机制

### 13. 持仓确认（通过API查询）
**Python:**
```python
# 不等待成交确认，直接返回（开源Bot模式）
# 后续通过API查询持仓来确认
logger.info(f"Order sent, not waiting for fill")
```
**Rust:** ✅ 已实现（不等待成交）

### 14. 错误分类处理
**Python:**
```python
try:
    order = self.client.create_order(...)
except RateLimitError:
    logger.error("Rate limited")
except InsufficientBalanceError:
    logger.error("Insufficient balance")
except Exception as e:
    logger.error(f"Unknown error: {e}")
```
**Rust:** ❌ 统一错误处理

### 15. 日志级别动态调整
**Python:**
```python
if self.verbose:
    logger.setLevel(logging.DEBUG)
else:
    logger.setLevel(logging.INFO)
```
**Rust:** ✅ 通过配置实现

### 16. 市场特定token获取
**Python:**
```python
self._token_up = market.get("tokens", {}).get("UP", {}).get("token_id")
self._token_down = market.get("tokens", {}).get("DOWN", {}).get("token_id")
```
**Rust:** ❌ 使用condition_id作为token

### 17. 订单簿实时获取
**Python:**
```python
def _get_order_book(self, token: str) -> Tuple[List, List]:
    """获取订单簿"""
    book = self.client.get_order_book(token)
    bids = book.get("bids", [])
    asks = book.get("asks", [])
    return bids, asks
```
**Rust:** ❌ 未实现（返回None）

### 18. 价格警告冷却（精确实现）
**Python:**
```python
_last_price_warnings: Dict[str, float] = {}

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
**Rust:** ❌ 只有简单警告，无冷却

### 19. 统计信息持久化
**Python:**
```python
def save_stats(self, filepath: str = "/tmp/mm_stats.json"):
    with open(filepath, 'w') as f:
        json.dump(self.stats, f, indent=2)

def load_stats(self, filepath: str = "/tmp/mm_stats.json"):
    if os.path.exists(filepath):
        with open(filepath, 'r') as f:
            self.stats = json.load(f)
```
**Rust:** ❌ 内存中，不持久化

### 20. 优雅退出保存状态
**Python:**
```python
def _signal_handler(self, signum, frame):
    logger.info("Shutting down...")
    self.save_stats()
    self.save_trade_history()
    self.running = False
```
**Rust:** ❌ 直接退出，不保存

## 关键差异总结

### 高优先级（影响交易安全）
1. **余额检查** - 防止超额下单
2. **订单ID验证** - 确保订单有效
3. **订单状态检查** - 确认下单成功
4. **强制取消跟踪订单** - 防止单腿

### 中优先级（功能完整）
5. **实时余额获取** - 动态检查
6. **实时仓位获取** - 动态检查
7. **订单簿实时获取** - 价格计算
8. **FOK订单类型** - 立即成交或取消

### 低优先级（优化）
9. **模拟模式** - 测试用
10. **统计持久化** - 重启保留
11. **价格警告冷却** - 避免日志刷屏
12. **错误分类** - 更精确处理