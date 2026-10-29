"""Canonical broad HTTP battery: static, routing, rewrite, headers, error pages,
gzip, access control, request limits and upstream proxying.

One script, two endpoints: run it against real nginx (the oracle) and against
the Rust data plane, then diff the two JSON captures. Only the observable
response contract is recorded — status, the complete response header map, the
decoded body — so a difference is always a real behavioural difference and never
a header-order or formatting artefact.

    python3 battery.py --port 20992 --upstream-port 20994 --out nginx.json

The battery owns the mock upstream. It is deterministic: every upstream answer
is a fixed function of the request, and `/slow` is the only delay (used by the
limit_conn case). Both endpoints are measured against the same fixture files
whose mtimes are pinned here, so `ETag` (`<mtime>-<size>`) and `Last-Modified`
are byte-identical on both sides and are compared exactly.

`Date` is recorded but never compared: the two runs happen seconds apart.
`Server` is recorded and compared through the intentional-difference allowlist in
compare.py — it proves which implementation answered each port.

Cases deliberately NOT in this battery because the real-nginx differential exposed
a runtime/compatibility gap that lives outside this slice's fix scope (reported
alongside the battery; each is a candidate case once the gap closes):

- malformed `Upgrade` requests: the runtime rejects them with 400 as hardening,
  nginx silently strips the hop-by-hop field and serves the request.

The `add_header` matrix, the directory-without-index 403, and `error_page`
mapping of a `return` status (including a `return 3xx`) are covered below; they
were excluded while those semantics were unimplemented.

Randomised / transport-shaped values are canonicalised before comparison:

- a multipart byte-range body is parsed into its parts (boundary token and
  per-part CRLF framing removed) because nginx derives the boundary from a
  per-process counter;
- the side's own listen port and the mock upstream port are replaced by
  `<port>` / `<upstream>` inside header values so an absolute `Location` built
  from the listener address compares equal (`:20992` vs `:20993`);
- gzip bodies are decompressed and the decoded representation is compared.
"""

from __future__ import annotations

import argparse
import base64
import datetime
import gzip
import hashlib
import http.server
import json
import os
import socket
import sys
import threading
import time
from pathlib import Path

FIXTURE_DIR = Path(__file__).resolve().parent
PUBLIC_DIR = FIXTURE_DIR / "public"

HOST = "diff.example.com"
# Second virtual host on the same listener; it owns the `add_header` policy so
# the header cases are isolated from generated error/limit responses.
HEADERS_HOST = "headers.example.com"
# Third virtual host: owns the gzip policy so generated error/limit/proxy
# responses on the default host are compared without that orthogonal axis.
GZIP_HOST = "gzip.example.com"
# Fourth virtual host: declared in the configuration with one exact location and
# no catch-all, so a request can select the host but no route (nginx still
# applies the server's inherited `add_header` policy to its generated 404).
NO_ROUTE_HOST = "noroute.example.com"
# 2020-01-01T00:00:00Z. Both endpoints serve these exact files, so pinning the
# mtime makes `ETag` and `Last-Modified` stable across checkouts (git does not
# preserve mtimes). The battery pins it on both runs before any request.
PINNED_MTIME_SECONDS = 1577836800

# Request headers the mock upstream never reports. `Connection`/`TE` framing is
# an artefact of the proxy's upstream HTTP version (nginx defaults to HTTP/1.0
# and sends `Connection: close`; the runtime speaks HTTP/1.1 and advertises
# `TE: trailers`), not part of the proxied request contract. Every other header
# is reported verbatim, so hop-by-hop stripping is observable.
UPSTREAM_FRAMING_HEADERS = frozenset({"connection", "te", "transfer-encoding"})

# Response headers whose values are normalised per side before comparison.
SIDE_PORTS = ("port", "upstream", "dead")


