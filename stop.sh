#!/bin/bash
# stop.sh - 停止 Polymarket Pro

echo "🛑 Stopping Polymarket Pro..."

# 查找并停止进程
if pgrep -f "polymarket-pro" > /dev/null; then
    pkill -f "polymarket-pro"
    sleep 2
    
    # 强制停止如果还在运行
    if pgrep -f "polymarket-pro" > /dev/null; then
        pkill -9 -f "polymarket-pro"
    fi
    
    echo "✅ Polymarket Pro stopped"
else
    echo "ℹ️  Polymarket Pro is not running"
fi
