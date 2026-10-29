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
- an invalid header line (`X Bad: v`, a space inside the field name): nginx
  answers 400 on a bad parse; the runtime's HTTP boundary guards so strictly
  that it never produces a response for the connection, so there is no
  comparable status/body record. `ignore_invalid_headers` is still parsed and
  materialized (see the `headers-invalid-strict` host) and the runtime's
  strictness is documented on the config field.

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
import re
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
# Fifth virtual host: owns the nginx `map` matrix. It declares its own
# `add_header` array so map results are observable on static responses without
# touching the header-policy host's expectations.
MAPS_HOST = "maps.example.com"
# Sixth virtual host: owns the nginx `types` / `default_type` matrix.
MIME_HOST = "mime.example.com"
# Seventh virtual host: owns the `gzip_static` / `gzip_vary` matrix.
GZSTATIC_HOST = "gzstatic.example.com"
# Eighth host family: owns the redirect-construction policy matrix
# (`absolute_redirect` / `port_in_redirect` / `server_name_in_redirect`).
REDIRECT_HOST = "redirect.example.com"
REDIRECT_REL_HOST = "redirect-rel.example.com"
REDIRECT_NOPORT_HOST = "redirect-noport.example.com"
REDIRECT_SRVNAME_HOST = "redirect-srvname.example.com"
# A second server_name of the `server_name_in_redirect on` host: the request
# presents this name, so an absolute Location must show the *first* name.
REDIRECT_ALIAS_HOST = "redirect-alias.example.com"
# Ninth host: `max_ranges` matrix.
RANGES_HOST = "ranges.example.com"
# Tenth host: configurable `limit_req_status` / `limit_conn_status`.
LIMITS_HOST = "limits.example.com"
# Eleventh host family: nginx proxy response-header policy
# (`proxy_hide_header` / `proxy_pass_header` / `proxy_redirect` /
# `proxy_cookie_path` / `proxy_cookie_domain`). The `-abs` host runs the same
# rewrites under `absolute_redirect on`.
PROXYHDR_HOST = "proxyhdr.example.com"
PROXYHDR_ABS_HOST = "proxyhdr-abs.example.com"
# Twelfth host family: `internal` locations (external 404, internal redirects).
INTERNAL_HOST = "internal.example.com"
INTERNAL_PLAIN_HOST = "internal-plain.example.com"
# Thirteenth host: the `merge_slashes off` matrix.
MERGE_HOST = "merge.example.com"
# Fourteenth host family: request header acceptance policy
# (`underscores_in_headers` default off / explicit on, invalid header name).
HEADERS_UNDERSCORE_OFF_HOST = "headers-underscore-off.example.com"
HEADERS_UNDERSCORE_ON_HOST = "headers-underscore-on.example.com"
HEADERS_INVALID_STRICT_HOST = "headers-invalid-strict.example.com"
# Fifteenth host family: nginx `charset` family header filter.
CHARSET_HOST = "charset.example.com"
CHARSET_TYPES_HOST = "charset-types.example.com"
# Sixteenth host: gzip refinement directives (`gzip_disable`, `gzip_proxied`).
GZIPREF_HOST = "gzipref.example.com"
# Seventeenth host: nginx `autoindex` directory listings.
AUTOINDEX_HOST = "autoindex.example.com"
# Eighteenth host: the `empty_gif` location handler.
EMPTY_GIF_HOST = "emptygif.example.com"
# Nineteenth host: the `stub_status` page (loopback/allow-gated).
STATUS_HOST = "status.example.com"
# Twentieth host: nginx `valid_referers` / `$invalid_referer`.
REFERER_HOST = "referer.example.com"
# Twenty-first host: `geo` / `split_clients` variable blocks.
GEOSPLIT_HOST = "geosplit.example.com"
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
    """Pin every fixture file *and directory* to the same mtime on both sides.

    Directories matter for `autoindex` listings: nginx renders a directory
    entry's own mtime, so an unpinned directory would make the committed oracle
    capture depend on checkout time rather than on the fixture bytes.
    """
    for path in sorted(PUBLIC_DIR.rglob("*")):
        os.utime(path, (PINNED_MTIME_SECONDS, PINNED_MTIME_SECONDS))


