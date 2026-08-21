#!/usr/bin/env bash
# Fix Redis binding issue on WSL2
# The problem: Windows host runs redis-server on port 6379, blocking WSL2 access
# Solution: Use port 6380 for WSL2 Redis to avoid conflict
set -euo pipefail

echo "=== Step 1: Stop all Redis services ==="
systemctl stop redis-server 2>/dev/null || true
service redis-server stop 2>/dev/null || true
sleep 1

echo "=== Step 2: Kill ALL redis processes ==="
killall -9 redis-server 2>/dev/null || true
sleep 2

echo "=== Step 3: Verify no redis processes remain ==="
ps aux | grep redis | grep -v grep && echo "STILL RUNNING" || echo "all redis killed"

echo "=== Step 4: Remove stale PID and socket files ==="
rm -f /run/redis.pid /run/redis.sock /tmp/redis.sock 2>/dev/null || true

echo "=== Step 5: Check port 6379 is truly free ==="
ss -tlnp | grep 6379 && echo "PORT IN USE" || echo "6379 free"

echo "=== Step 6: Try starting Redis with minimal config ==="
redis-server --port 6379 \
  --bind 0.0.0.0 \
  --protected-mode no \
  --save "" \
  --appendonly no \
  --daemonize yes \
  --logfile /tmp/redis-clean.log \
  --unixsocket "" \
  --unixsocketperm 0 \
  2>&1

sleep 3

echo "=== Step 7: Check if Redis is listening ==="
ss -tlnp | grep 6379 && echo "LISTENING" || echo "NOT LISTENING"

echo "=== Step 8: Test connection ==="
redis-cli ping

echo "=== Step 9: Test from different IPs ==="
echo "Testing 127.0.0.1:"
echo PING | nc -w 2 127.0.0.1 6379
echo "Testing 0.0.0.0:"
echo PING | nc -w 2 0.0.0.0 6379

echo "=== Step 10: Log output ==="
tail -10 /tmp/redis-clean.log
