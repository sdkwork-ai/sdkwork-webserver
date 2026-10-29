#!/usr/bin/env bash
# Run the differential-protocols fixture against a real nginx oracle, capture
# the battery, and stop the oracle again. Exit codes: 0 captured, 3 tooling
# unavailable, 4 port already taken, 5 nginx never listened.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PORT=${NGINX_DIFF_PROTOCOLS_ORACLE_PORT:-21092}
FCGI=$((PORT + 2))
DEAD=$((PORT + 5))
UWSGI=$((PORT + 10))
SCGI=$((PORT + 11))
MEMCACHED=$((PORT + 14))
WORK=${NGINX_DIFF_PROTOCOLS_WORK:-/tmp/ngx-differential-protocols}
CONF="$WORK/nginx.conf"
OUT=${NGINX_DIFF_PROTOCOLS_ORACLE_OUT:-$HERE/nginx-oracle.json}
if [ -z "${NGINX_BIN:-}" ]; then
    if [ -x /tmp/nginx-deb/root/usr/sbin/nginx ]; then
        NGINX_BIN=/tmp/nginx-deb/root/usr/sbin/nginx
    else
        NGINX_BIN=nginx
    fi
fi

oracle_unavailable() {
    echo "oracle-unavailable: $*" >&2
    exit 3
}

command -v "$NGINX_BIN" >/dev/null 2>&1 || oracle_unavailable "no nginx binary at '$NGINX_BIN'"
PY=$(command -v python3 || command -v python || true)
[ -n "$PY" ] || oracle_unavailable "no python on PATH"

mkdir -p "$WORK"
for sub in client_body proxy fastcgi uwsgi scgi; do
    mkdir -p "$WORK/tmp/$sub"
done
sed \
    -e "s|@ROOT@|$HERE/public|g" \
    -e "s|@PORT@|$PORT|g" \
    -e "s|@FCGI@|$FCGI|g" \
    -e "s|@UWSGI@|$UWSGI|g" \
    -e "s|@SCGI@|$SCGI|g" \
    -e "s|@MEMCACHED@|$MEMCACHED|g" \
    -e "s|@DEAD@|$DEAD|g" \
    "$HERE/nginx.conf" |
    sed "s|^http {|http {\n    client_body_temp_path $WORK/tmp/client_body;\n    proxy_temp_path $WORK/tmp/proxy;\n    fastcgi_temp_path $WORK/tmp/fastcgi;\n    uwsgi_temp_path $WORK/tmp/uwsgi;\n    scgi_temp_path $WORK/tmp/scgi;|" >"$CONF"

probe_port() {
    "$PY" -c "import socket,sys; socket.create_connection(('127.0.0.1', $1), 1).close()" \
        2>/dev/null
}
if probe_port "$PORT" || probe_port "$FCGI" || probe_port "$UWSGI" || probe_port "$SCGI" || probe_port "$MEMCACHED" || probe_port "$DEAD"; then
    oracle_unavailable "a fixture port ($PORT/$FCGI/$UWSGI/$SCGI/$MEMCACHED/$DEAD) is already listening"
fi

"$PY" "$HERE/mock_fastcgi.py" --port "$FCGI" >"$WORK/mock-fastcgi.log" 2>&1 &
MOCK_PID=$!
"$PY" "$HERE/mock_uwsgi.py" --port "$UWSGI" >"$WORK/mock-uwsgi.log" 2>&1 &
MOCK_UWSGI_PID=$!
"$PY" "$HERE/mock_scgi.py" --port "$SCGI" >"$WORK/mock-scgi.log" 2>&1 &
MOCK_SCGI_PID=$!
"$PY" "$HERE/mock_memcached.py" --port "$MEMCACHED" >"$WORK/mock-memcached.log" 2>&1 &
MOCK_MEMCACHED_PID=$!
env TZ=UTC "$NGINX_BIN" -c "$CONF" -p "$WORK/" \
    -g "pid $WORK/nginx.pid; error_log $WORK/error.log notice;" \
    >"$WORK/out.log" 2>&1 &
NGINX_PID=$!
# shellcheck disable=SC2064
trap 'kill "$NGINX_PID" "$MOCK_PID" "$MOCK_UWSGI_PID" "$MOCK_SCGI_PID" "$MOCK_MEMCACHED_PID" 2>/dev/null; wait "$NGINX_PID" "$MOCK_PID" "$MOCK_UWSGI_PID" "$MOCK_SCGI_PID" "$MOCK_MEMCACHED_PID" 2>/dev/null' EXIT

for _ in $(seq 1 40); do
    probe_port "$PORT" && break
    sleep 0.25
done
if ! probe_port "$PORT"; then
    echo "oracle-unavailable: nginx never listened on $PORT" >&2
    tail -5 "$WORK/out.log" >&2 || true
    exit 5
fi

cd "$HERE"
"$PY" battery.py --port "$PORT" --out "$OUT"
