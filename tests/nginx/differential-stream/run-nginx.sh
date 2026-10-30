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
UPSTREAM=$((BASE + 10))
sed \
    -e "s|@PORT0@|$BASE|g" \
    -e "s|@PORT1@|$((BASE + 1))|g" \
    -e "s|@PORT2@|$((BASE + 2))|g" \
    -e "s|@PORT3@|$((BASE + 3))|g" \
    -e "s|@PORT4@|$((BASE + 4))|g" \
    -e "s|@PORT5@|$((BASE + 5))|g" \
    -e "s|@PORT6@|$((BASE + 6))|g" \
    -e "s|@PORT7@|$((BASE + 7))|g" \
    -e "s|@UPSTREAM@|$UPSTREAM|g" \
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
    --upstream-port "$UPSTREAM" \
    --out "$OUT"
