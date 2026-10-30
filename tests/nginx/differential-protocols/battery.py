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
import socket

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


def request(port: int, method: str, path: str, headers: dict, body: bytes | None, base_port: int):
    lines = [f"{method} {path} HTTP/1.1", f"Host: {HOST}", "Connection: close"]
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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--out", required=True)
    arguments = parser.parse_args()
    captured = {}
    server = None
    for name, method, path, headers, body in CASES:
        result, server_line = request(
            arguments.port, method, path, headers, body, arguments.port
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
