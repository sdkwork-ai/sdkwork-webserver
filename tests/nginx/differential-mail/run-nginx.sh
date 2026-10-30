#!/usr/bin/env bash
# Render the mail differential fixture for the real nginx oracle and capture it.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NGINX_BIN="${NGINX_BIN:-/tmp/nginx-deb/root/usr/sbin/nginx}"
PY="${PYTHON:-python3}"
OUT="${1:-$HERE/nginx-oracle.json}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

oracle_unavailable() {
    echo "oracle-unavailable: $1" >&2
    exit 5
}

command -v "$NGINX_BIN" >/dev/null 2>&1 || [ -x "$NGINX_BIN" ] || oracle_unavailable "nginx binary not found at $NGINX_BIN"
"$NGINX_BIN" -V 2>&1 | grep -q -- "--with-mail" || oracle_unavailable "nginx is not built --with-mail"
"$NGINX_BIN" -V 2>&1 | grep -q -- "--with-mail_ssl_module" || oracle_unavailable "nginx is not built --with-mail_ssl_module"

PORT=21490
AUTH=21500
BACKEND_POP3=21501
BACKEND_IMAP=21502
BACKEND_SMTP=21503
CERT="$HERE/../full-surface/cert-a.pem"
KEY="$HERE/../full-surface/key-a.pem"

send() { "$PY" - "$@" <<'PYTHON'
import socket, sys
port = int(sys.argv[1])
s = socket.socket(); s.settimeout(0.3)
try:
    s.connect(("127.0.0.1", port)); print("listening")
except OSError:
    print("free")
finally:
    s.close()
PYTHON
}
if [ "$(send "$PORT")" = "listening" ] || [ "$(send "$AUTH")" = "listening" ]; then
    oracle_unavailable "port already in use (proxy=$PORT auth=$AUTH)"
fi

sed -e "s|@AUTH@|$AUTH|g" \
    -e "s|@POP3@|$PORT|g" \
    -e "s|@IMAP@|$((PORT+1))|g" \
    -e "s|@SMTP@|$((PORT+2))|g" \
    -e "s|@SMTP_TLS@|$((PORT+3))|g" \
    -e "s|@SMTP_STARTTLS@|$((PORT+4))|g" \
    -e "s|@CERT@|$CERT|g" \
    -e "s|@KEY@|$KEY|g" \
    "$HERE/nginx.conf" > "$WORK/nginx.conf"

env TZ=UTC "$NGINX_BIN" -c "$WORK/nginx.conf" -p "$WORK/" \
    -g "pid $WORK/nginx.pid; error_log $WORK/error.log notice;" \
    >"$WORK/out.log" 2>&1 &
NGINX_PID=$!
trap 'kill "$NGINX_PID" 2>/dev/null; wait "$NGINX_PID" 2>/dev/null; rm -rf "$WORK"' EXIT

for _ in $(seq 1 40); do
    [ "$(send "$PORT")" = "listening" ] && break
    sleep 0.1
done
[ "$(send "$PORT")" = "listening" ] || { echo "oracle-unavailable: nginx never listened" >&2; tail -5 "$WORK/out.log" >&2; exit 5; }

cd "$HERE"
"$PY" battery.py \
    --pop3 "$PORT" --imap "$((PORT+1))" --smtp "$((PORT+2))" \
    --smtp-tls "$((PORT+3))" --smtp-starttls "$((PORT+4))" \
    --auth "$AUTH" \
    --backend-pop3 "$BACKEND_POP3" --backend-imap "$BACKEND_IMAP" --backend-smtp "$BACKEND_SMTP" \
    --server-identity "nginx/1.29.6" \
    --out "$OUT"
