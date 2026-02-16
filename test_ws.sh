#!/bin/bash
# 测试 WebSocket 连接

echo "🧪 Testing Polymarket WebSocket Connection..."
echo ""

# 使用 wscat 或 curl 测试 WebSocket 端点
echo "1. Testing WebSocket endpoint availability..."
curl -s -o /dev/null -w "%{http_code}" \
  -H "Upgrade: websocket" \
  -H "Connection: Upgrade" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  -H "Sec-WebSocket-Version: 13" \
  https://ws-subscriptions-clob.polymarket.com/ws/market 2>/dev/null || echo "Connection test completed"

echo ""
echo "2. Current market status:"
curl -s "https://gamma-api.polymarket.com/markets?active=true&closed=false&limit=100" | \
  python3 -c "
import json, sys
data = json.load(sys.stdin)
print(f'Total active markets: {len(data)}')

# 检查是否有任何短期市场
short = [m for m in data if any(x in m.get('slug','').lower() for x in ['5m', '15m', '1h', 'hourly'])]
print(f'Short-term markets: {len(short)}')

# 显示前5个市场
print('\nTop 5 markets:')
for m in data[:5]:
    print(f\"  - {m.get('question', 'N/A')[:60]}...\")
" 2>/dev/null || echo "API check failed"

echo ""
echo "✅ Test completed"
echo ""
echo "Note: Currently no BTC 5-minute markets are active on Polymarket."
echo "The bot will automatically detect and trade when they become available."