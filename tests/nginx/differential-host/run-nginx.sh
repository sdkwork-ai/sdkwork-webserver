#!/usr/bin/env bash
# Start a real nginx (the host-selection differential oracle), capture the
# battery, and always stop it again.
#
# The fixture is `daemon off; master_process off;`, so nginx runs as one
# foreground process and this shell's `$!` names it exactly. The PID comes from
# `$!` and a trap releases it: a leaked server would otherwise answer the next
# run (and could even answer for the gateway port).
#
# The fixture uses the default compile-time temp/log paths under
# /var/cache/nginx and /var/log/nginx, which an unprivileged package binary
# cannot create. The http temp paths and the startup error log are redirected
# into $WORK, and TZ is pinned to UTC, exactly like
# tests/nginx/differential/run-nginx.sh. These directives never affect the
# response contract the battery records.
#
# Exit codes: 0 captured, 3 tooling unavailable, 4 port already taken,
#             5 nginx never listened.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PORT=${NGINX_HOST_ORACLE_PORT:-21010}
MOCK_PORT=${NGINX_HOST_MOCK_PORT:-21012}
WORK=${NGINX_HOST_WORK:-/tmp/ngx-differential-host}
OUT=${NGINX_HOST_ORACLE_OUT:-$HERE/nginx-oracle.json}

oracle_unavailable() {
    echo "oracle-unavailable: $*" >&2
    exit 3
}

resolve_nginx() {
    if [ -n "${NGINX_BIN:-}" ]; then
        command -v "$NGINX_BIN" >/dev/null 2>&1 && NGINX_BIN=$(command -v "$NGINX_BIN")
        [ -x "$NGINX_BIN" ] && { echo "$NGINX_BIN"; return 0; }
        return 1
    fi
    if command -v nginx >/dev/null 2>&1; then
        command -v nginx
        return 0
    fi
    # The pinned real binary extracted from the nginx.org noble .deb.
    for candidate in /tmp/nginx-deb/root/usr/sbin/nginx; do
        [ -x "$candidate" ] && { echo "$candidate"; return 0; }
    done
    return 1
}

NGINX_BIN=$(resolve_nginx) || oracle_unavailable "no nginx binary (set NGINX_BIN or install the pinned 1.29.6 package)"
PY=$(command -v python3 || command -v python || true)
[ -n "$PY" ] || oracle_unavailable "no python on PATH"

mkdir -p "$WORK"
for sub in client_body proxy fastcgi uwsgi scgi; do
    mkdir -p "$WORK/tmp/$sub"
done
sed -e "s|@PUBLIC@|$HERE/public|g" \
    -e "s|@PORT@|$PORT|g" \
    -e "s|@MOCK@|$MOCK_PORT|g" "$HERE/nginx.conf" |
    sed "s|^http {|http {\n    client_body_temp_path $WORK/tmp/client_body;\n    proxy_temp_path $WORK/tmp/proxy;\n    fastcgi_temp_path $WORK/tmp/fastcgi;\n    uwsgi_temp_path $WORK/tmp/uwsgi;\n    scgi_temp_path $WORK/tmp/scgi;|" >"$WORK/nginx.conf"

probe() {
    "$PY" -c "import socket,sys; socket.create_connection(('127.0.0.1', $1), 1).close()" \
        2>/dev/null
}

for busy in "$PORT" "$MOCK_PORT"; do
    if probe "$busy"; then
        oracle_unavailable "127.0.0.1:$busy is already listening"
    fi
done

env TZ=UTC "$NGINX_BIN" -c "$WORK/nginx.conf" -p "$WORK/" \
    -g "pid $WORK/nginx.pid; error_log $WORK/error.log notice;" >"$WORK/out.log" 2>&1 &
NGINX_PID=$!
# shellcheck disable=SC2064  # expand the PID now, not at trap time
trap 'kill "$NGINX_PID" 2>/dev/null; wait "$NGINX_PID" 2>/dev/null' EXIT

for _ in $(seq 1 40); do
    probe "$PORT" && break
    sleep 0.25
done

if ! probe "$PORT"; then
    echo "oracle-unavailable: nginx never listened on $PORT" >&2
    tail -5 "$WORK/out.log" >&2 || true
    exit 5
fi

cd "$HERE"
"$PY" battery.py --port "$PORT" --mock-port "$MOCK_PORT" --out "$OUT"
