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
GRPC=$((PORT + 20))
GRPC_TLS_OK=$((PORT + 21))
GRPC_TLS_UNTRUSTED=$((PORT + 22))
UNIX_PORT=$((PORT + 30))
WORK=${NGINX_DIFF_PROTOCOLS_WORK:-/tmp/ngx-differential-protocols}
UNIX_ROOT_DIR="$WORK/unix"
FCGI_SOCKET="$UNIX_ROOT_DIR/fcgi.sock"
UWSGI_SOCKET="$UNIX_ROOT_DIR/uwsgi.sock"
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
OPENSSL=$(command -v openssl || true)
[ -n "$OPENSSL" ] || oracle_unavailable "no openssl on PATH (required to mint the gRPC mock certificate)"
REPO_ROOT=$(cd "$HERE/../../.." && pwd)
mkdir -p "$WORK"
GRPC_MOCK="$REPO_ROOT/target/debug/sdkwork-webserver-grpc-mock"
if [ ! -x "$GRPC_MOCK" ]; then
    (cd "$REPO_ROOT" && cargo build -p sdkwork-webserver-grpc-mock >"$WORK/grpc-mock-build.log" 2>&1) \
        || oracle_unavailable "cannot build the Rust gRPC mock responder"
fi
[ -x "$GRPC_MOCK" ] || oracle_unavailable "gRPC mock responder binary is missing"

mkdir -p "$WORK" "$UNIX_ROOT_DIR"
rm -f "$FCGI_SOCKET" "$UWSGI_SOCKET"
for sub in client_body proxy fastcgi uwsgi scgi; do
    mkdir -p "$WORK/tmp/$sub"
done
# gRPC TLS material: a CA, an unrelated CA for the negative verification case,
# and a CA-signed server certificate with the mock's SAN.
"$OPENSSL" req -x509 -newkey rsa:2048 -nodes -keyout "$WORK/grpc-ca.key" \
    -out "$WORK/grpc-ca.pem" -days 3 -subj "/CN=grpc-test-ca" >/dev/null 2>&1 \
    || oracle_unavailable "cannot generate the gRPC mock CA"
"$OPENSSL" req -x509 -newkey rsa:2048 -nodes -keyout "$WORK/grpc-other-ca.key" \
    -out "$WORK/grpc-other-ca.pem" -days 3 -subj "/CN=grpc-other-ca" >/dev/null 2>&1 \
    || oracle_unavailable "cannot generate the unrelated gRPC CA"
"$OPENSSL" req -newkey rsa:2048 -nodes -keyout "$WORK/grpc-server.key" \
    -out "$WORK/grpc-server.csr" -subj "/CN=localhost" >/dev/null 2>&1 \
    || oracle_unavailable "cannot generate the gRPC server key"
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n' >"$WORK/grpc-server.ext"
"$OPENSSL" x509 -req -in "$WORK/grpc-server.csr" -CA "$WORK/grpc-ca.pem" \
    -CAkey "$WORK/grpc-ca.key" -CAcreateserial -days 3 -out "$WORK/grpc-server.pem" \
    -extfile "$WORK/grpc-server.ext" >/dev/null 2>&1 \
    || oracle_unavailable "cannot sign the gRPC server certificate"
sed \
    -e "s|@UNIX_INCLUDE@|$WORK/unix-server.conf|g" \
    -e "s|@UNIX_PORT@|$UNIX_PORT|g" \
    -e "s|@FCGI_SOCKET@|$FCGI_SOCKET|g" \
    -e "s|@UWSGI_SOCKET@|$UWSGI_SOCKET|g" \
    -e "s|@ROOT@|$HERE/public|g" \
    -e "s|@PORT@|$PORT|g" \
    -e "s|@FCGI@|$FCGI|g" \
    -e "s|@UWSGI@|$UWSGI|g" \
    -e "s|@SCGI@|$SCGI|g" \
    -e "s|@MEMCACHED@|$MEMCACHED|g" \
    -e "s|@GRPC@|$GRPC|g" \
    -e "s|@GRPC_TLS_OK@|$GRPC_TLS_OK|g" \
    -e "s|@GRPC_TLS_UNTRUSTED@|$GRPC_TLS_UNTRUSTED|g" \
    -e "s|@DEAD@|$DEAD|g" \
    "$HERE/nginx.conf" |
    sed "s|^http {|http {\n    client_body_temp_path $WORK/tmp/client_body;\n    proxy_temp_path $WORK/tmp/proxy;\n    fastcgi_temp_path $WORK/tmp/fastcgi;\n    uwsgi_temp_path $WORK/tmp/uwsgi;\n    scgi_temp_path $WORK/tmp/scgi;|" >"$CONF.main"
