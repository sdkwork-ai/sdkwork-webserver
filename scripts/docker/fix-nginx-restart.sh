#!/usr/bin/env bash
set -euo pipefail

cat > /etc/nginx/conf.d/sdkwork-server-names-hash.conf <<EOF
server_names_hash_max_size 4096;
server_names_hash_bucket_size 128;
EOF

for f in /etc/nginx/sites-available/sdkwork/*.conf; do
  [ -f "$f" ] || continue
  sed -i -e '/listen 80;/d' -e '/listen \[::\]:80;/d' "$f"
done

nginx -t
systemctl restart nginx
systemctl is-active nginx
echo "site count: $(ls /etc/nginx/sites-enabled/sdkwork/*.conf | wc -l)"

for h in course-dev.sdkwork.com code-dev.sdkwork.com im.sdkwork.com forum-test.sdkwork.com router-test.sdkwork.com; do
  code=$(curl --noproxy '*' -sS -o /tmp/b.html -w '%{http_code}' -H "Host: ${h}" http://127.0.0.1:8088/)
  title=$(python3 - <<'PY'
import re, pathlib
t = pathlib.Path('/tmp/b.html').read_text(encoding='utf-8', errors='ignore')
m = re.search(r'<title>(.*?)</title>', t, re.I | re.S)
print((m.group(1) if m else '?')[:70].replace('\n', ' '))
PY
)
  echo "${h} -> ${code} | ${title}"
done
