#!/usr/bin/env bash
# Start a real nginx (the differential oracle) on the shared fixture, capture the
# battery, and always stop it again.
#
# Portable on purpose: it uses whatever `nginx` is on PATH, so the same file
# serves a WSL invocation from Windows and a native Linux CI runner. It is a file
# rather than an inline command because matching the conf path with `pkill -f`
# also matches the caller's own command line and would kill the caller's shell.
#
# The fixture is `daemon off; master_process off;`, so nginx runs as one
# foreground process and this shell's `$!` names it exactly. The PID comes from
# `$!` and a trap releases it: an earlier revision only looked for a leaked
# process on its *next* start, so every run leaked the server and left the port
# bound. `-g pid ...` is still required — against the default `/run/nginx.pid`
# an unprivileged nginx exits with EACCES before it ever listens.
#
# Exit codes: 0 captured, 3 tooling unavailable, 4 port already taken,
#             5 nginx never listened.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PORT=${NGINX_DIFF_ORACLE_PORT:-20990}
WORK=${NGINX_DIFF_WORK:-/tmp/ngx-differential}
CONF="$WORK/nginx.conf"
OUT=${NGINX_DIFF_ORACLE_OUT:-$HERE/nginx-oracle.json}

oracle_unavailable() {
    echo "oracle-unavailable: $*" >&2
    exit 3
}

command -v nginx >/dev/null 2>&1 || oracle_unavailable "no nginx on PATH"
PY=$(command -v python3 || command -v python || true)
[ -n "$PY" ] || oracle_unavailable "no python on PATH"

mkdir -p "$WORK"
sed -e "s|@PUBLIC@|$HERE/public|g" -e "s|@PORT@|$PORT|g" "$HERE/nginx.conf" >"$CONF"

probe() {
    "$PY" -c "import socket,sys; socket.create_connection(('127.0.0.1', $PORT), 1).close()" \
        2>/dev/null
}

# A leaked single-process nginx from an interrupted run would answer instead of
# the one started below, so a busy port is reported rather than silently reused.
if probe; then
    oracle_unavailable "127.0.0.1:$PORT is already listening"
fi

env TZ=UTC nginx -c "$CONF" -p "$WORK/" -g "pid $WORK/nginx.pid;" >"$WORK/out.log" 2>&1 &
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
"$PY" battery.py --port "$PORT" --out "$OUT"
