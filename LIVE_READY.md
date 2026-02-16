# 实盘测试准备完成 ✅

## 更新时间
2026-02-16 22:27

## API 认证状态 ✅

**已配置**:
- ✅ Builder API Key/Secret/Passphrase
- ✅ 私钥 (PK)
- ✅ EOA 地址
- ✅ Safe 地址

**注意**: 
- Builder API 可以作为 CLOB API 使用
- 代码已支持从私钥自动 derive API key（备用方案）

---

## 配置确认

| 参数 | 值 | 说明 |
|------|-----|------|
| ORDER_SIZE | 1.0 | 每单 $1 |
| MAX_POSITION | 6.0 | 单个市场最大 $6 |
| MAX_TOTAL_POSITION | 36.0 | 总持仓最大 $36 |
| SIMULATION_MODE | false | 关闭模拟 |

---

## 实盘前检查清单

- [x] API 认证配置完成
- [x] 私钥和地址匹配
- [x] 仓位配置确认
- [x] 模拟模式测试通过
- [ ] 关闭模拟模式
- [ ] 确认 USDC 余额
- [ ] 小额测试 ($1)

---

## 启动实盘命令

```bash
cd /root/.openclaw/workspace/polymarket-pro-rust

# 1. 关闭模拟模式
sed -i 's/SIMULATION_MODE=true/SIMULATION_MODE=false/' .env

# 2. 确认配置
grep -E "ORDER_SIZE|MAX_POSITION|SIMULATION" .env

# 3. 启动实盘
./start.sh
```

---

## 监控命令

```bash
# 查看日志
tail -f ~/.polymarket-pro/trading.log

# 查看进程
ps aux | grep polymarket

# 停止
./stop.sh
```

---

## 风险提示

⚠️ **实盘交易有风险**:
- 建议先小额测试 ($1)
- 观察 1-2 个周期确认正常
- 随时准备停止

**准备好实盘了吗？**
