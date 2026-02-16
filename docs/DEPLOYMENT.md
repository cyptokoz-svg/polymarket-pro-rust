# 部署指南

## 系统要求

- Linux/macOS/Windows
- Rust 1.75+
- 2GB RAM
- 网络连接

## 生产部署

### 1. 构建发布版本

```bash
cargo build --release
```

二进制文件位于 `target/release/polymarket-pro`。

### 2. 创建 systemd 服务

创建 `/etc/systemd/system/polymarket-pro.service`:

```ini
[Unit]
Description=Polymarket Pro Trading Bot
After=network.target

[Service]
Type=simple
User=trading
Group=trading
WorkingDirectory=/opt/polymarket-pro
Environment="RUST_LOG=info"
EnvironmentFile=/opt/polymarket-pro/.env
ExecStart=/opt/polymarket-pro/polymarket-pro
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

### 3. 配置环境变量

创建 `/opt/polymarket-pro/.env`:

```bash
PK=0x...
SAFE_ADDRESS=0x...
BROWSER_ADDRESS=0x...
POLY_BUILDER_API_KEY=...
POLY_BUILDER_API_SECRET=...
POLY_BUILDER_API_PASSPHRASE=...
RUST_LOG=info
```

### 4. 启动服务

```bash
# 创建用户
sudo useradd -r -s /bin/false trading

# 设置权限
sudo mkdir -p /opt/polymarket-pro
sudo cp target/release/polymarket-pro /opt/polymarket-pro/
sudo cp polymarket-pro.toml /opt/polymarket-pro/
sudo chown -R trading:trading /opt/polymarket-pro
sudo chmod 600 /opt/polymarket-pro/.env

# 启动服务
sudo systemctl daemon-reload
sudo systemctl enable polymarket-pro
sudo systemctl start polymarket-pro

# 查看日志
sudo journalctl -u polymarket-pro -f
```

## Docker 部署

### Dockerfile

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/polymarket-pro /usr/local/bin/
USER 1000
CMD ["polymarket-pro"]
```

### docker-compose.yml

```yaml
version: '3.8'
services:
  polymarket-pro:
    build: .
    container_name: polymarket-pro
    restart: unless-stopped
    env_file: .env
    volumes:
      - ./data:/data
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
```

## 监控

### 日志监控

```bash
# 实时日志
journalctl -u polymarket-pro -f

# 错误统计
grep "ERROR" /var/log/polymarket-pro.log | wc -l
```

### 健康检查

```bash
# 检查进程
systemctl is-active polymarket-pro

# 检查资源使用
ps aux | grep polymarket-pro
```

## 备份

### 数据文件

```bash
# 备份统计数据
cp /tmp/polymarket_stats.json /backup/

# 备份交易历史
cp /tmp/polymarket_trade_history.json /backup/
```

### 配置备份

```bash
tar czf backup-$(date +%Y%m%d).tar.gz \
  polymarket-pro.toml \
  .env
```

## 更新

```bash
# 停止服务
sudo systemctl stop polymarket-pro

# 备份数据
cp /tmp/polymarket_stats.json /tmp/polymarket_stats.json.bak

# 更新代码
git pull

# 重新构建
cargo build --release

# 替换二进制
sudo cp target/release/polymarket-pro /opt/polymarket-pro/

# 启动服务
sudo systemctl start polymarket-pro
```

## 故障排除

### 服务无法启动

```bash
# 检查配置
sudo -u trading /opt/polymarket-pro/polymarket-pro --config-check

# 查看详细日志
RUST_LOG=debug /opt/polymarket-pro/polymarket-pro
```

### 内存不足

```bash
# 添加交换空间
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
```