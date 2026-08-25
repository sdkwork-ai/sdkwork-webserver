#!/usr/bin/env bash
set -euo pipefail

for i in $(seq 1 45); do
  st=$(docker inspect -f '{{.State.Health.Status}}' sdkwork-webserver-development 2>/dev/null || echo starting)
  echo "t=$i status=$st"
  [ "$st" = "healthy" ] && break
  sleep 4
done

docker logs --tail 20 sdkwork-webserver-development 2>&1 | grep -E 'Materialized|imported' | head -8 || true
python3 /mnt/e/sdkwork-space/sdkwork-webserver/scripts/docker/probe-module-hosts.py --port 13808 --timeout 4

echo '--- spot ---'
for h in birdcoder-dev.sdkwork.com course-dev.sdkwork.com forum-dev.sdkwork.com im-dev.sdkwork.com account-dev.sdkwork.com; do
  code=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' --resolve "${h}:13808:127.0.0.1" "http://${h}:13808/" || echo ERR)
  title=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else '?')[:80].replace('\n', ' '))
PY
)
  echo "$h -> $code | $title"
done
