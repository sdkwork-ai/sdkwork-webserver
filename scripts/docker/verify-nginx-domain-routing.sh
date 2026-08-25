#!/usr/bin/env bash
set -euo pipefail

probe_nginx() {
  local host="$1"
  code=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' --max-time 5 \
    -H "Host: ${host}" "http://127.0.0.1/" || echo ERR)
  title=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else t[:100]).replace('\n',' ')[:90])
PY
)
  echo "nginx:80 Host=$host -> $code | $title"
}

echo '=== via WSL nginx :80 ==='
for h in im-dev.sdkwork.com im-test.sdkwork.com im.sdkwork.com \
         router-dev.sdkwork.com router-test.sdkwork.com router.sdkwork.com \
         course-dev.sdkwork.com course-test.sdkwork.com course.sdkwork.com \
         code-dev.sdkwork.com server-dev.sdkwork.com; do
  probe_nginx "$h"
done

echo '=== import ports direct ==='
for pair in 'im-dev.sdkwork.com:13808' 'im-test.sdkwork.com:18898' 'im.sdkwork.com:18098' \
            'course-dev.sdkwork.com:13808' 'course-test.sdkwork.com:18898' 'course.sdkwork.com:18098'; do
  h="${pair%%:*}"; p="${pair##*:}"
  code=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' --max-time 5 \
    -H "Host: ${h}" "http://127.0.0.1:${p}/" || echo ERR)
  title=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else t[:100]).replace('\n',' ')[:90])
PY
)
  echo "import Host=$h :$p -> $code | $title"
done

echo '=== how many module nginx sites ==='
ls /etc/nginx/sites-enabled/sdkwork/*.conf 2>/dev/null | wc -l
ls /etc/nginx/sites-enabled/sdkwork/*-dev.sdkwork.com.conf 2>/dev/null | wc -l
ls /etc/nginx/sites-enabled/sdkwork/*-test.sdkwork.com.conf 2>/dev/null | wc -l
