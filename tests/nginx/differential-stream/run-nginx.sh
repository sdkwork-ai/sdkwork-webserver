#!/usr/bin/env bash
# Start a real nginx (the differential stream oracle) on the shared fixture,
# capture the battery, and always stop it again. The upstream echo server is
# owned by `battery.py`; only the listener ports are rendered here.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BASE=${NGINX_DIFF_STREAM_ORACLE_PORT:-21292}
WORK=${NGINX_DIFF_STREAM_WORK:-/tmp/ngx-differential-stream}
CONF="$WORK/nginx.conf"
OUT=${NGINX_DIFF_STREAM_ORACLE_OUT:-$HERE/nginx-oracle.json}
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
UPSTREAM=$((BASE + 20))
UPSTREAM_TLS=$((BASE + 21))
UPSTREAM_HC=$((BASE + 22))
UPSTREAM_TLS2=$((BASE + 23))

# Wave 7E certificates: `proxy_ssl` needs a verified CA-signed upstream identity
# (SAN DNS:tls-stream.test) plus an unrelated CA for the untrusted case. Mint
# once per WORK directory with the openssl on PATH.
OPENSSL=$(command -v openssl || true)
[ -n "$OPENSSL" ] || { echo "oracle-unavailable: no openssl on PATH (required for proxy_ssl)" >&2; exit 3; }
TLS_CA="$WORK/tls-ca.pem"
OTHER_CA="$WORK/tls-other-ca.pem"
TLS_CERT="$WORK/tls-stream.pem"
TLS_KEY="$WORK/tls-stream.key"
if [ ! -f "$TLS_CERT" ] || [ ! -f "$TLS_CA" ] || [ ! -f "$OTHER_CA" ]; then
    "$OPENSSL" req -x509 -newkey rsa:2048 -nodes -keyout "$WORK/tls-ca.key" \
        -out "$TLS_CA" -days 3 -subj /CN=stream-tls-ca >/dev/null 2>&1 || {
        echo "oracle-unavailable: cannot mint the stream TLS CA" >&2; exit 3; }
    "$OPENSSL" req -x509 -newkey rsa:2048 -nodes -keyout "$WORK/tls-other-ca.key" \
        -out "$OTHER_CA" -days 3 -subj /CN=stream-tls-other-ca >/dev/null 2>&1 || {
        echo "oracle-unavailable: cannot mint the unrelated stream TLS CA" >&2; exit 3; }
    cat >"$WORK/tls-stream.ext" <<'EOF'
subjectAltName=DNS:tls-stream.test
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
EOF
    "$OPENSSL" req -newkey rsa:2048 -nodes -keyout "$TLS_KEY" \
        -out "$WORK/tls-stream.csr" -subj /CN=tls-stream.test >/dev/null 2>&1
    "$OPENSSL" x509 -req -in "$WORK/tls-stream.csr" -CA "$TLS_CA" \
        -CAkey "$WORK/tls-ca.key" -CAcreateserial -days 3 \
        -out "$TLS_CERT" -extfile "$WORK/tls-stream.ext" >/dev/null 2>&1 || {
        echo "oracle-unavailable: cannot sign the stream TLS identity" >&2; exit 3; }
fi
sed \
    -e "s|@PORT0@|$BASE|g" \
    -e "s|@PORT1@|$((BASE + 1))|g" \
    -e "s|@PORT2@|$((BASE + 2))|g" \
    -e "s|@PORT3@|$((BASE + 3))|g" \
    -e "s|@PORT4@|$((BASE + 4))|g" \
    -e "s|@PORT5@|$((BASE + 5))|g" \
    -e "s|@PORT6@|$((BASE + 6))|g" \
    -e "s|@PORT7@|$((BASE + 7))|g" \
    -e "s|@PORT8@|$((BASE + 8))|g" \
    -e "s|@PORT9@|$((BASE + 9))|g" \
    -e "s|@PORT10@|$((BASE + 10))|g" \
    -e "s|@PORT11@|$((BASE + 11))|g" \
    -e "s|@UPSTREAM@|$UPSTREAM|g" \
    -e "s|@UPSTREAM_TLS@|$UPSTREAM_TLS|g" \
    -e "s|@UPSTREAM_HC@|$UPSTREAM_HC|g" \
    -e "s|@UPSTREAM_TLS2@|$UPSTREAM_TLS2|g" \
    -e "s|@TLS_CA@|$TLS_CA|g" \
    -e "s|@OTHER_CA@|$OTHER_CA|g" \
    "$HERE/nginx.conf" >"$CONF"

probe() {
    "$PY" -c "import socket; socket.create_connection(('127.0.0.1', $1), 1).close()" 2>/dev/null
}
if probe "$BASE"; then
    echo "oracle-unavailable: 127.0.0.1:$BASE is already listening" >&2
    exit 4
fi

env TZ=UTC "$NGINX_BIN" -c "$CONF" -p "$WORK/" \
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
    --port0 "$BASE" \
    --port1 "$((BASE + 1))" \
    --port2 "$((BASE + 2))" \
    --port3 "$((BASE + 3))" \
    --port4 "$((BASE + 4))" \
    --port5 "$((BASE + 5))" \
    --port6 "$((BASE + 6))" \
    --port7 "$((BASE + 7))" \
    --port8 "$((BASE + 8))" \
    --port9 "$((BASE + 9))" \
    --port10 "$((BASE + 10))" \
    --port11 "$((BASE + 11))" \
    --upstream-port "$UPSTREAM" \
    --tls-echo-port "$UPSTREAM_TLS" \
    --half-close-echo-port "$UPSTREAM_HC" \
    --tls-echo-2-port "$UPSTREAM_TLS2" \
    --tls-cert "$TLS_CERT" \
    --tls-key "$TLS_KEY" \
    --out "$OUT"
