#!/usr/bin/env bash
# Start real nginx on the extra stream fixture (split_clients + pass), capture
# the battery, and always stop it again. No TLS material is needed: this fixture
# uses only directives compiled into the pinned 1.29.6 oracle.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BASE=${NGINX_DIFF_STREAM_EXTRA_PORT:-21320}
WORK=${NGINX_DIFF_STREAM_EXTRA_WORK:-/tmp/ngx-differential-stream-extra}
OUT=${NGINX_DIFF_STREAM_EXTRA_ORACLE_OUT:-$HERE/nginx-oracle.json}
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
UPSTREAM=$((BASE + 10))

sed \
    -e "s|@SPLIT@|$BASE|g" \
    -e "s|@PASS_TARGET@|$((BASE + 1))|g" \
    -e "s|@PASS@|$((BASE + 2))|g" \
    -e "s|@PASS_PROXY_TARGET@|$((BASE + 3))|g" \
    -e "s|@PASS_PROXY@|$((BASE + 4))|g" \
    -e "s|@UPSTREAM@|$UPSTREAM|g" \
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
    --split-port "$BASE" \
    --pass-target-port "$((BASE + 1))" \
    --pass-port "$((BASE + 2))" \
    --pass-proxy-target-port "$((BASE + 3))" \
    --pass-proxy-port "$((BASE + 4))" \
    --upstream-port "$UPSTREAM" \
    --out "$OUT"
