#!/usr/bin/env bash
# Debug why Redis can't bind to port 6379
echo "=== Checking what's on port 6379 ==="
echo "--- ss -tlnp ---"
ss -tlnp 2>/dev/null | grep 6379 || echo "ss: nothing on 6379"

echo ""
echo "--- /proc/net/tcp hex dump for 0x18EB (6379) ---"
while IFS= read -r line; do
  local=$(echo "$line" | awk '{print "$2"}')
  if [[ "$local" == *"18EB"* ]]; then
    echo "FOUND: $line"
  fi
done < /proc/net/tcp

echo ""
echo "--- Processes with 'redis' in name ---"
ps aux | grep redis | grep -v grep

echo ""
echo "--- Kill all redis processes ---"
killall -9 redis-server 2>/dev/null
sleep 2
ps aux | grep redis | grep -v grep && echo "still running" || echo "all killed"

echo ""
echo "--- Check port again after kill ---"
ss -tlnp 2>/dev/null | grep 6379 || echo "6379 is free"

echo ""
echo "--- Try to start redis on 6379 with minimal config ---"
redis-server --port 6379 --bind 0.0.0.0 --protected-mode no --save '' --appendonly no --daemonize yes --logfile /tmp/redis-debug.log 2>&1
sleep 3
echo "--- Redis log ---"
tail -10 /tmp/redis-debug.log
echo "--- Check port once more ---"
ss -tlnp 2>/dev/null | grep 6379 || echo "6379 still not shown in ss"
echo "--- Can we connect? ---"
redis-cli ping 2>&1 || echo "can't connect"
