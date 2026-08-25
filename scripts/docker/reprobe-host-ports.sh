#!/usr/bin/env bash
set -euo pipefail

probe() {
  local label="$1" host="$2" port="$3"
  code=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' --max-time 5 \
    -H "Host: ${host}" "http://127.0.0.1:${port}/" || echo ERR)
  title=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else t[:100]).replace('\n',' ')[:90])
PY
)
  echo "$label | Host=$host port=$port -> $code | $title"
}

echo '=== docker port maps ==='
docker ps --format '{{.Names}}\t{{.Ports}}' | grep webserver || true

echo '=== host probes ==='
probe development-im im-dev.sdkwork.com 13808
probe test-im im-test.sdkwork.com 18888
probe prod-im im.sdkwork.com 18080
probe prod-im-via-dev-host im-dev.sdkwork.com 18080
probe test-course course-test.sdkwork.com 18888
probe prod-course course.sdkwork.com 18080
probe router-prod router.sdkwork.com 18080
probe router-test router-test.sdkwork.com 18888

echo '=== inside prod on 8080 ==='
docker exec sdkwork-webserver-production sh -c \
  'curl -sS -o /tmp/b.html -w "%{http_code}" -H "Host: im.sdkwork.com" http://127.0.0.1:8080/; echo; head -c 200 /tmp/b.html; echo'
docker exec sdkwork-webserver-test sh -c \
  'curl -sS -o /tmp/b.html -w "%{http_code}" -H "Host: im-test.sdkwork.com" http://127.0.0.1:8080/; echo; head -c 200 /tmp/b.html; echo'

echo '=== what listens on host 18080/18888 ==='
ss -ltnp 2>/dev/null | grep -E ':18080|:18888|:13808' || netstat -ltnp 2>/dev/null | grep -E '18080|18888|13808' || true