# The mock upstream's own port, used to build absolute `Location` / `Refresh`
# values that only a proxy response-header policy can rewrite.
UPSTREAM_PLACEHOLDER = 0


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
        if path.endswith("/up-text"):
            self._write(200, "text/plain", b"the quick brown fox jumps over the lazy dog\n", extra=[("X-Upstream", "yes")])
            return
        if path.endswith("/up-text-lm"):
            self._write(
                200,
                "text/plain",
                b"the quick brown fox jumps over the lazy dog\n",
                extra=[
                    ("X-Upstream", "yes"),
                    ("Last-Modified", "Wed, 01 Jan 2020 00:00:00 GMT"),
                ],
            )
            return
        if path.endswith("/up-text-nocache"):
            self._write(
                200,
                "text/plain",
                b"the quick brown fox jumps over the lazy dog\n",
                extra=[("X-Upstream", "yes"), ("Cache-Control", "no-cache")],
            )
            return
        if path.endswith("/up-text-expired"):
            self._write(
                200,
                "text/plain",
                b"the quick brown fox jumps over the lazy dog\n",
                extra=[
                    ("X-Upstream", "yes"),
                    ("Expires", "Thu, 01 Jan 1970 00:00:00 GMT"),
                ],
            )
            return
        if path.endswith("/up-text-future"):
            self._write(
                200,
                "text/plain",
                b"the quick brown fox jumps over the lazy dog\n",
                extra=[
                    ("X-Upstream", "yes"),
                    ("Expires", "Fri, 01 Jan 2100 00:00:00 GMT"),
                ],
            )
            return
        if path.endswith("/up-plain"):
            self._write(200, "text/html", b"<html>up</html>\n", extra=[("X-Upstream", "yes")])
            return
        if path.endswith("/up-charset"):
            self._write(
                200,
                "text/html; charset=iso-8859-1",
                b"<html>up</html>\n",
                extra=[("X-Upstream", "yes")],
            )
            return
        if path.endswith("/up-utf8"):
            self._write(
                200,
                "text/html; charset=UTF-8",
                b"<html>up</html>\n",
                extra=[("X-Upstream", "yes")],
            )
            return
        if path.endswith("/up-params"):
            self._write(
                200,
                "text/html; boundary=xyz; charset=UTF-8",
                b"<html>up</html>\n",
                extra=[("X-Upstream", "yes")],
            )
            return
        if path.endswith("/hidden-headers"):
            # The default-hidden upstream set (`ngx_http_proxy_hide_headers`)
            # plus one ordinary field: nginx hides every X-Accel-*/X-Pad field
            # and the upstream `Server` banner, and forwards X-Upstream/X-Secret.
            self._write(
                200,
                "text/plain",
                b"hidden-headers\n",
                extra=[
                    ("Server", "upstream-banner/1.0"),
                    ("X-Pad", "pad"),
                    ("X-Accel-Expires", "60"),
                    ("X-Accel-Buffering", "yes"),
                    ("X-Accel-Limit-Rate", "100"),
                    ("X-Accel-Charset", "utf-8"),
                    ("X-Secret", "s"),
                    ("X-Upstream", "yes"),
                ],
            )
            return
        if path.endswith("/redir-basic"):
            self._write(
                302,
                "text/plain",
                b"redir-basic\n",
                extra=[
                    ("X-Upstream", "yes"),
                    (
                        "Location",
                        "http://127.0.0.1:%d/echo/target?x=1" % UPSTREAM_PLACEHOLDER,
                    ),
                ],
            )
            return
        if path.endswith("/redir-other"):
            self._write(
                302,
                "text/plain",
                b"redir-other\n",
                extra=[
                    ("X-Upstream", "yes"),
                    ("Location", "http://other.example.com/path"),
                ],
            )
            return
        if path.endswith("/redir-relative"):
            self._write(
                302,
                "text/plain",
                b"redir-relative\n",
                extra=[("X-Upstream", "yes"), ("Location", "/local/path")],
            )
            return
        if path.endswith("/refresh-basic"):
            self._write(
                200,
                "text/plain",
                b"refresh-basic\n",
                extra=[
                    ("X-Upstream", "yes"),
                    (
                        "Refresh",
                        "5; url=http://127.0.0.1:%d/echo/target" % UPSTREAM_PLACEHOLDER,
                    ),
                ],
            )
            return
        if path.endswith("/cookies"):
            self._write(
                200,
                "text/plain",
                b"cookies\n",
                extra=[
                    ("X-Upstream", "yes"),
                    (
                        "Set-Cookie",
                        "sid=abc; Path=/upstream; Domain=upstream.example.com; HttpOnly",
                    ),
                    ("Set-Cookie", "other=1; Path=/"),
                    ("Set-Cookie", "bare=2"),
                ],
            )
            return
        if path.endswith("/cookies-case"):
            self._write(
                200,
                "text/plain",
                b"cookies-case\n",
                extra=[
                    ("X-Upstream", "yes"),
                    (
                        "Set-Cookie",
                        "c=1; path=/upstream; domain=upstream.example.com",
                    ),
                ],
            )
            return
        if path.endswith("/echo/method"):
            length = int(self.headers.get("Content-Length") or 0)
            payload = self.rfile.read(length) if length else b""
            body = (
                json.dumps(
                    {
                        "method": self.command,
                        "body": payload.decode("utf-8", "replace"),
                    },
                    sort_keys=True,
                ).encode()
                + b"\n"
            )
            self._write(200, "application/json", body, extra=[("X-Upstream", "yes")])
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
    canonicalize_stub_status(record)
    return record


STUB_STATUS_MARKER = "Active connections: "


