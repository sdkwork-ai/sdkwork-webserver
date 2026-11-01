#!/usr/bin/env bash
# Start real nginx on the mail client-certificate fixture, capture the battery,
# then stop it. Requires an nginx built --with-mail --with-mail_ssl_module.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BASE=${NGINX_DIFF_MAIL_CLIENT_CERT_PORT:-21820}
WORK=${NGINX_DIFF_MAIL_CLIENT_CERT_WORK:-/tmp/ngx-differential-mail-client-cert}
OUT=${NGINX_DIFF_MAIL_CLIENT_CERT_ORACLE_OUT:-$HERE/nginx-oracle.json}
if [ -z "${NGINX_BIN:-}" ]; then
    if [ -x /tmp/nginx-oracle-src/objs/nginx ]; then
        NGINX_BIN=/tmp/nginx-oracle-src/objs/nginx
    elif [ -x /tmp/nginx-deb/root/usr/sbin/nginx ]; then
        NGINX_BIN=/tmp/nginx-deb/root/usr/sbin/nginx
    else
        NGINX_BIN=nginx
    fi
fi
PY=$(command -v python3 || command -v python || true)
[ -n "$PY" ] || { echo "oracle-unavailable: no python on PATH" >&2; exit 3; }
command -v "$NGINX_BIN" >/dev/null 2>&1 || [ -x "$NGINX_BIN" ] || {
    echo "oracle-unavailable: no nginx binary at '$NGINX_BIN'" >&2
    exit 3
}
"$NGINX_BIN" -V 2>&1 | grep -q -- "--with-mail_ssl_module" || {
    echo "oracle-unavailable: nginx is not built --with-mail_ssl_module" >&2
    exit 3
}

mkdir -p "$WORK"
AUTH=$((BASE + 20))
BACKEND=$((BASE + 21))
SERVER_CERT="$HERE/../../full-surface/cert-a.pem"
SERVER_KEY="$HERE/../../full-surface/key-a.pem"

sed \
    -e "s|@SMTP_ON@|$BASE|g" \
    -e "s|@SMTP_ON_PASS_OFF@|$((BASE + 1))|g" \
    -e "s|@SMTP_TRUSTED_ONLY@|$((BASE + 2))|g" \
    -e "s|@SMTP_OPTIONAL@|$((BASE + 3))|g" \
    -e "s|@SMTP_NO_CA@|$((BASE + 4))|g" \
    -e "s|@SMTP_DEPTH0@|$((BASE + 5))|g" \
    -e "s|@SMTP_DEPTH1@|$((BASE + 6))|g" \
    -e "s|@SMTP_DEPTH2@|$((BASE + 7))|g" \
    -e "s|@SMTP_TRUSTED_INTERMEDIATE@|$((BASE + 8))|g" \
    -e "s|@SMTP_STARTTLS@|$((BASE + 9))|g" \
    -e "s|@IMAP_ON@|$((BASE + 10))|g" \
    -e "s|@POP3_ON@|$((BASE + 11))|g" \
    -e "s|@SMTP_VERIFY_OFF@|$((BASE + 12))|g" \
    -e "s|@AUTH@|$AUTH|g" \
    -e "s|@BACKEND@|$BACKEND|g" \
    -e "s|@SERVER_CERT@|$SERVER_CERT|g" \
    -e "s|@SERVER_KEY@|$SERVER_KEY|g" \
    -e "s|@CA@|$HERE/ca.pem|g" \
    -e "s|@INTERMEDIATE_CA@|$HERE/intermediate-ca.pem|g" \
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
    --smtp-on "$BASE" \
    --smtp-on-pass-off "$((BASE + 1))" \
    --smtp-trusted-only "$((BASE + 2))" \
    --smtp-optional "$((BASE + 3))" \
    --smtp-no-ca "$((BASE + 4))" \
    --smtp-depth0 "$((BASE + 5))" \
    --smtp-depth1 "$((BASE + 6))" \
    --smtp-depth2 "$((BASE + 7))" \
    --smtp-trusted-intermediate "$((BASE + 8))" \
    --smtp-starttls "$((BASE + 9))" \
    --imap-on "$((BASE + 10))" \
    --pop3-on "$((BASE + 11))" \
    --smtp-verify-off "$((BASE + 12))" \
    --auth-port "$AUTH" \
    --backend-port "$BACKEND" \
    --ca "$HERE/ca.pem" \
    --client-trusted-cert "$HERE/client-trusted.pem" \
    --client-trusted-key "$HERE/client-trusted.key" \
    --client-untrusted-cert "$HERE/client-untrusted.pem" \
    --client-untrusted-key "$HERE/client-untrusted.key" \
    --client-selfsigned-cert "$HERE/client-selfsigned.pem" \
    --client-selfsigned-key "$HERE/client-selfsigned.key" \
    --client-intermediate-cert "$HERE/client-intermediate.pem" \
    --client-intermediate-chain "$HERE/client-intermediate-chain.pem" \
    --client-intermediate-key "$HERE/client-intermediate.key" \
    --out "$OUT"
