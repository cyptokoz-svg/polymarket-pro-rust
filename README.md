# Polymarket Pro

高性能 Polymarket 交易机器人，使用 Rust 和官方 `rs-clob-client` 库构建。

## 功能特性

- **实时交易**: 45秒交易周期，WebSocket实时价格推送
- **自动做市**: 基于库存偏斜的动态买卖价差调整
- **风险管理**: 价格范围验证、持仓限制、订单超时清理
- **Gasless赎回**: 通过 Builder Relayer 实现无 gas 费用赎回
- **退出管理**: 支持止盈/止损、持仓时间限制
- **模拟模式**: 支持模拟交易记录

## 快速开始

### 1. 安装依赖

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆项目
git clone <repository-url>
cd polymarket-pro-rust

# 构建发布版本
cargo build --release
```

### 2. 配置环境变量（推荐）

**安全最佳实践**: 敏感信息（私钥、API密钥）应通过环境变量提供，而非配置文件。

```bash
export PK="0x..."                          # 私钥（64位hex）- 必需
export SAFE_ADDRESS="0x..."                # Gnosis Safe地址 - 必需
export BROWSER_ADDRESS="0x..."             # 浏览器地址（用于验证）- 必需
export POLY_BUILDER_API_KEY="..."          # Builder API密钥（可选）
export POLY_BUILDER_API_SECRET="..."       # Builder API密钥（可选）
export POLY_BUILDER_API_PASSPHRASE="..."   # Builder API口令（可选）
```

**注意**: 环境变量优先级高于配置文件。如果设置了环境变量，配置文件中的对应值将被忽略。

### 3. 运行

```bash
# 直接运行
./target/release/polymarket-pro

# 或使用 cargo
cargo run --release
```

## 配置文件（可选）

创建 `polymarket-pro.toml` 用于非敏感配置：

```toml
[trading]
order_size = 1.0
max_position = 5.0
max_total_position = 30.0
safe_range_low = 0.01
safe_range_high = 0.99
refresh_interval = 45

[risk]
take_profit = 0.03
stop_loss = 0.05
max_hold_time = 180

[websocket]
enabled = true
```

**注意**: 不要在配置文件中存放私钥或API密钥！使用环境变量提供敏感信息。

## 项目结构

```
src/
├── main.rs              # 主入口
├── api/                 # API客户端
│   ├── clob.rs         # CLOB交易API
│   ├── gamma.rs        # Gamma市场数据API
│   └── market.rs       # 市场信息转换
├── trading/            # 交易核心
│   ├── executor.rs     # 交易执行器
│   ├── position.rs     # 持仓管理
│   ├── order.rs        # 订单构建
│   ├── orderbook.rs    # 订单簿分析
│   ├── stats.rs        # 统计跟踪
│   └── ...
├── wallet/             # 钱包管理
│   ├── mod.rs          # 私钥钱包
│   └── safe.rs         # Gnosis Safe集成
├── websocket/          # WebSocket客户端
├── redeem/             # 赎回模块
└── utils/              # 工具函数
```

## 安全特性

- ✅ 私钥长度和格式验证
- ✅ 地址格式验证
- ✅ HTTP请求超时（30秒）
- ✅ 文件权限保护（0o600）
- ✅ 生产环境错误信息脱敏
- ✅ WebSocket强制TLS（wss://）
- ✅ 加密安全随机数生成

## 监控指标

程序会定期输出以下统计信息：

```
📊 Trading Stats: 45 cycles, 12 orders, 0 errors, PnL: +2.34 USDC
```

统计信息保存在 `/tmp/polymarket_stats.json`。

## 故障排除

### 编译错误

```bash
# 更新依赖
cargo update

# 清理并重新构建
cargo clean && cargo build --release
```

### 配置验证失败

- 确保 `PK` 是 66 字符（0x + 64位hex）
- 确保 `SAFE_ADDRESS` 是 42 字符（0x + 40位hex）
- 确保设置了 `BROWSER_ADDRESS` 环境变量

### 连接问题

- 检查网络连接
- 确认 API 密钥有效
- 查看日志输出：`RUST_LOG=debug cargo run`

## 开发

### 运行测试

```bash
cargo test
```

### 代码检查

```bash
cargo clippy
cargo fmt
```

## 许可证

MIT License