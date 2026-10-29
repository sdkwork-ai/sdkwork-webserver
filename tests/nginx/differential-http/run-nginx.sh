#!/usr/bin/env bash
# Start a real nginx (the differential HTTP oracle) on the shared fixture,
# capture the battery, and always stop it again.
#
# Portable on purpose: it uses `$NGINX_BIN` (default `nginx` on PATH), so the
# same file serves a WSL invocation from Windows and a native Linux runner.
# The pinned reference build is nginx 1.29.6; a package binary extracted from
# nginx.org's noble .deb works unprivileged when the http temp paths and the
# pid/error-log locations are redirected into $WORK (the fixture only sets
# `daemon off; master_process off;`).
#
# The fixture is one foreground nginx process, so this shell's `$!` names it
# exactly and the trap releases it. A busy listen port is reported instead of
# being silently reused: a leaked nginx from an interrupted run would answer
# instead of the one started below.
#
# Exit codes: 0 captured, 3 tooling unavailable, 4 port already taken,
#             5 nginx never listened.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PORT=${NGINX_DIFF_HTTP_ORACLE_PORT:-20992}
UPSTREAM=$((PORT + 2))
DEAD=$((PORT + 5))
WORK=${NGINX_DIFF_HTTP_WORK:-/tmp/ngx-differential-http}
CONF="$WORK/nginx.conf"
OUT=${NGINX_DIFF_HTTP_ORACLE_OUT:-$HERE/nginx-oracle.json}
# The pinned reference build: an extracted nginx.org .deb in this environment,
# otherwise a native `nginx` on PATH.
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
# Redirect the http temp paths into $WORK: an unprivileged package binary
# otherwise resolves its compile-time defaults under /var/cache/nginx. These
# directives never affect the response contract the battery records.
for sub in client_body proxy fastcgi uwsgi scgi; do
    mkdir -p "$WORK/tmp/$sub"
done
sed \
    -e "s|@PUBLIC@|$HERE/public|g" \
    -e "s|@PORT@|$PORT|g" \
    -e "s|@UPSTREAM@|$UPSTREAM|g" \
    -e "s|@DEAD@|$DEAD|g" \
    -e "s|@HTPASSWD@|$HERE/htpasswd|g" \
    "$HERE/nginx.conf" |
    sed "s|^http {|http {\n    client_body_temp_path $WORK/tmp/client_body;\n    proxy_temp_path $WORK/tmp/proxy;\n    fastcgi_temp_path $WORK/tmp/fastcgi;\n    uwsgi_temp_path $WORK/tmp/uwsgi;\n    scgi_temp_path $WORK/tmp/scgi;|" >"$CONF"
# `include maps.fragment;` resolves against the rendered configuration's own
# directory ($WORK), exactly like nginx's conf_prefix.
cp "$HERE/maps.fragment" "$WORK/maps.fragment"
# Inject the empty-string-key maps (the JS config parser cannot tokenise them).
# `r` appends the file after the marker line, then the marker comment is removed.
sed -i "\|# @EMPTY_KEY_MAP@|r $HERE/maps-empty-key.inc" "$CONF"
sed -i "\|# @EMPTY_KEY_MAP@|d" "$CONF"

probe_port() {
    "$PY" -c "import socket,sys; socket.create_connection(('127.0.0.1', $1), 1).close()" \
        2>/dev/null
}

probe() {
    probe_port "$PORT"
}

if probe; then
    oracle_unavailable "127.0.0.1:$PORT is already listening"
fi
# The battery binds the mock upstream, and the 502 case needs a port nothing
# listens on. Either one being busy would silently measure a different server.
if probe_port "$UPSTREAM"; then
    oracle_unavailable "127.0.0.1:$UPSTREAM (mock upstream) is already listening"
fi
if probe_port "$DEAD"; then
    oracle_unavailable "127.0.0.1:$DEAD (dead upstream) is already listening"
fi

env TZ=UTC "$NGINX_BIN" -c "$CONF" -p "$WORK/" \
    -g "pid $WORK/nginx.pid; error_log $WORK/error.log notice;" \
    >"$WORK/out.log" 2>&1 &
NGINX_PID=$!
# shellcheck disable=SC2064  # expand the PID now, not at trap time
trap 'kill "$NGINX_PID" 2>/dev/null; wait "$NGINX_PID" 2>/dev/null' EXIT

for _ in $(seq 1 40); do
    probe && break
    sleep 0.25
done

if ! probe; then
    echo "oracle-unavailable: nginx never listened on $PORT" >&2
    tail -5 "$WORK/out.log" >&2 || true
    exit 5
fi

cd "$HERE"
"$PY" battery.py \
    --port "$PORT" \
    --upstream-port "$UPSTREAM" \
    --dead-port "$DEAD" \
    --out "$OUT"
