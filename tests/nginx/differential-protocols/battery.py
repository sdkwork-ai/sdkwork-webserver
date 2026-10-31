"""Request matrix for the differential upstream-protocol slice.

The battery sends the same raw HTTP/1.1 requests to the real-nginx oracle and
to the Rust data plane and records the observable response contract: status, the
canonical header map and the decoded body. `Date`/`Server`/hop-by-hop fields and
the runtime's security headers are excluded because they are deployment
differences documented by the broader HTTP slice, not part of the FastCGI
contract. Everything the FastCGI responder echoed (PARAMS and raw STDIN) stays
in the body and must match byte for byte.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import socket
import stat
import threading
import time

HOST = "fcgi.example.com"
IGNORED_RESPONSE_HEADERS = {
    "date",
    "server",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "x-content-type-options",
    "strict-transport-security",
    # The runtime's static-delivery contract declares `public, no-cache` for a
    # representation with no `expires` (the same documented difference the broad
    # HTTP slice allowlists); nginx has no equivalent default.
    "cache-control",
    # hyper can only write HTTP/1.1 trailers for fields declared in a `Trailer`
    # response header; nginx writes arbitrary trailers without declaring them.
    # The trailers themselves are compared; only the declaration differs.
    "trailer",
}

def grpc_frame(payload: bytes) -> bytes:
    """One uncompressed gRPC message: flag byte + big-endian length + payload."""
    return b"\x00" + len(payload).to_bytes(4, "big") + payload


CASES = [
    ("basic-get", "GET", "/echo/hello?x=1", {"X-Sent": "alpha"}, None),
    ("args-get", "GET", "/echo/args?name=value&flag", {"X-Sent": "beta"}, None),
    (
        "post-upload",
        "POST",
        "/echo/upload?q=2",
        {"X-Sent": "gamma", "Content-Type": "text/plain"},
        b"hello-upload",
    ),
    ("split-info", "GET", "/split/info.php/extra/path?a=b", {"X-Sent": "delta"}, None),
    ("split-nomatch", "GET", "/split/plain", {"X-Sent": "epsilon"}, None),
    ("dir-index", "GET", "/dir/", {"X-Sent": "zeta"}, None),
    ("headers-policy", "GET", "/headers", {"X-Sent": "eta"}, None),
    ("intercept-404", "GET", "/errors/missing", {"X-Sent": "theta"}, None),
    ("basic-get-again", "GET", "/echo/second?again=1", {"X-Sent": "mu"}, None),
    ("status-503", "GET", "/echo/status/503", {"X-Sent": "iota"}, None),
    ("retry-upstream", "GET", "/retry/ok", {"X-Sent": "kappa"}, None),
    ("head", "HEAD", "/echo/head", {"X-Sent": "lambda"}, None),
    ("uwsgi-basic", "GET", "/uwsgi/hello?x=1", {"X-Sent": "nu"}, None),
    ("uwsgi-post", "POST", "/uwsgi/upload?q=2", {"X-Sent": "xi", "Content-Type": "text/plain"}, b"hello-uwsgi"),
    ("uwsgi-auto-headers", "GET", "/uwsgi/headers", {"X-Sent": "omicron", "X-Normal": "yes"}, None),
    ("uwsgi-intercept-404", "GET", "/uwsgi/errors/missing", {"X-Sent": "pi"}, None),
    ("scgi-basic", "GET", "/scgi/hello?x=1", {"X-Sent": "rho"}, None),
    ("scgi-post", "POST", "/scgi/upload?q=2", {"X-Sent": "sigma", "Content-Type": "text/plain"}, b"hello-scgi"),
    ("scgi-auto-headers", "GET", "/scgi/headers", {"X-Sent": "tau", "X-Normal": "yes"}, None),
    ("scgi-intercept-404", "GET", "/scgi/errors/missing", {"X-Sent": "upsilon"}, None),
    ("memcached-hit", "GET", "/mc/hit", {}, None),
    ("memcached-head", "HEAD", "/mc/head", {}, None),
    ("memcached-large", "GET", "/mc/large", {}, None),
    ("memcached-miss", "GET", "/mc/missing", {}, None),
    ("memcached-raw-502", "GET", "/mc-raw/error", {}, None),
    ("memcached-post", "POST", "/mc/hit", {"Content-Type": "text/plain"}, b"ignored"),
    # ---- gRPC over HTTP/2 (ngx_http_grpc_module) ----
    (
        "grpc-unary",
        "POST",
        "/grpc.test.Echo/Unary",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-error-trailers-only",
        "POST",
        "/grpc.test.Echo/Error",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-status-error",
        "POST",
        "/grpc.test.Echo/StatusError",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-stream",
        "POST",
        "/grpc.test.Echo/Stream",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-set-header",
        "POST",
        "/grpc.test.Echo/Headers",
        {"Content-Type": "application/grpc", "TE": "trailers", "X-Sent": "client-sent", "X-Client": "drop-me"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-hide-pass",
        "POST",
        "/grpc.test.Echo/Hide",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-bad-trailers",
        "POST",
        "/grpc.test.Echo/BadTrailers",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-read-timeout",
        "POST",
        "/grpc.test.Echo/Slow",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-tls-ok",
        "POST",
        "/grpc.test.Echo/TlsOk",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-tls-untrusted",
        "POST",
        "/grpc.test.Echo/TlsUntrusted",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-retry",
        "POST",
        "/grpc.test.Echo/Retry",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-dead",
        "POST",
        "/grpc.test.Echo/Dead",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    (
        "grpc-intercept-http",
        "POST",
        "/grpc.test.Echo/InterceptHttp",
        {"Content-Type": "application/grpc", "TE": "trailers"},
        grpc_frame(b"ping"),
    ),
    # ---- Wave 7A transport: source-address binding (mock echoes the accepted
    # peer) and `fastcgi_keep_conn` connection reuse ----
    ("fastcgi-bind-peer", "GET", "/bind/peer", {"X-Sent": "bind"}, None),
    ("uwsgi-bind-peer", "GET", "/bind-uwsgi/peer", {"X-Sent": "bind-uwsgi"}, None),
    ("scgi-bind-peer", "GET", "/bind-scgi/peer", {"X-Sent": "bind-scgi"}, None),
    ("memcached-bind-peer", "GET", "/bind-cache/bind", {}, None),
    ("fastcgi-keepconn-reuse", "SPECIAL", "", {}, None),
    # ---- Wave 7B: uWSGI upstream TLS (`uwsgi_pass suwsgi://…`) ----
    # The mock's server certificate only carries `DNS:uwsgi-tls.test`, so the
    # trusted-CA case proves `uwsgi_ssl_name` is used for SNI/verification; the
    # mTLS case adds the CA-signed client identity; the untrusted case uses an
    # unrelated CA and must fail closed on both sides (status-only body
    # allowance: nginx renders its own 502 page).
    ("uwsgi-tls-ok", "GET", "/uwsgi-tls/hello?x=1", {"X-Sent": "tls-alpha"}, None),
    ("uwsgi-tls-mtls", "GET", "/uwsgi-tls-mtls/hello?x=1", {"X-Sent": "tls-beta"}, None),
    ("uwsgi-tls-untrusted", "GET", "/uwsgi-tls-untrusted/hello", {"X-Sent": "tls-gamma"}, None),

    # ---- Wave 7C: CGI response-cache sequences (SPECIAL rows) ----
    ("cache-hit", "SPECIAL", "", {}, None),
    ("cache-default", "SPECIAL", "", {}, None),
    ("cache-bypass", "SPECIAL", "", {}, None),
    ("cache-no-cache", "SPECIAL", "", {}, None),
    ("cache-valid", "SPECIAL", "", {}, None),
    ("cache-key", "SPECIAL", "", {}, None),
    ("cache-min-uses", "SPECIAL", "", {}, None),
    ("cache-lock", "SPECIAL", "", {}, None),
    ("cache-stale", "SPECIAL", "", {}, None),
    ("cache-revalidate", "SPECIAL", "", {}, None),
    ("cache-convert-head", "SPECIAL", "", {}, None),
    ("cache-range", "SPECIAL", "", {}, None),
    ("cache-range-default", "SPECIAL", "", {}, None),
    ("cache-range-beyond", "SPECIAL", "", {}, None),
    ("cache-uwsgi-hit", "SPECIAL", "", {}, None),
    ("cache-scgi-hit", "SPECIAL", "", {}, None),

    # ---- Wave 7D: response buffering policy, atomic store, limit_rate,
    # force_ranges, ignore_headers and memcached flag handling. Each SPECIAL
    # row records the client contract plus the side effects that only the
    # battery can observe (stored files, pacing window, multipart boundary). ----
    ("d7-store", "SPECIAL", "", {}, None),
    ("d7-rate", "SPECIAL", "", {}, None),
    ("d7-range-single", "SPECIAL", "", {}, None),
    ("d7-range-suffix", "SPECIAL", "", {}, None),
    ("d7-range-multipart", "SPECIAL", "", {}, None),
    ("d7-range-invalid", "SPECIAL", "", {}, None),
    ("d7-range-off", "SPECIAL", "", {}, None),
    ("d7-bufsize", "SPECIAL", "", {}, None),
    ("d7-ignore", "SPECIAL", "", {}, None),
    ("d7-passbody", "SPECIAL", "", {}, None),
    ("d7-memcached-gzip-on", "SPECIAL", "", {}, None),
    ("d7-memcached-gzip-off", "SPECIAL", "", {}, None),
]


def read_all(stream: socket.socket) -> bytes:
    data = b""
    while True:
        chunk = stream.recv(65536)
        if not chunk:
            return data
        data += chunk


def decode_chunked(raw: bytes) -> tuple[bytes, dict[str, str]]:
    output = b""
    trailers: dict[str, str] = {}
    index = 0
    while True:
        newline = raw.index(b"\r\n", index)
        size = int(raw[index:newline].split(b";")[0], 16)
        index = newline + 2
        if size == 0:
            # The remaining lines after the terminal chunk are the trailer
            # section (`grpc-status`, `grpc-message`, ...).
            for line in raw[index:].split(b"\r\n"):
                name, separator, value = line.partition(b":")
                if separator:
                    trailers[name.strip().lower().decode("latin-1")] = (
                        value.strip().decode("latin-1")
                    )
            return output, trailers
        output += raw[index : index + size]
        index += size + 2



def normalize_grpc_ports(payload: bytes, base_port: int) -> bytes:
    """Replace the fixture's own gRPC responder ports with tokens.

    The oracle and the gateway always run on different ports (the two endpoints
    must never share a socket), but the port is not part of the upstream contract:
    the mock echoes `:authority`, so it has to compare equal on both sides.
    """
    for offset, token in (
        (20, b":<grpc>"),
        (21, b":<grpc-tls>"),
        (22, b":<grpc-tls-untrusted>"),
    ):
        payload = payload.replace(b":" + str(base_port + offset).encode(), token)
    return payload


def request(
    port: int,
    method: str,
    path: str,
    headers: dict,
    body: bytes | None,
    base_port: int,
    host: str = HOST,
):
    lines = [f"{method} {path} HTTP/1.1", f"Host: {host}", "Connection: close"]
    for name, value in headers.items():
        lines.append(f"{name}: {value}")
    if body is not None:
        lines.append(f"Content-Length: {len(body)}")
    raw = ("\r\n".join(lines) + "\r\n\r\n").encode()
    if body is not None:
        raw += body
    with socket.create_connection(("127.0.0.1", port), 5) as connection:
        connection.sendall(raw)
        response = read_all(connection)
    head, separator, payload = response.partition(b"\r\n\r\n")
    if not separator:
        raise AssertionError(f"no response for {method} {path}")
    lines = head.decode("latin-1").split("\r\n")
    status = int(lines[0].split(" ")[1])
    server_line = None
    raw_headers: dict[str, list[str]] = {}
    for line in lines[1:]:
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        if name.strip().lower() == "server":
            server_line = value.strip()
        raw_headers.setdefault(name.strip().lower(), []).append(value.strip())
    response_headers = {
        name: values
        for name, values in raw_headers.items()
        if name not in IGNORED_RESPONSE_HEADERS
    }
    trailers: dict[str, str] = {}
    if "chunked" in (raw_headers.get("transfer-encoding") or []):
        payload, trailers = decode_chunked(payload)
    elif "content-length" in raw_headers:
        length = int(raw_headers["content-length"][0])
        payload = payload[:length]
    normalized_headers = {
        name: sorted(normalize_grpc_ports(value.encode("latin-1"), base_port).decode("latin-1") for value in values)
        for name, values in response_headers.items()
    }
    normalized_trailers = {
        name: normalize_grpc_ports(value.encode("latin-1"), base_port).decode("latin-1")
        for name, value in trailers.items()
    }
    return ({
        "status": status,
        "headers": normalized_headers,
        "trailers": normalized_trailers,
        "body": normalize_grpc_ports(payload, base_port).decode("utf-8", "replace"),
    }, server_line)


# Typed-plane unix-socket transport cases. Both endpoints run the same
# `fastcgi_pass unix:` / `uwsgi_pass unix:` pair (the oracle through nginx
# configuration, the gateway through a typed JSON configuration confined by
# `nginx.unixSocketRoot`). They are sent to a second listener port so the
# single-port contract of the rest of the battery is unchanged.
UNIX_CASES = [
    ("unix-fastcgi", "GET", "/unix-fcgi/hello?x=1", {"X-Sent": "unix-fcgi"}, None),
    ("unix-uwsgi", "GET", "/unix-uwsgi/hello?x=1", {"X-Sent": "unix-uwsgi"}, None),
]
UNIX_HOST = "unix.example.com"


def normalize_multipart_boundaries(result: dict) -> dict:
    """Replace nginx's random multipart boundary with a token in both the
    `Content-Type` header and the body so two independent 206 responses stay
    comparable (the boundary is opaque to clients)."""
    body = result.get("body", "")
    boundaries = []
    for values in result.get("headers", {}).values():
        for value in values:
            match = re.search(r"boundary=([A-Za-z0-9]+)", value)
            if match:
                boundaries.append(match.group(1))
    for boundary in boundaries:
        body = body.replace(boundary, "<boundary>")
    if boundaries:
        for name, values in result.get("headers", {}).items():
            if name == "content-type":
                result["headers"][name] = [
                    re.sub(r"boundary=[A-Za-z0-9]+", "boundary=<boundary>", value)
                    for value in values
                ]
    result["body"] = body
    return result


def stored_files(store_dir: str) -> str:
    """Deterministic listing of the files a `*_store` directive created."""
    if not store_dir or not os.path.isdir(store_dir):
        return "stored=none"
    entries = []
    for root, _directories, files in os.walk(store_dir):
        for name in sorted(files):
            path = os.path.join(root, name)
            relative = os.path.relpath(path, store_dir).replace(os.sep, "/")
            try:
                with open(path, "rb") as handle:
                    content = handle.read()
                mode = stat.S_IMODE(os.stat(path).st_mode)
            except OSError as error:
                entries.append(f"{relative}=error:{error}")
                continue
            entries.append(f"{relative}|{mode:o}|{content.decode('utf-8', 'replace')}")
    return "stored=" + (";".join(entries) if entries else "none")


def wave7d_case(name: str, port: int, store_dir: str) -> dict:
    def get(path: str, headers: dict | None = None, method: str = "GET", body=None):
        result, _server = request(port, method, path, headers or {}, body, port)
        # The mock FastCGI responder reports its own per-side connection id in
        # the echoed body. The two sides run against independent responders, so
        # the absolute id is not part of the contract (existing cases rely on
        # the two responders accepting connections in the same order; the new
        # cases intentionally compare only the request contract).
        result["body"] = re.sub(r'"connection":\d+', '"connection":<id>', result["body"])
        return result

    if name == "d7-store":
        result = get("/d7/store/f1")
        result["body"] = result["body"] + "\n" + stored_files(store_dir)
        return result
    if name == "d7-rate":
        started = time.monotonic()
        result = get("/d7/rate/")
        elapsed = time.monotonic() - started
        # 20 000 bytes at `fastcgi_limit_rate 20k` must take about one second on
        # both sides; the coarse window tolerates scheduler jitter while still
        # proving the pacing happened (unthrottled delivery is milliseconds).
        window_ok = 0.45 <= elapsed <= 4.0
        result["body"] = (
            f"bytes={len(result['body'])} rate-window={'ok' if window_ok else 'miss'}"
        )
        return result
    if name == "d7-range-single":
        return normalize_multipart_boundaries(get("/d7/range/", {"Range": "bytes=2-5"}))
    if name == "d7-range-suffix":
        return normalize_multipart_boundaries(get("/d7/range/", {"Range": "bytes=-4"}))
    if name == "d7-range-multipart":
        return normalize_multipart_boundaries(
            get("/d7/range/", {"Range": "bytes=0-1,8-9"})
        )
    if name == "d7-range-invalid":
        return get("/d7/range/", {"Range": "bytes=100-200"})
    if name == "d7-range-off":
        return get("/echo/range-off", {"Range": "bytes=0-3"})
    if name == "d7-bufsize":
        return get("/d7/bufsize/")
    if name == "d7-ignore":
        return get("/d7/ignore/")
    if name == "d7-passbody":
        return get("/echo-noreq/upload", {"Content-Type": "text/plain"}, "POST", b"should-not-forward")
    if name == "d7-memcached-gzip-on":
        return get("/d7/gz-on/gzip-key")
    if name == "d7-memcached-gzip-off":
        return get("/d7/gz-off/plain-key")
    raise AssertionError(f"unknown wave-7d case {name}")


def cache_case(name: str, port: int) -> dict:
    """Run one stateful cache sequence and fold it into one comparable record.

    The body is a relation string (generation values, hit flags), so the oracle
    and the runtime compare equal only when they agree on *when* the upstream
    was consulted -- absolute timing is not part of the contract.
    """

    def get(path: str, headers: dict | None = None, method: str = "GET") -> dict:
        result, _ = request(port, method, path, headers or {}, None, port)
        return result

    if name == "cache-default":
        first = get("/cache-default/one")
        second = get("/cache-default/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s second=%s cached=%s"
            % (first["body"], second["body"], first["body"] == second["body"]),
        }
    if name == "cache-hit":
        first = get("/cache/basic/one")
        second = get("/cache/basic/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s second=%s cached=%s"
            % (first["body"], second["body"], first["body"] == second["body"]),
        }
    if name == "cache-bypass":
        first = get("/cache-bypass/one")
        bypassed = get("/cache-bypass/one?bypass=1")
        third = get("/cache-bypass/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s bypassed=%s third=%s"
            % (first["body"], bypassed["body"], third["body"]),
        }
    if name == "cache-no-cache":
        first = get("/cache-nocache/one?nocache=1")
        second = get("/cache-nocache/one")
        third = get("/cache-nocache/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s second=%s third=%s"
            % (first["body"], second["body"], third["body"]),
        }
    if name == "cache-valid":
        first = get("/cache-valid/one")
        time.sleep(2.4)
        expired = get("/cache-valid/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s expired=%s refetched=%s"
            % (first["body"], expired["body"], first["body"] != expired["body"]),
        }
    if name == "cache-key":
        first = get("/cache-key/first?key=shared")
        second = get("/cache-key/second?key=shared")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s second=%s shared=%s"
            % (first["body"], second["body"], first["body"] == second["body"]),
        }
    if name == "cache-min-uses":
        first = get("/cache-min-uses/one")
        second = get("/cache-min-uses/one")
        third = get("/cache-min-uses/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s second=%s third=%s"
            % (first["body"], second["body"], third["body"]),
        }
    if name == "cache-lock":
        results: list[dict] = []
        lock = threading.Lock()
        barrier = threading.Barrier(2)

        def fetch() -> None:
            barrier.wait()
            result, _ = request(port, "GET", "/cache-lock/slow", {}, None, port)
            with lock:
                results.append(result)

        threads = [threading.Thread(target=fetch) for _ in range(2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        bodies = sorted(result["body"] for result in results)
        status = results[0]["status"] if results else 0
        return {
            "status": status,
            "headers": results[0]["headers"] if results else {},
            "trailers": {},
            "body": "bodies=%s coalesced=%s"
            % (",".join(bodies), len({result["body"] for result in results}) == 1),
        }
    if name == "cache-stale":
        prime = get("/cache-stale/one")
        time.sleep(2.4)
        stale = get("/cache-stale/one", {"X-Fail": "1"})
        return {
            "status": stale["status"],
            "headers": stale["headers"],
            "trailers": {},
            "body": "prime=%s stale=%s served_stale=%s"
            % (prime["body"], stale["body"], stale["status"] == 200),
        }
    if name == "cache-revalidate":
        prime = get("/cache-revalidate/one")
        time.sleep(2.4)
        revalidated = get("/cache-revalidate/one")
        return {
            "status": revalidated["status"],
            "headers": revalidated["headers"],
            "trailers": {},
            "body": "prime=%s revalidated=%s refreshed=%s"
            % (prime["body"], revalidated["body"], prime["body"] == revalidated["body"]),
        }
    if name == "cache-convert-head":
        first = get("/cache-head/one")
        head = get("/cache-head/one", method="HEAD")
        second = get("/cache-head/one")
        return {
            "status": head["status"],
            "headers": head["headers"],
            "trailers": {},
            "body": "get=%s head_body=%s second=%s"
            % (first["body"], len(head["body"]), second["body"]),
        }
    # nginx may consult the upstream twice for the first Range request (it
    # fills the cache with the full representation), so the absolute generation
    # counter is not part of the contract here: the cache decision is.
    if name == "cache-range-default":
        first = get("/cache-range-default/one", {"Range": "bytes=0-4"})
        second = get("/cache-range-default/one", {"Range": "bytes=0-4"})
        return {
            "status": first["status"],
            "headers": {},
            "trailers": {},
            "body": "cached=%s" % (first["body"] == second["body"]),
        }
    # nginx may consult the upstream twice for the first Range request (it
    # fills the cache with the full representation), so the absolute generation
    # counter is not part of the contract here: the cache decision is.
    if name == "cache-range-beyond":
        first = get("/cache-range-beyond/one", {"Range": "bytes=100-104"})
        second = get("/cache-range-beyond/one", {"Range": "bytes=100-104"})
        return {
            "status": first["status"],
            "headers": {},
            "trailers": {},
            "body": "cached=%s" % (first["body"] == second["body"]),
        }
    if name == "cache-uwsgi-hit":
        first = get("/cache-uwsgi/one")
        second = get("/cache-uwsgi/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s second=%s cached=%s"
            % (first["body"], second["body"], first["body"] == second["body"]),
        }
    if name == "cache-scgi-hit":
        first = get("/cache-scgi/one")
        second = get("/cache-scgi/one")
        return {
            "status": first["status"],
            "headers": first["headers"],
            "trailers": {},
            "body": "first=%s second=%s cached=%s"
            % (first["body"], second["body"], first["body"] == second["body"]),
        }
    # nginx may consult the upstream twice for the first Range request (it
    # fills the cache with the full representation), so the absolute generation
    # counter is not part of the contract here: the cache decision is.
    if name == "cache-range":
        first = get("/cache-range/one", {"Range": "bytes=0-4"})
        second = get("/cache-range/one", {"Range": "bytes=0-4"})
        return {
            "status": first["status"],
            "headers": {},
            "trailers": {},
            "body": "cached=%s" % (first["body"] == second["body"]),
        }
    raise AssertionError(f"unknown cache case {name}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--unix-port", type=int, default=None)
    parser.add_argument("--store-dir", default="")
    parser.add_argument("--out", required=True)
    arguments = parser.parse_args()
    captured = {}
    server = None
    for name, method, path, headers, body in CASES:
        if method == "SPECIAL" and name.startswith("d7-"):
            captured[name] = wave7d_case(
                name, arguments.port, arguments.store_dir
            )
            continue
        if method == "SPECIAL" and name.startswith("cache-"):
            captured[name] = cache_case(name, arguments.port)
            continue
        if method == "SPECIAL":
            # `fastcgi_keep_conn` reuse: two sequential requests must be answered
            # on the same responder connection (the mock reports its local
            # connection id only when the client keeps the socket open).
            first, server_line = request(
                arguments.port, "GET", "/keepconn/one", {"X-Sent": "keepconn-1"},
                None, arguments.port,
            )
            second, _ = request(
                arguments.port, "GET", "/keepconn/two", {"X-Sent": "keepconn-2"},
                None, arguments.port,
            )
            try:
                first_id = json.loads(first["body"]).get("connection")
                second_id = json.loads(second["body"]).get("connection")
            except (ValueError, AttributeError):
                first_id = second_id = None
            captured[name] = {
                "status": first["status"],
                "headers": first["headers"],
                "trailers": {},
                "body": "same-connection=%s" % (
                    first_id is not None and first_id == second_id
                ),
            }
            if server is None and server_line is not None:
                server = server_line
            continue
        result, server_line = request(
            arguments.port, method, path, headers, body, arguments.port
        )
        if server is None and server_line is not None:
            server = server_line
        captured[name] = result
    if arguments.unix_port is not None:
        for name, method, path, headers, body in UNIX_CASES:
            result, server_line = request(
                arguments.unix_port, method, path, headers, body,
                arguments.unix_port, host=UNIX_HOST,
            )
            if server is None and server_line is not None:
                server = server_line
            captured[name] = result
    output = {"_server": server, "_cases": len(captured), "cases": captured}
    with open(arguments.out, "w", encoding="utf-8") as handle:
        json.dump(output, handle, indent=1, sort_keys=True)
    print(f"battery captured {len(captured)} cases")


if __name__ == "__main__":
    main()