awk -v unix_file="$WORK/unix-server.conf" '
    /# @UNIX_SERVER_INJECT@/ { while ((getline line < unix_file) > 0) print line; close(unix_file); next }
    { print }
' "$CONF.main" >"$CONF"
rm -f "$CONF.main"

sed \
    -e "s|@UNIX_PORT@|$UNIX_PORT|g" \
    -e "s|@FCGI_SOCKET@|$FCGI_SOCKET|g" \
    -e "s|@UWSGI_SOCKET@|$UWSGI_SOCKET|g" \
    -e "s|@ROOT@|$HERE/public|g" \
    "$HERE/unix-server.conf" >"$WORK/unix-server.conf"

probe_port() {
    "$PY" -c "import socket,sys; socket.create_connection(('127.0.0.1', $1), 1).close()" \
        2>/dev/null
}
if probe_port "$PORT" || probe_port "$UNIX_PORT" || probe_port "$FCGI" || probe_port "$UWSGI" || probe_port "$SCGI" || probe_port "$MEMCACHED" || probe_port "$GRPC" || probe_port "$GRPC_TLS_OK" || probe_port "$GRPC_TLS_UNTRUSTED" || probe_port "$DEAD"; then
    oracle_unavailable "a fixture port ($PORT/$UNIX_PORT/$FCGI/$UWSGI/$SCGI/$MEMCACHED/$GRPC/$GRPC_TLS_OK/$GRPC_TLS_UNTRUSTED/$DEAD) is already listening"
fi

"$PY" "$HERE/mock_fastcgi.py" --port "$FCGI" >"$WORK/mock-fastcgi.log" 2>&1 &
MOCK_PID=$!
"$PY" "$HERE/mock_uwsgi.py" --port "$UWSGI" >"$WORK/mock-uwsgi.log" 2>&1 &
MOCK_UWSGI_PID=$!
"$PY" "$HERE/mock_scgi.py" --port "$SCGI" >"$WORK/mock-scgi.log" 2>&1 &
MOCK_SCGI_PID=$!
"$PY" "$HERE/mock_memcached.py" --port "$MEMCACHED" >"$WORK/mock-memcached.log" 2>&1 &
MOCK_MEMCACHED_PID=$!
"$PY" "$HERE/mock_fastcgi.py" --unix-socket "$FCGI_SOCKET" >"$WORK/mock-fastcgi-unix.log" 2>&1 &
MOCK_FCGI_UNIX_PID=$!
"$PY" "$HERE/mock_uwsgi.py" --unix-socket "$UWSGI_SOCKET" >"$WORK/mock-uwsgi-unix.log" 2>&1 &
MOCK_UWSGI_UNIX_PID=$!
"$GRPC_MOCK" --plain-port "$GRPC" \
    --tls-port "$GRPC_TLS_OK" --tls-port "$GRPC_TLS_UNTRUSTED" \
    --cert "$WORK/grpc-server.pem" --key "$WORK/grpc-server.key" \
    >"$WORK/mock-grpc.log" 2>&1 &
MOCK_GRPC_PID=$!
for _ in $(seq 1 40); do
    [ -S "$FCGI_SOCKET" ] && [ -S "$UWSGI_SOCKET" ] && break
    sleep 0.1
done
env TZ=UTC "$NGINX_BIN" -c "$CONF" -p "$WORK/" \
    -g "pid $WORK/nginx.pid; error_log $WORK/error.log notice;" \
    >"$WORK/out.log" 2>&1 &
NGINX_PID=$!
# shellcheck disable=SC2064
trap 'kill "$NGINX_PID" "$MOCK_PID" "$MOCK_UWSGI_PID" "$MOCK_SCGI_PID" "$MOCK_MEMCACHED_PID" "$MOCK_GRPC_PID" "$MOCK_FCGI_UNIX_PID" "$MOCK_UWSGI_UNIX_PID" 2>/dev/null; wait "$NGINX_PID" "$MOCK_PID" "$MOCK_UWSGI_PID" "$MOCK_SCGI_PID" "$MOCK_MEMCACHED_PID" "$MOCK_GRPC_PID" "$MOCK_FCGI_UNIX_PID" "$MOCK_UWSGI_UNIX_PID" 2>/dev/null' EXIT

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
"$PY" battery.py --port "$PORT" --unix-port "$UNIX_PORT" --out "$OUT"
