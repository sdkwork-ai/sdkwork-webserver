#!/usr/bin/env bash
set -euo pipefail

wait_healthy() {
  local name="$1"
  for i in $(seq 1 45); do
    st=$(docker inspect -f '{{.State.Health.Status}}' "$name" 2>/dev/null || echo starting)
    echo "$name t=$i status=$st"
    [ "$st" = "healthy" ] && return 0
    sleep 4
  done
  return 1
}

wait_healthy sdkwork-webserver-test
wait_healthy sdkwork-webserver-production

echo '=== test :18888 ==='
python3 /mnt/e/sdkwork-space/sdkwork-webserver/scripts/docker/probe-module-hosts.py --port 18888 --timeout 4

echo '=== production :18080 ==='
python3 /mnt/e/sdkwork-space/sdkwork-webserver/scripts/docker/probe-module-hosts.py --port 18080 --timeout 4

echo '=== spot cross-env ==='
for pair in \
  'im-dev.sdkwork.com:13808' \
  'im-test.sdkwork.com:18888' \
  'im.sdkwork.com:18080' \
  'course-dev.sdkwork.com:13808' \
  'course-test.sdkwork.com:18888' \
  'code-dev.sdkwork.com:13808' \
  'router-dev.sdkwork.com:13808'; do
  h="${pair%%:*}"
  p="${pair##*:}"
  code=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' --resolve "${h}:${p}:127.0.0.1" "http://${h}:${p}/" || echo ERR)
  title=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else '?')[:70].replace('\n', ' '))
PY
)
  echo "$h:$p -> $code | $title"
done
