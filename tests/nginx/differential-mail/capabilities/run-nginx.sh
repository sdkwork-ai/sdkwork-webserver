#!/usr/bin/env bash
# Start real nginx on the mail capability/realip fixture, capture the battery,
# then stop it. Plaintext-only: the pinned oracle is built without mail_ssl.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BASE=${NGINX_DIFF_MAIL_CAPS_PORT:-21420}
WORK=${NGINX_DIFF_MAIL_CAPS_WORK:-/tmp/ngx-differential-mail-caps}
OUT=${NGINX_DIFF_MAIL_CAPS_ORACLE_OUT:-$HERE/nginx-oracle.json}
if [ -z "${NGINX_BIN:-}" ]; then
    if [ -x /tmp/nginx-deb/root/usr/sbin/nginx ]; then
        NGINX_BIN=/tmp/nginx-deb/root/usr/sbin/nginx
    else
        NGINX_BIN=nginx
    fi
fi
PY=$(command -v python3 || command -v python || true)
[ -n "$PY" ] || { echo "oracle-unavailable: no python on PATH" >&2; exit 3; }
command -v "$NGINX_BIN" >/dev/null 2>&1 || {
    echo "oracle-unavailable: no nginx binary at '$NGINX_BIN'" >&2
    exit 3
}

mkdir -p "$WORK"
AUTH=$((BASE + 20))
BACKEND=$((BASE + 21))

sed \
    -e "s|@SMTP_DEFAULT@|$BASE|g" \
    -e "s|@SMTP_CUSTOM@|$((BASE + 1))|g" \
    -e "s|@IMAP_DEFAULT@|$((BASE + 2))|g" \
    -e "s|@IMAP_CUSTOM@|$((BASE + 3))|g" \
    -e "s|@POP3_DEFAULT@|$((BASE + 4))|g" \
    -e "s|@POP3_CUSTOM@|$((BASE + 5))|g" \
    -e "s|@SMTP_PROXY_TRUSTED@|$((BASE + 6))|g" \
    -e "s|@SMTP_PROXY_UNTRUSTED@|$((BASE + 7))|g" \
    -e "s|@AUTH@|$AUTH|g" \
    -e "s|@BACKEND@|$BACKEND|g" \
    "$HERE/nginx.conf" >"$WORK/nginx.conf"

probe() {
    "$PY" -c "import socket; socket.create_connection(('127.0.0.1', $1), 1).close()" 2>/dev/null
}
if probe "$BASE"; then
    echo "oracle-unavailable: 127.0.0.1:$BASE is already listening" >&2
    exit 4
fi

env TZ=UTC "$NGINX_BIN" -c "$WORK/nginx.conf" -p "$WORK/" \
    -g "pid $WORK/nginx.pid; error_log $WORK/error.log notice;" \
    >"$WORK/out.log" 2>&1 &
NGINX_PID=$!
trap 'kill "$NGINX_PID" 2>/dev/null; wait "$NGINX_PID" 2>/dev/null' EXIT

for _ in $(seq 1 40); do
    probe "$BASE" && break
    sleep 0.25
done
if ! probe "$BASE"; then
    echo "oracle-unavailable: nginx never listened on $BASE" >&2
    tail -5 "$WORK/out.log" >&2 || true
    exit 5
fi

"$PY" "$HERE/battery.py" \
    --smtp-default "$BASE" \
    --smtp-custom "$((BASE + 1))" \
    --imap-default "$((BASE + 2))" \
    --imap-custom "$((BASE + 3))" \
    --pop3-default "$((BASE + 4))" \
    --pop3-custom "$((BASE + 5))" \
    --smtp-proxy-trusted "$((BASE + 6))" \
    --smtp-proxy-untrusted "$((BASE + 7))" \
    --auth-port "$AUTH" \
    --backend-port "$BACKEND" \
    --out "$OUT"