def sha256_hex(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def pin_fixture_mtimes() -> None:
    """Pin every fixture file to the same mtime on both sides."""
    for path in sorted(PUBLIC_DIR.rglob("*")):
        if path.is_file():
            os.utime(path, (PINNED_MTIME_SECONDS, PINNED_MTIME_SECONDS))


class _MockUpstreamHandler(http.server.BaseHTTPRequestHandler):
    """Deterministic upstream: response is a pure function of the request."""

    protocol_version = "HTTP/1.1"
    # The mock owns its whole header list: no `Server`/`Date` (both would
    # otherwise leak timing/identity into a proxied response comparison).
    def log_message(self, *_args: object) -> None:  # noqa: D401 - stdlib hook
        pass

    def send_response(self, code: int, message: str | None = None) -> None:
        self.send_response_only(code, message)

    def version_string(self) -> str:
        return ""

    def date_time_string(self, timestamp: float | None = None) -> str:
        return ""

    def _write(self, status: int, content_type: str, payload: bytes, extra=()) -> None:
        self.send_response(status)
        for name, value in extra:
            self.send_header(name, value)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(payload)

    def do_GET(self) -> None:  # noqa: N802 - stdlib hook
        self._handle()

    def do_HEAD(self) -> None:  # noqa: N802 - stdlib hook
        self._handle()

    def do_POST(self) -> None:  # noqa: N802 - stdlib hook
        self._handle()

    def _handle(self) -> None:
        path, _, query = self.path.partition("?")
        if "/status/" in path:
            try:
                code = int(path.rsplit("/", 1)[1])
            except ValueError:
                code = 500
            self._write(
                code,
                "text/plain",
                ("upstream-status-%d\n" % code).encode(),
                extra=[("X-Upstream", "yes")],
            )
            return
        if path.endswith("/slow"):
            # Long enough for the limit_conn test to hold the first slot while
            # the second request arrives; the battery never depends on the exact
            # duration, only on the overlap.
            time.sleep(2.0)
            self._write(200, "text/plain", b"slow-done\n", extra=[("X-Upstream", "yes")])
            return
        if path.endswith("/hop-response"):
            self._write(
                200,
                "text/plain",
                b"hop-response\n",
                extra=[
                    ("X-Upstream", "yes"),
                    ("X-Drop", "1"),
                    ("Set-Cookie", "sid=diff; Path=/"),
                    # A Connection header names an additional hop-by-hop field:
                    # both endpoints must drop `X-Drop` as well as this header.
                    ("Connection", "x-drop"),
                ],
            )
            return
        if path.endswith("/echo/host"):
            payload = json.dumps(
                {"host": self.headers.get("Host")}, sort_keys=True
            ).encode() + b"\n"
            self._write(200, "application/json", payload, extra=[("X-Upstream", "yes")])
            return
        if "/echo/header/" in path:
            name = path.rsplit("/echo/header/", 1)[1]
            payload = json.dumps(
                {"name": name, "value": self.headers.get(name)}, sort_keys=True
            ).encode() + b"\n"
            self._write(200, "application/json", payload, extra=[("X-Upstream", "yes")])
            return
        if path.endswith("/echo/headers"):
            observed = sorted(
                (name.lower(), value)
                for name, value in self.headers.items()
                if name.lower() not in UPSTREAM_FRAMING_HEADERS
            )
            payload = json.dumps({"headers": observed}, sort_keys=True).encode() + b"\n"
            self._write(200, "application/json", payload, extra=[("X-Upstream", "yes")])
            return
        # `/echo/path...` and every other path report the received request
        # target; the query string is part of the contract under test.
        payload = json.dumps({"path": self.path}, sort_keys=True).encode() + b"\n"
        self._write(200, "application/json", payload, extra=[("X-Upstream", "yes")])


class _MockUpstream(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def start_upstream(port: int) -> _MockUpstream:
    server = _MockUpstream(("127.0.0.1", port), _MockUpstreamHandler)
    thread = threading.Thread(target=server.serve_forever, name="mock-upstream", daemon=True)
    thread.start()
    return server


def request(
    port: int,
    path: str,
    method: str = "GET",
    headers=None,
    connection: str | None = "close",
    timeout: float = 15.0,
    host: str = HOST,
):
    lines = ["%s %s HTTP/1.1" % (method, path), "Host: %s" % host]
    if connection is not None:
        lines.append("Connection: %s" % connection)
    for name, value in headers or []:
        lines.append("%s: %s" % (name, value))
    payload = ("\r\n".join(lines) + "\r\n\r\n").encode("latin-1")
    with socket.create_connection(("127.0.0.1", port), timeout=timeout) as sock:
        sock.settimeout(timeout)
        sock.sendall(payload)
        data = bytearray()
        while True:
            try:
                chunk = sock.recv(65536)
            except socket.timeout:
                break
            if not chunk:
                break
            data.extend(chunk)
    head, _, body = bytes(data).partition(b"\r\n\r\n")
    head_lines = head.decode("latin-1").split("\r\n")
    status = int(head_lines[0].split(" ")[1])
    parsed: dict[str, list[str]] = {}
    for line in head_lines[1:]:
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        parsed.setdefault(name.strip().lower(), []).append(value.strip())
    if "chunked" in ",".join(parsed.get("transfer-encoding", [])).lower():
        body = dechunk(body)
    return status, parsed, body


def dechunk(payload: bytes) -> bytes:
    """Remove HTTP/1.1 chunked framing from a raw socket body."""
    output = bytearray()
    remainder = payload
    while remainder:
        size_line, separator, rest = remainder.partition(b"\r\n")
        if not separator or not size_line:
            break
        try:
            size = int(size_line.split(b";", 1)[0].strip(), 16)
        except ValueError:
            break
        if size == 0:
            break
        output.extend(rest[:size])
        remainder = rest[size + 2 :]  # chunk bytes + trailing CRLF
    return bytes(output)


def media_type(header_values) -> str:
    if not header_values:
        return ""
    return header_values[0].split(";", 1)[0].strip().lower()


def parse_multipart(headers, body: bytes):
    """Canonicalise a `multipart/byteranges` body into its parts.

    nginx's boundary token is a per-process counter (`000…001`, `000…002`, …),
    so the token and the CRLF framing around it are not part of the observable
    contract; the per-part headers and payload bytes are.
    """
    content_type = " ".join(headers.get("content-type", []))
    if not content_type.lower().startswith("multipart/byteranges"):
        return None
    boundary = None
    for piece in content_type.split(";"):
        piece = piece.strip()
        if piece.lower().startswith("boundary="):
            boundary = piece.split("=", 1)[1].strip('"')
    if not boundary:
        return None
    delimiter = b"--" + boundary.encode("latin-1")
    chunks = body.split(delimiter)
    parts = []
    for chunk in chunks[1:]:
        if chunk.startswith(b"--"):
            break
        if chunk.startswith(b"\r\n"):
            chunk = chunk[2:]
        part_head, separator, part_body = chunk.partition(b"\r\n\r\n")
        if not separator:
            return {"invalid": True}
        if part_body.endswith(b"\r\n"):
            part_body = part_body[:-2]
        part_headers = {}
        for line in part_head.split(b"\r\n"):
            if b":" not in line:
                continue
            name, value = line.split(b":", 1)
            part_headers[name.strip().lower().decode("latin-1")] = (
                value.strip().decode("latin-1")
            )
        parts.append(
            {
                "headers": part_headers,
                "body_len": len(part_body),
                "body_sha256": sha256_hex(part_body),
            }
        )
    return {"parts": parts}


def decoded_representation(headers, body: bytes) -> bytes:
    encoding = ",".join(headers.get("content-encoding", [])).lower()
    if "gzip" in encoding:
        try:
            return gzip.decompress(body)
        except (OSError, EOFError):
            return b"<gzip-decode-error>"
    return body


def normalize_side_ports(payload: bytes, port: int, upstream_port: int, dead_port: int) -> bytes:
    """Replace each side's listener/upstream/dead port with a fixed token.

    Both endpoints are measured on different ports (the oracle and the gateway
    must never share a socket), but a port is not part of the response contract:
    an absolute `Location`, an echoed `Host`, or an `X-Forwarded-Host` value must
    compare equal on both sides. The tokens are identical for both captures, so
    lengths stay comparable too.
    """
    for value, token in (
        (port, b":<port>"),
        (upstream_port, b":<upstream>"),
        (dead_port, b":<dead>"),
    ):
        payload = payload.replace(b":" + str(value).encode(), token)
    return payload


def canonicalize(port: int, upstream_port: int, dead_port: int, status, headers, body):
    replacements = {
        ":" + str(port): ":<port>",
        ":" + str(upstream_port): ":<upstream>",
        ":" + str(dead_port): ":<dead>",
    }
    normalized_headers = {}
    for name, values in headers.items():
        joined = "\n".join(values)
        for needle, replacement in replacements.items():
            joined = joined.replace(needle, replacement)
        normalized_headers[name] = joined

    decoded = decoded_representation(headers, body)
    multipart = parse_multipart(headers, body)
    if multipart is None:
        decoded = normalize_side_ports(decoded, port, upstream_port, dead_port)
    record = {
        "status": status,
        "headers": normalized_headers,
        "body_len": len(decoded),
        "body_sha256": sha256_hex(decoded),
        "nginx_error_page": b"<center>nginx/" in decoded,
    }
    if multipart is not None:
        record["multipart"] = multipart
        record["content_type_media"] = media_type(headers.get("content-type"))
        # The boundary token is a per-process counter, not part of the contract;
        # dropping it leaves the media type that both sides must agree on (the
        # parts themselves are compared structurally above).
        record["headers"]["content-type"] = "multipart/byteranges"
    if len(decoded) <= 512:
        try:
            text = decoded.decode("utf-8")
        except UnicodeDecodeError:
            text = None
        if text is not None and all(
            character.isprintable() or character in "\r\n\t" for character in text
        ):
            record["body_text"] = text
    return record


def capture(
    port: int,
    label: str,
    path: str,
    results: dict,
    upstream_port: int,
    dead_port: int,
    method: str = "GET",
    headers=None,
    connection: str | None = "close",
    host: str = HOST,
) -> None:
    status, response_headers, body = request(
        port, path, method=method, headers=headers, connection=connection, host=host
    )
    results[label] = canonicalize(
        port, upstream_port, dead_port, status, response_headers, body
    )


def capture_concurrent_connection_limit(
    port: int, upstream_port: int, dead_port: int, results: dict
) -> None:
    """Hold the first /conn/ request while the second one hits the limit.

    limit_conn counts concurrent downstream connections, so the two requests
    must overlap: the first one blocks in the mock upstream `/slow` while the
    second one is sent. Both responses are then recorded by request position,
    not by wall-clock timing.
    """
    held: dict = {}

    def first():
        try:
            held["first"] = request(port, "/conn/slow")
        except OSError as error:  # pragma: no cover - diagnostic path
            held["error"] = error

    worker = threading.Thread(target=first, name="limit-conn-first", daemon=True)
    worker.start()
    # Let the first request reach the data plane and acquire its slot before
    # the second one is sent; the mock upstream then keeps it there.
    time.sleep(0.5)
    second = request(port, "/conn/other")
    worker.join(timeout=15.0)
    first_status = held.get("first")
    if first_status is None:
        raise RuntimeError("the held limit_conn request did not complete")
    results["limit-conn.first"] = canonicalize(
        port, upstream_port, dead_port, *first_status
    )
    results["limit-conn.second"] = canonicalize(
        port, upstream_port, dead_port, *second
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--upstream-port", type=int, required=True)
    parser.add_argument("--dead-port", type=int, required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    pin_fixture_mtimes()
    try:
        upstream = start_upstream(args.upstream_port)
    except OSError as error:
        print(
            "cannot bind mock upstream on 127.0.0.1:%d: %s"
            % (args.upstream_port, error),
            file=sys.stderr,
        )
        return 2

    port = args.port
    upstream_port = args.upstream_port
    dead_port = args.dead_port
    results: dict = {}

    def case(label, path, method="GET", headers=None, connection="close", host=HOST):
        capture(
            port, label, path, results, upstream_port, dead_port,
            method=method, headers=headers, connection=connection, host=host,
        )

    try:
        # Baseline probe: identity witness plus the validators every
        # conditional case is derived from, so neither side needs a hard-coded
        # tag.
        baseline_status, baseline_headers, _ = request(port, "/alias/hello.txt")
        identity = baseline_headers.get("server", [None])[0]
        etag = baseline_headers.get("etag", [None])[0]
        last_modified = baseline_headers.get("last-modified", [None])[0]
        if etag is None or last_modified is None:
            print(
                "baseline is missing a validator: %r" % (baseline_headers,),
                file=sys.stderr,
            )
            return 2
        if baseline_status != 200:
            print(
                "baseline /alias/hello.txt answered %d" % baseline_status,
                file=sys.stderr,
            )
            return 2
        # One hour before the served Last-Modified, derived from the validator
        # itself so neither side needs a hard-coded date.
        parsed_last_modified = datetime.datetime.strptime(
            last_modified, "%a, %d %b %Y %H:%M:%S GMT"
        )
        older_last_modified = (
            parsed_last_modified - datetime.timedelta(hours=1)
        ).strftime("%a, %d %b %Y %H:%M:%S GMT")

        # ---- static delivery ------------------------------------------
        case("static.root-index", "/")
        case("static.subdir-index", "/sub/")
        case("static.dir-redirect", "/sub")
        case("static.404", "/missing.txt")
        # A directory request whose `index` file does not exist: nginx (autoindex
        # off) answers 403, with no entity tag and with `error_page` still able
        # to map the status.
        case("static.dir-no-index", "/nodx/")
        case("static.mime.html", "/alias/index.html")
        case("static.mime.txt", "/alias/hello.txt")
        case("static.mime.json", "/alias/data.json")
        case("static.mime.css", "/alias/style.css")
        case("static.mime.js", "/alias/app.js")
        case("static.mime.png", "/alias/image.png")
        case("static.mime.unknown", "/alias/blob.zzz")
        case("static.head", "/alias/hello.txt", method="HEAD")
        case("static.range.simple", "/alias/hello.txt", headers=[("Range", "bytes=0-3")])
        case("static.range.open", "/alias/hello.txt", headers=[("Range", "bytes=6-")])
        case("static.range.suffix", "/alias/hello.txt", headers=[("Range", "bytes=-4")])
        case("static.range.clamped", "/alias/hello.txt", headers=[("Range", "bytes=0-99")])
        case(
            "static.range.multi",
            "/alias/hello.txt",
            headers=[("Range", "bytes=0-1,4-5")],
        )
        case(
            "static.range.unsatisfiable",
            "/alias/hello.txt",
            headers=[("Range", "bytes=99-")],
        )
        case("static.range.invalid", "/alias/hello.txt", headers=[("Range", "bytes=abc")])
        case(
            "static.conditional.inm-match",
            "/alias/hello.txt",
            headers=[("If-None-Match", etag)],
        )
        case(
            "static.conditional.ims-equal",
            "/alias/hello.txt",
            headers=[("If-Modified-Since", last_modified)],
        )
        case(
            "static.conditional.ims-older",
            "/alias/hello.txt",
            headers=[("If-Modified-Since", older_last_modified)],
        )

        # ---- routing and rewrite --------------------------------------
        case("route.exact", "/exact")
        case("route.prefix", "/exact/child")
        case("route.regex", "/image.png")
        case("route.caret-tilde", "/assets/image.png")
        case("route.ret-status", "/ret-status")
        case("route.ret-body", "/ret-body")
        case("route.ret-url", "/ret-url")
        # `return 404` under the host-level `error_page 404`: nginx maps a
        # status produced by a rewrite-phase `return`, not only one produced by
        # a resource handler.
        case("route.ret-404", "/ret-404")
        case("route.rewrite-last", "/rw-last/hello.txt")
        case("route.rewrite-break", "/rw-break/hello.txt")
        case("route.rewrite-capture", "/cap/abc/123")
        case("route.rewrite-redirect", "/rw-redirect/hello.txt")
        case("route.rewrite-permanent", "/rw-perm/hello.txt")
        case("route.try-files-hit", "/app/index.html")
        case("route.try-files-fallback", "/app/missing")
        case("route.server-root", "/hello.txt")

        # ---- response headers -----------------------------------------
        case("headers.server-on-200", "/alias/hello.txt", host=HEADERS_HOST)
        case("headers.server-on-301", "/rw-perm/hello.txt", host=HEADERS_HOST)
        # http-level policy inherited by a host that declares no `add_header`
        # (200: both fields; the 301 case shows the default redirect statuses).
        case("headers.http-level-inherited", "/alias/hello.txt")
        case("headers.http-level-on-301", "/rw-perm/hello.txt")
        # Status gating on a `return 404`: the non-`always` server field is
        # omitted, the `always` field is emitted.
        case("headers.server-gating-on-404", "/hdr-404", host=HEADERS_HOST)
        # A location-level declaration replaces the inherited server array.
        case("headers.location-replaces-server", "/hdr-loc", host=HEADERS_HOST)
        # nginx variable expansion inside an `add_header` value.
        case("headers.variables", "/hdr-vars", host=HEADERS_HOST)
        # Location-level `always` field on a static 404 and on a static 200.
        case(
            "headers.location-always-on-static-404",
            "/hdrfiles/missing.txt",
            host=HEADERS_HOST,
        )
        case(
            "headers.location-replaces-server-on-200",
            "/hdrfiles/hello.txt",
            host=HEADERS_HOST,
        )
        # A generated 403 from the access phase still runs the headers filter.
        case(
            "headers.location-always-on-generated-403",
            "/hdrdenied/inside",
            host=HEADERS_HOST,
        )
        # `error_page` mapping of a `return 3xx` final status.
        case("headers.redirect-error-page-301", "/hdr-301", host=HEADERS_HOST)
        # A host declared in the configuration but with no matching location:
        # nginx still applies its inherited `add_header` policy to the generated
        # 404, and the runtime must resolve the host even though no route
        # matched.
        case(
            "headers.host-policy-on-unmatched-route",
            "/no-such-route.txt",
            host=NO_ROUTE_HOST,
        )

        # ---- error_page -------------------------------------------------
        case("error-page.location-override", "/lost/missing")
        case("error-page.code-override", "/lost-code/missing")
        case("error-page.recursive-chain", "/chain/missing")

        # ---- gzip negotiation ------------------------------------------
        gzip_headers = [("Accept-Encoding", "gzip")]
        case("gzip.text", "/gz/text.txt", headers=gzip_headers, host=GZIP_HOST)
        case("gzip.identity", "/gz/text.txt", headers=[("Accept-Encoding", "identity")], host=GZIP_HOST)
        case("gzip.q0", "/gz/text.txt", headers=[("Accept-Encoding", "gzip;q=0")], host=GZIP_HOST)
        case("gzip.q-low", "/gz/text.txt", headers=[("Accept-Encoding", "gzip;q=0.5")], host=GZIP_HOST)
        case("gzip.types.json", "/gz/data.json", headers=gzip_headers, host=GZIP_HOST)
        case("gzip.types-css", "/gz/style.css", headers=gzip_headers, host=GZIP_HOST)
        case("gzip.min-length", "/gz/small.txt", headers=gzip_headers, host=GZIP_HOST)
        case("gzip.location-off", "/gzplain/text.txt", headers=gzip_headers, host=GZIP_HOST)
        case("gzip.no-accept-encoding", "/gz/text.txt", host=GZIP_HOST)

        # ---- access control and auth -----------------------------------
        case("access.allow-then-deny", "/admin/inside")
        case("access.deny-all", "/blocked/inside")
        case("access.deny-first", "/deny-first/inside")
        case("auth.missing", "/private/inside")
        case(
            "auth.wrong",
            "/private/inside",
            headers=[
                (
                    "Authorization",
                    "Basic " + base64.b64encode(b"diff:wrong").decode("latin-1"),
                )
            ],
        )
        case(
            "auth.good",
            "/private/inside",
            headers=[
                (
                    "Authorization",
                    "Basic " + base64.b64encode(b"diff:s3cret").decode("latin-1"),
                )
            ],
        )

        # ---- request limits --------------------------------------------
        case("limit-req.first", "/rate/a")
        case("limit-req.second", "/rate/b")
        case("limit-req.third", "/rate/c")
        capture_concurrent_connection_limit(port, upstream_port, dead_port, results)

        # ---- upstream proxying -----------------------------------------
        case("proxy.pass-path", "/proxy/echo/path?x=1")
        case("proxy.pass-uri-replace", "/proxy-uri/echo/path?a=1")
        case("proxy.pass-uri-args", "/proxy-uri/a/b?q=2")
        case("proxy.pass-uri-exact", "/proxy-uri/")
        case("proxy.pass-uri-splice", "/proxy-prefix/inner")
        case("proxy.host-default", "/proxy/echo/host")
        case("proxy.host-set", "/proxy-host/")
        case("proxy.forwarded-for", "/proxy-fwd/echo/header/x-forwarded-for")
        case("proxy.forwarded-proto", "/proxy-fwd/echo/header/x-forwarded-proto")
        case(
            "proxy.hop-by-hop-request",
            "/proxy-hbh/echo/headers",
            headers=[
                ("Keep-Alive", "timeout=5"),
                ("X-Client-Echo", "hop"),
            ],
        )
        case("proxy.response-pass-through", "/proxy-headers/hop-response")
        case("proxy.upstream-503", "/proxy-status/503")
        case("proxy.upstream-502", "/proxy-dead/down")

        document = {
            "_server": identity,
            "_port": port,
            "_upstream_port": upstream_port,
            "_etag": etag,
            "_last_modified": last_modified,
            "_fixture_hello_bytes": len((PUBLIC_DIR / "hello.txt").read_bytes()),
        }
        document.update(dict(sorted(results.items())))
        with open(args.out, "w", encoding="utf-8") as handle:
            json.dump(document, handle, indent=2, sort_keys=True)
            handle.write("\n")
        print("wrote %s (%d cases)" % (args.out, len(results)))
        print("identity: server=%r" % (identity,))
        print("validators: etag=%r last-modified=%r" % (etag, last_modified))
        return 0
    finally:
        upstream.shutdown()
        upstream.server_close()


if __name__ == "__main__":
    sys.exit(main())
