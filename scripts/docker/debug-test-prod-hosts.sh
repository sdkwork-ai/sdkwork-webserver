#!/usr/bin/env bash
set -euo pipefail

echo '=== which confs are registered as imports? ==='
for c in sdkwork-webserver-development sdkwork-webserver-test sdkwork-webserver-production; do
  echo "-- $c"
  docker exec "$c" sh -c 'ls /etc/sdkwork/webserver/modules/*/nginx.standalone.*.conf 2>/dev/null | head -5; echo ...; find /etc/sdkwork/webserver -name "*.imports*" -o -name "*module*list*" 2>/dev/null | head; grep -R "sdkwork-im" /etc/sdkwork/webserver/*.toml /etc/sdkwork/webserver/**/*.toml 2>/dev/null | head -20'
done

echo '=== direct curl inside containers ==='
docker exec sdkwork-webserver-test sh -c 'wget -qO- --header="Host: im-test.sdkwork.com" http://127.0.0.1:80/ 2>/dev/null | head -c 200 || curl -sS -H "Host: im-test.sdkwork.com" http://127.0.0.1:8080/ | head -c 300'
echo
docker exec sdkwork-webserver-production sh -c 'curl -sS -H "Host: im.sdkwork.com" http://127.0.0.1:8080/ | head -c 400; echo; curl -sS -H "Host: im.sdkwork.com" http://127.0.0.1:80/ | head -c 400'
echo
echo '=== host port probes with correct Host ==='
for pair in 'im-test.sdkwork.com:18888' 'im.sdkwork.com:18080'; do
  h="${pair%%:*}"; p="${pair##*:}"
  code=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' -H "Host: $h" "http://127.0.0.1:$p/" || echo ERR)
  title=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else t[:80]).replace('\n',' ')[:80])
PY
)
  echo "$h:$p Host-header -> $code | $title"
  code2=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' --resolve "${h}:${p}:127.0.0.1" "http://${h}:${p}/" || echo ERR)
  title2=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else t[:80]).replace('\n',' ')[:80])
PY
)
  echo "$h:$p resolve -> $code2 | $title2"
done

echo '=== gateway listen / import logs ==='
docker logs sdkwork-webserver-production 2>&1 | grep -iE 'sdkwork-im|import|listen|bind|virtual' | tail -30
docker logs sdkwork-webserver-test 2>&1 | grep -iE 'sdkwork-im|import|listen|bind|virtual' | tail -20