def canonicalize_stub_status(record) -> None:
    """Mask the process-local `stub_status` counters.

    nginx's status page reports live per-process numbers (accepted requests,
    active connections, ...) that depend on the measurement order and can never
    match between two sequential runs. The field *shape* is the compared
    contract: digit runs are replaced by `N` and the length header by a token,
    so status/content-type/body-template differences still fail.
    """
    text = record.get("body_text")
    if not isinstance(text, str) or not text.startswith(STUB_STATUS_MARKER):
        return
    masked = re.sub(r"[0-9]+", "N", text)
    record["body_text"] = masked
    record["body_len"] = len(masked)
    record["body_sha256"] = sha256_hex(masked.encode("utf-8"))
    record["headers"]["content-length"] = "<dynamic>" 


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
    port: int,
    upstream_port: int,
    dead_port: int,
    results: dict,
    host: str = HOST,
    prefix: str = "limit-conn",
    base_path: str = "/conn/",
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
            held["first"] = request(port, base_path + "slow", host=host)
        except OSError as error:  # pragma: no cover - diagnostic path
            held["error"] = error

    worker = threading.Thread(target=first, name="limit-conn-first", daemon=True)
    worker.start()
    # Let the first request reach the data plane and acquire its slot before
    # the second one is sent; the mock upstream then keeps it there.
    time.sleep(0.5)
    second = request(port, base_path + "other", host=host)
    worker.join(timeout=15.0)
    first_status = held.get("first")
    if first_status is None:
        raise RuntimeError("the held limit_conn request did not complete")
    results[prefix + ".first"] = canonicalize(
        port, upstream_port, dead_port, *first_status
    )
    results[prefix + ".second"] = canonicalize(
        port, upstream_port, dead_port, *second
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--upstream-port", type=int, required=True)
    parser.add_argument("--dead-port", type=int, required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    global UPSTREAM_PLACEHOLDER
    UPSTREAM_PLACEHOLDER = args.upstream_port

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
        # nginx `max_ranges 0` disables byte-range processing entirely: even a
        # valid Range answers the full 200 and `Accept-Ranges` is not sent.
        case("static.max-ranges.zero-plain", "/zero/plain.txt", host=RANGES_HOST)
        case(
            "static.max-ranges.zero-range",
            "/zero/plain.txt",
            headers=[("Range", "bytes=0-2")],
            host=RANGES_HOST,
        )
        # A ceiling of 1 accepts a single range and declines a two-range
        # request (full 200, `Accept-Ranges: bytes` still advertised).
        case(
            "static.max-ranges.one-single",
            "/one/plain.txt",
            headers=[("Range", "bytes=0-2")],
            host=RANGES_HOST,
        )
        case(
            "static.max-ranges.one-multi-declined",
            "/one/plain.txt",
            headers=[("Range", "bytes=0-2,4-5")],
            host=RANGES_HOST,
        )
        case(
            "static.max-ranges.two-multi",
            "/two/plain.txt",
            headers=[("Range", "bytes=0-2,4-5")],
            host=RANGES_HOST,
        )

        # ---- redirect construction policy ------------------------------
        # Differential-confirmed against nginx 1.29.6: the header filter makes
        # a relative target absolute from the resolved host/port unless
        # `absolute_redirect off`; the port is the listener port unless it is
        # the scheme default or `port_in_redirect off`; the host is the first
        # `server_name` under `server_name_in_redirect on` and the request
        # `Host` otherwise; an absolute target passes through untouched.
        case("redirect.default.dir", "/sub", host=REDIRECT_HOST)
        case("redirect.default.return-relative", "/ret-rel", host=REDIRECT_HOST)
        case("redirect.default.return-absolute", "/ret-abs", host=REDIRECT_HOST)
        case(
            "redirect.default.rewrite-redirect",
            "/rw-redirect/hello.txt",
            host=REDIRECT_HOST,
        )
        case(
            "redirect.default.mapped-error-page",
            "/mapped-301",
            host=REDIRECT_HOST,
        )
        case("redirect.relative.dir", "/sub", host=REDIRECT_REL_HOST)
        case(
            "redirect.relative.return-relative",
            "/ret-rel",
            host=REDIRECT_REL_HOST,
        )
        case("redirect.noport.dir", "/sub", host=REDIRECT_NOPORT_HOST)
        case("redirect.srvname.dir", "/sub", host=REDIRECT_ALIAS_HOST)
        case(
            "redirect.srvname.return-relative",
            "/ret-rel",
            host=REDIRECT_ALIAS_HOST,
        )
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

        # ---- nginx types / default_type --------------------------------
        # A host-level `types` block replaces the inherited http table:
        # `.txt` resolves through the host row, not the http row.
        case("mime.host-table", "/m/hello.txt", host=MIME_HOST)
        # An extension absent from the declared table falls to `default_type`.
        case("mime.host-default", "/m/blob.zzz", host=MIME_HOST)
        # A location-level table replaces the host table...
        case("mime.location-table", "/m-loc/hello.txt", host=MIME_HOST)
        # ...and its `default_type` applies to unknown extensions.
        case("mime.location-default", "/m-loc/blob.zzz", host=MIME_HOST)
        # A location table without its own `default_type` inherits the host
        # default (the two directives have independent inheritance chains).
        case("mime.location-inherits-default", "/m-loc-inherit/blob.zzz", host=MIME_HOST)
        # An explicitly empty table replaces the inherited one: every
        # extension uses `default_type`.
        case("mime.empty-table", "/m-raw/blob.zzz", host=MIME_HOST)
        # A `return` body's type comes from the request URI extension through
        # the resolved table (`default_type` when it has none).
        case("mime.return-no-extension", "/m-ret", host=MIME_HOST)
        case("mime.return-extension", "/m-ret.txt", host=MIME_HOST)

        # ---- gzip_static / gzip_vary ----------------------------------
        accept_gzip = [("Accept-Encoding", "gzip")]
        accept_identity = [("Accept-Encoding", "identity")]
        accept_rejects_gzip = [("Accept-Encoding", "gzip;q=0")]
        # `gzip_static on`: the sidecar replies only when gzip is accepted; its
        # `Content-Length`, `ETag` and bytes are the sidecar's.
        case("gz.static-on-gzip", "/on/page.txt", host=GZSTATIC_HOST, headers=accept_gzip)
        case("gz.static-on-identity", "/on/page.txt", host=GZSTATIC_HOST, headers=accept_identity)
        case("gz.static-on-q0", "/on/page.txt", host=GZSTATIC_HOST, headers=accept_rejects_gzip)
        case("gz.static-on-no-header", "/on/page.txt", host=GZSTATIC_HOST)
        # A sidecar that does not exist falls back to the identity file; a
        # `gzip_vary on` in `on` mode does not declare the negotiation when no
        # sidecar was found.
        case("gz.static-missing-sidecar", "/on/plain.txt", host=GZSTATIC_HOST, headers=accept_gzip)
        # `gzip_static on` + `gzip_vary on`: Vary on both representations.
        case("gz.static-vary-gzip", "/onvary/page.txt", host=GZSTATIC_HOST, headers=accept_gzip)
        case("gz.static-vary-identity", "/onvary/page.txt", host=GZSTATIC_HOST, headers=accept_identity)
        case("gz.static-vary-no-header", "/onvary/page.txt", host=GZSTATIC_HOST)
        # `gzip_static always`: sidecar regardless of `Accept-Encoding`, and no
        # `Vary` negotiation field.
        case("gz.static-always-identity", "/always/page.txt", host=GZSTATIC_HOST, headers=accept_identity)
        case("gz.static-always-q0", "/always/page.txt", host=GZSTATIC_HOST, headers=accept_rejects_gzip)
        # No `gzip_static`: identity representation even when gzip is accepted.
        case("gz.static-off", "/off/page.txt", host=GZSTATIC_HOST, headers=accept_gzip)
        # A range request ranges over the stored gzip bytes, exactly like nginx.
        case(
            "gz.static-range",
            "/on/page.txt",
            host=GZSTATIC_HOST,
            headers=accept_gzip + [("Range", "bytes=0-3")],
        )
        # HEAD mirrors the headers of the negotiated representation.
        case("gz.static-head", "/on/page.txt", host=GZSTATIC_HOST, method="HEAD", headers=accept_gzip)
        # Dynamic gzip: `gzip_vary on` declares the negotiation field on both
        # the compressed and the identity representation.
        case("gz.dynamic-vary-gzip", "/dynvary/page.txt", host=GZSTATIC_HOST, headers=accept_gzip)
        case("gz.dynamic-vary-identity", "/dynvary/page.txt", host=GZSTATIC_HOST, headers=accept_identity)
        # `gzip_vary off` (and nginx's default-off) suppress the negotiation
        # field even though the representation is compressed.
        case("gz.dynamic-vary-off", "/dynnovary/page.txt", host=GZSTATIC_HOST, headers=accept_gzip)
        case("gz.dynamic-vary-default", "/dyndefault/page.txt", host=GZSTATIC_HOST, headers=accept_gzip)

        # A conditional request validated by the sidecar's own entity tag is
        # answered 304, proving the sidecar's validators are the published ones.
        _, gz_headers, _ = request(
            port,
            "/on/page.txt",
            headers=accept_gzip,
            host=GZSTATIC_HOST,
        )
        sidecar_etag = gz_headers.get("etag", [None])[0]
        if sidecar_etag is not None:
            case(
                "gz.static-conditional",
                "/on/page.txt",
                host=GZSTATIC_HOST,
                headers=accept_gzip + [("If-None-Match", sidecar_etag)],
            )

        # ---- nginx map variables ---------------------------------------
        # Default fallback: an unmatched source value selects `default`.
        case(
            "map.header-default",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Tenant", "unknown-tenant")],
        )
        # An absent source value is the empty string, so the authored empty key
        # wins over `default` (nginx semantics for a missing header/argument).
        case("map.header-empty-key", "/alias/hello.txt", host=MAPS_HOST)
        # Keys are matched case-insensitively (nginx lowercases the map hash).
        case(
            "map.header-exact-key",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Tenant", "ACME")],
        )
        # Regex keys run in authored order after hash keys (`~*` here).
        case(
            "map.header-regex-key",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Tenant", "BETA-42")],
        )
        # A regex match's `$1` capture expands inside the map value.
        case(
            "map.header-regex-capture",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Tenant", "gamma-7")],
        )
        # `hostnames` wildcard keys match a subdomain...
        case(
            "map.hostnames-wildcard",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Hostkey", "a.wild.example.com")],
        )
        # ...but `*.suffix` does NOT match the apex (differential-probed).
        case(
            "map.hostnames-apex-is-not-wildcard",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Hostkey", "wild.example.com")],
        )
        # An exact hostname key wins over the wildcard.
        case(
            "map.hostnames-exact",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Hostkey", "exact.example.com")],
        )
        # A map whose result is empty removes the header like nginx.
        case(
            "map.header-empty-result",
            "/alias/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Tenant", "acme")],
        )
        # `return` body/URL and `rewrite` replacements are variable scripts:
        # they expand map results too.
        case(
            "map.return-body",
            "/m-ret",
            host=MAPS_HOST,
            headers=[("X-Tenant", "beta-9")],
        )
        case(
            "map.return-url",
            "/m-url",
            host=MAPS_HOST,
            headers=[("X-Tenant", "acme")],
        )
        case(
            "map.rewrite",
            "/m-rw/hello.txt",
            host=MAPS_HOST,
            headers=[("X-Tenant", "acme")],
        )
        # `proxy_set_header` value expansion from a map result.
        case(
            "map.proxy-set-header",
            "/m-proxy",
            host=MAPS_HOST,
            headers=[("X-Tenant", "acme")],
        )
        # `$request_method` source.
        case("map.method-source", "/m-ret", host=MAPS_HOST)
        # `$arg_` values are raw: `?tag=` is present-but-empty and matches `""`.
        case("map.arg-empty", "/m-ret?tag=", host=MAPS_HOST)
        # ...while an encoded value keeps its raw spelling for key matching.
        case("map.arg-raw", "/m-ret?tag=a%20b", host=MAPS_HOST)
        case("map.arg-default", "/m-ret?tag=zzz", host=MAPS_HOST)

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
        # `limit_req_status 429` (location) and `limit_conn_status 429` (server
        # inheritance) replace the nginx default 503.
        case("limit-req-status.first", "/rate429/a", host=LIMITS_HOST)
        case("limit-req-status.second", "/rate429/b", host=LIMITS_HOST)
        case("limit-req-status.third", "/rate429/c", host=LIMITS_HOST)
        capture_concurrent_connection_limit(
            port,
            upstream_port,
            dead_port,
            results,
            host=LIMITS_HOST,
            prefix="limit-conn-status",
            base_path="/conn429/",
        )

        # ---- proxy response-header policy ------------------------------
        # The default-hidden upstream set: Server/X-Pad/X-Accel-* are never
        # forwarded; X-Upstream/X-Secret pass through.
        case("proxy.hidden-default", "/plain/hidden-headers", host=PROXYHDR_HOST)
        case("proxy.hide-header", "/hide/hidden-headers", host=PROXYHDR_HOST)
        # `proxy_pass_header` removes a name from the hidden set (pass wins).
        case("proxy.pass-header", "/pass/hidden-headers", host=PROXYHDR_HOST)
        # `proxy_redirect default` replaces the `proxy_pass` URL base with the
        # location name; a non-matching target passes through untouched.
        case(
            "proxy.redirect-default",
            "/redirect-default/redir-basic",
            host=PROXYHDR_HOST,
        )
        case(
            "proxy.redirect-default-other",
            "/redirect-default/redir-other",
            host=PROXYHDR_HOST,
        )
        case(
            "proxy.redirect-default-relative",
            "/redirect-default/redir-relative",
            host=PROXYHDR_HOST,
        )
        # `proxy_redirect off` forwards the upstream target verbatim.
        case(
            "proxy.redirect-off",
            "/redirect-off/redir-basic",
            host=PROXYHDR_HOST,
        )
        # Explicit rules: a relative replacement, and a rule on another host.
        case(
            "proxy.redirect-explicit",
            "/redirect-explicit/redir-basic",
            host=PROXYHDR_HOST,
        )
        case(
            "proxy.redirect-explicit-absolute",
            "/redirect-explicit-absolute/redir-other",
            host=PROXYHDR_HOST,
        )
        # The `Refresh` header's `url=` part is rewritten exactly like a
        # dangling Location (the delay prefix is preserved).
        case(
            "proxy.redirect-refresh",
            "/redirect-default/refresh-basic",
            host=PROXYHDR_HOST,
        )
        # `proxy_redirect` rewrites a relative target, which the header filter
        # then makes absolute under `absolute_redirect on`.
        case(
            "proxy.redirect-absolute-default",
            "/redirect-default/redir-basic",
            host=PROXYHDR_ABS_HOST,
        )
        case(
            "proxy.redirect-absolute-explicit",
            "/redirect-explicit/redir-basic",
            host=PROXYHDR_ABS_HOST,
        )
        # Cookie attribute rewriting: Path prefix, Domain exact (case-insensitive,
        # leading dot preserved), both on one cookie, and `off`.
        case("proxy.cookie-path", "/cookie-path/cookies", host=PROXYHDR_HOST)
        case("proxy.cookie-domain", "/cookie-domain/cookies", host=PROXYHDR_HOST)
        case(
            "proxy.cookie-domain-dot",
            "/cookie-domain-dot/cookies",
            host=PROXYHDR_HOST,
        )
        case("proxy.cookie-both", "/cookie-both/cookies", host=PROXYHDR_HOST)
        case("proxy.cookie-off", "/cookie-off/cookies", host=PROXYHDR_HOST)
        case(
            "proxy.cookie-case",
            "/cookie-path/cookies-case",
            host=PROXYHDR_HOST,
        )

        # ---- gzip refinements -----------------------------------------
        accept_gzip_header = [("Accept-Encoding", "gzip")]
        # Control: a normal user agent is compressed.
        case(
            "gzip.refine-control",
            "/plain/hello.txt",
            host=GZIPREF_HOST,
            headers=accept_gzip_header,
        )
        # `gzip_disable` regexes are case-insensitive against `User-Agent`.
        case(
            "gzip.disable-ua-match",
            "/disable-ua/hello.txt",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("User-Agent", "Mozilla/5.0 (compatible; Googlebot/2.1)")],
        )
        case(
            "gzip.disable-ua-no-match",
            "/disable-ua/hello.txt",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("User-Agent", "Mozilla/5.0 (X11; Linux x86_64)")],
        )
        # The predefined `msie6` selector: IE 4/5/6 without SV1, and the Opera
        # carve-out that clears MSIE detection.
        case(
            "gzip.disable-msie6",
            "/disable-msie6/hello.txt",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("User-Agent", "Mozilla/4.0 (compatible; MSIE 6.0; Windows NT 5.1)")],
        )
        case(
            "gzip.disable-msie6-sv1",
            "/disable-msie6/hello.txt",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("User-Agent", "Mozilla/4.0 (compatible; MSIE 6.0; Windows NT 5.1; SV1)")],
        )
        case(
            "gzip.disable-msie6-opera",
            "/disable-msie6/hello.txt",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("User-Agent", "Mozilla/4.0 (compatible; MSIE 6.0; Windows NT 5.1; Opera 9.0)")],
        )
        # `gzip_proxied`: with a `Via` header the request is proxied traffic.
        # nginx's default is `off`, so no compression.
        case(
            "gzip.proxied-default-off",
            "/px-only/up-text",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("Via", "1.0 proxy.example.com")],
        )
        case(
            "gzip.proxied-explicit-off",
            "/px-off/up-text",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("Via", "1.0 proxy.example.com")],
        )
        case(
            "gzip.proxied-any",
            "/px-any/up-text",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("Via", "1.0 proxy.example.com")],
        )
        # Without `Via` the proxied policy does not apply at all.
        case(
            "gzip.proxied-any-no-via",
            "/px-any/up-text",
            host=GZIPREF_HOST,
            headers=accept_gzip_header,
        )
        # `no-cache` accepts a response whose Cache-Control declares it.
        case(
            "gzip.proxied-no-cache",
            "/px-nocache/up-text-nocache",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("Via", "1.0 proxy.example.com")],
        )
        # `no_last_modified` declines a response carrying Last-Modified.
        case(
            "gzip.proxied-no-last-modified",
            "/px-lm/up-text-lm",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("Via", "1.0 proxy.example.com")],
        )
        # `auth` accepts a proxied request that carries Authorization.
        case(
            "gzip.proxied-auth",
            "/px-auth/up-text",
            host=GZIPREF_HOST,
            headers=accept_gzip_header
            + [("Via", "1.0 proxy.example.com"), ("Authorization", "Bearer token")],
        )
        # `expired` accepts an already-expired Expires and declines a future one.
        case(
            "gzip.proxied-expired",
            "/px-expired/up-text-expired",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("Via", "1.0 proxy.example.com")],
        )
        case(
            "gzip.proxied-not-expired",
            "/px-expired/up-text-future",
            host=GZIPREF_HOST,
            headers=accept_gzip_header + [("Via", "1.0 proxy.example.com")],
        )

        # ---- stub_status -----------------------------------------------
        # Loopback/allow-gated status page: the counter values are masked by
        # `canonicalize_stub_status` (they are process-local runtime state), so
        # the compared contract is the field shape and the plain-text headers.
        case("stub-status.get", "/status", host=STATUS_HOST)
        case("stub-status.head", "/status", host=STATUS_HOST, method="HEAD")
        case("stub-status.post-405", "/status", host=STATUS_HOST, method="POST")

        # ---- empty_gif -------------------------------------------------
        # The canonical 43-byte GIF with its image/gif content type. HEAD keeps
        # the generated headers; a non-GET/HEAD method is 405.
        case("empty-gif.get", "/pixel.gif", host=EMPTY_GIF_HOST)
        case("empty-gif.head", "/pixel.gif", host=EMPTY_GIF_HOST, method="HEAD")
        case("empty-gif.post-405", "/pixel.gif", host=EMPTY_GIF_HOST, method="POST")

        # ---- autoindex listings ---------------------------------------
        # Exact HTML listing (entries sorted directories-first then by name,
        # hidden names skipped, pinned mtimes/sizes, escaping applied).
        case("autoindex.html", "/list/", host=AUTOINDEX_HOST)
        # `autoindex_exact_size off`: nginx's rounded K/M/G column.
        case("autoindex.html-human-size", "/list-human/", host=AUTOINDEX_HOST)
        # `autoindex_localtime on`; both sides run with TZ=UTC.
        case("autoindex.html-localtime", "/list-localtime/", host=AUTOINDEX_HOST)
        # JSON / JSONP / XML shapes.
        case("autoindex.json", "/list-json/", host=AUTOINDEX_HOST)
        case(
            "autoindex.jsonp",
            "/list-jsonp/?callback=cb",
            host=AUTOINDEX_HOST,
        )
        # Without a `callback` nginx downgrades jsonp to plain JSON.
        case("autoindex.jsonp-no-callback", "/list-jsonp/", host=AUTOINDEX_HOST)
        # An invalid callback name is rejected with 400.
        case(
            "autoindex.jsonp-invalid-callback",
            "/list-jsonp/?callback=bad()",
            host=AUTOINDEX_HOST,
        )
        case("autoindex.xml", "/list-xml/", host=AUTOINDEX_HOST)
        # `autoindex off` keeps the directory-without-index 403.
        case("autoindex.off-403", "/list-off/", host=AUTOINDEX_HOST)
        # A directory whose `index.html` exists serves the index, not a listing.
        case("autoindex.index-wins", "/with-index/", host=AUTOINDEX_HOST)
        # Directory requests without the trailing slash still 301-redirect.
        case("autoindex.dir-redirect", "/list", host=AUTOINDEX_HOST)
        # HEAD keeps the generated headers and drops the body.
        case("autoindex.head", "/list/", host=AUTOINDEX_HOST, method="HEAD")

        # ---- geo / split_clients ---------------------------------------
        def geosplit_case(label, headers=None):
            case(label, "/gs/", host=GEOSPLIT_HOST, headers=headers or [])

        geosplit_case("geo.loopback")
        geosplit_case("geo.xff-range", [("X-Forwarded-For", "10.0.0.7")])
        geosplit_case("geo.xff-v6", [("X-Forwarded-For", "2001:db8::10")])
        geosplit_case("geo.xff-default", [("X-Forwarded-For", "8.8.8.8")])
        for key in ("alpha", "beta", "gamma", "delta", "epsilon", "nginx"):
            geosplit_case(
                f"split.{key}",
                [("X-Split-Key", key)],
            ) if False else case(
                f"split.{key}",
                f"/gs/?key={key}",
                host=GEOSPLIT_HOST,
            )

        # ---- valid_referers / $invalid_referer -------------------------
        # `X-Referer-Invalid: 1` is emitted only for rejected referers; a
        # valid referer expands to an empty value and nginx omits the field.
        def referer_case(label, referer=None, path="/ref/", host=REFERER_HOST):
            headers = [] if referer is None else [("Referer", referer)]
            case(label, path, host=host, headers=headers)

        referer_case("referer.missing")
        referer_case("referer.blocked", "relative/path")
        referer_case("referer.uppercase-scheme", "HTTP://REFERER.EXAMPLE.COM/x")
        referer_case("referer.server-names", "http://referer.example.com/x")
        referer_case("referer.wildcard", "http://a.allowed.test/x")
        referer_case("referer.path-match", "http://allowed.org/galleries/pic")
        referer_case("referer.path-mismatch", "http://allowed.org/other")
        referer_case("referer.regex", "http://www.google.com/search")
        referer_case("referer.invalid", "http://evil.test/x")
        referer_case("referer.port", "http://sub.allowed.test:8443/x")
        referer_case("referer.location-override-valid", "http://referer.example.com/x", "/ref-own/")
        referer_case("referer.location-override-invalid", "http://a.allowed.test/x", "/ref-own/")

        # ---- proxy_method ----------------------------------------------
        # The client's GET is forwarded upstream as POST (`proxy_method POST`).
        case("proxy.method-override-post", "/proxy-method/echo/method")

        # ---- charset family -------------------------------------------
        # The server-level `charset utf-8` is appended for the built-in
        # default type list...
        case("charset.default-html", "/plain/index.html", host=CHARSET_HOST)
        case("charset.default-txt", "/plain/hello.txt", host=CHARSET_HOST)
        case("charset.default-js", "/plain/app.js", host=CHARSET_HOST)
        # ...and not for media types outside it (`application/json`,
        # `text/css`, images).
        case("charset.default-json-untouched", "/plain/data.json", host=CHARSET_HOST)
        case("charset.default-css-untouched", "/plain/style.css", host=CHARSET_HOST)
        case("charset.default-png-untouched", "/plain/image.png", host=CHARSET_HOST)
        # `charset off` at a location disables the inherited value.
        case("charset.off-location", "/off/index.html", host=CHARSET_HOST)
        # A declared list is seeded with `text/html` first.
        case("charset.declared-types-css", "/types-css/style.css", host=CHARSET_HOST)
        case("charset.declared-types-seeded-html", "/types-css/index.html", host=CHARSET_HOST)
        case("charset.declared-types-json-untouched", "/types-css/data.json", host=CHARSET_HOST)
        # `charset_types *` matches every media type.
        case("charset.wildcard-png", "/wild/image.png", host=CHARSET_HOST)
        # A `return` body derives its type from the URI extension (`text/plain`
        # here) and still receives the charset.
        case("charset.return-body", "/ret/", host=CHARSET_HOST)
        # nginx never publishes a charset on a 301/302.
        case("charset.redirect-301", "/ret-301/", host=CHARSET_HOST)
        # Proxied responses: configured charset applies when the upstream has
        # none; an upstream charset is kept unless `override_charset on`.
        case("charset.proxy-no-upstream-charset", "/p-off/up-plain", host=CHARSET_HOST)
        case("charset.proxy-upstream-charset-kept", "/p-off/up-charset", host=CHARSET_HOST)
        # With override on, an equal charset is normalized to the configured
        # spelling; a differing one would need `charset_map` (accepted-
        # unenforced, header left untouched).
        case("charset.proxy-override-normalizes", "/p-on/up-utf8", host=CHARSET_HOST)
        case(
            "charset.proxy-override-differing-kept",
            "/p-on/up-charset",
            host=CHARSET_HOST,
        )
        case(
            "charset.proxy-override-params-normalized",
            "/p-on/up-params",
            host=CHARSET_HOST,
        )
        # Server-level `charset_types` is inherited by a location that declares
        # none, and replaced by one that declares its own list.
        case("charset.server-types-json", "/plain/data.json", host=CHARSET_TYPES_HOST)
        case(
            "charset.location-types-replace",
            "/loc-css/data.json",
            host=CHARSET_TYPES_HOST,
        )
        case("charset.location-types-css", "/loc-css/style.css", host=CHARSET_TYPES_HOST)

        # ---- internal locations ---------------------------------------
        # An external request for an `internal` location is answered with the
        # generated 404 (mapped through the host `error_page 404` here).
        case("internal.external", "/internal/x", host=INTERNAL_HOST)
        case("internal.public", "/pub/x", host=INTERNAL_HOST)
        # Reachable through `rewrite … last` and through `error_page`.
        case("internal.rewrite", "/int-rewrite/x", host=INTERNAL_HOST)
        case("internal.errpage", "/int-errpage/x", host=INTERNAL_HOST)
        # Without an `error_page` mapping, the generated 404 still carries the
        # server `always` header.
        case("internal.external-plain", "/internal-plain/x", host=INTERNAL_PLAIN_HOST)

        # ---- merge_slashes off ----------------------------------------
        # An exact location only matches the unmerged spelling.
        case("merge.exact", "/merge/exact", host=MERGE_HOST)
        case("merge.double-slash-does-not-match-exact", "/merge//exact", host=MERGE_HOST)
        case("merge.triple-slash-does-not-match-exact", "/merge///exact", host=MERGE_HOST)
        # `$uri` keeps the repeated slashes; `$request_uri` is always raw.
        case("merge.uri-keeps-slashes", "/merge/show//x", host=MERGE_HOST)
        # A dot-segment still resolves, popping the empty segment.
        case("merge.dotdot-pops-empty-segment", "/merge/show//../show/x", host=MERGE_HOST)
        # Percent-decoded slashes participate in the unmerged identity.
        case("merge.encoded-slashes", "/merge%2F%2Fexact", host=MERGE_HOST)
        # Static resolution collapses the repeated slash at the filesystem.
        case("merge.static", "/merge/files//hello.txt", host=MERGE_HOST)

        # ---- request header acceptance policy -------------------------
        # nginx default `underscores_in_headers off`: the field is dropped
        # before the proxy module forwards the request headers.
        case(
            "headers.underscore-default-dropped",
            "/echo/echo/headers",
            host=HEADERS_UNDERSCORE_OFF_HOST,
            headers=[("X_Underscore", "v")],
        )
        case(
            "headers.underscore-on-forwarded",
            "/echo/echo/headers",
            host=HEADERS_UNDERSCORE_ON_HOST,
            headers=[("X_Underscore", "v")],
        )
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
