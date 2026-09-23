# WORKSPACE-PATH:allow-fixture: fixtures bind fixed loopback test ports and name the local checkout binary, so the literal is the value under assertion rather than a binding this build resolves
"""Full-surface single-page nginx behavioral regression for sdkwork-webserver.

One fixture (nginx.conf) plus this one probe file exercise every
runtime-executable http-core-v1 directive family declared in
specs/nginx-gap.catalog.json against the Rust data plane:

  routing            exact / prefix / ^~ / regex / ~* locations, rewrite
                     (last/break/redirect/permanent), return, try_files,
                     alias, root, index, wildcard server_name, default server
  uri semantics      canonical $uri vs raw $request_uri (percent decoding,
                     dot segments, merge slashes, encoded slash)
  static files       MIME by extension, ETag/Last-Modified conditionals,
                     ranges, HEAD, 404, SPA fallback
  proxying           proxy_pass with/without URI part, variable proxy_pass,
                     proxy_set_header inheritance/override,
                     proxy_pass_request_headers off, hop-by-hop stripping,
                     body limits, chunked request bodies, Expect: 100-continue,
                     proxy_cache, incremental streaming, WebSocket tunnels
  upstreams          smooth weighted round robin, least_conn, ip_hash,
                     hash consistent, passive health + backup failover,
                     upstream keepalive
  response filters   gzip negotiation, sub_filter, limit_req, limit_conn,
                     allow/deny ACL, auth_basic, secure_link
  http wire          keep-alive, Connection: close, pipelining, HTTP/1.0
  tls / http2        SNI certificate selection, ALPN h2 SETTINGS exchange
  stream             plaintext TCP proxy round trip

Canonical run (one command, one report):

    cargo build -p sdkwork-api-webserver-standalone-gateway
    python tests/nginx/full-surface/probe.py

The probe owns the mock upstreams, generates the runtime materials
(htpasswd, TLS certs) and by default spawns and stops the server itself;
pass --skip-server-spawn to run against an already-serving fixture.
Exit code 0 means every case passed; failures print as FAIL lines and
the summary table is the single page to review.
"""

from __future__ import annotations

import argparse
import base64
import datetime
import hashlib
import json
import os
import re
import shutil
import socket
import ssl
import subprocess
import sys
import threading
import time
import traceback
from pathlib import Path

FIXTURE_DIR = Path(__file__).resolve().parent
REPO_ROOT = FIXTURE_DIR.parent.parent.parent
SERVER_LOG = FIXTURE_DIR / "logs" / "server.log"

WEBSOCKET_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


# --------------------------------------------------------------------------
# HTTP client (raw sockets; immune to ambient proxy environment variables)
# --------------------------------------------------------------------------


class Response:
    def __init__(self, version: str, status: int, headers: list[tuple[str, str]], body: bytes):
        self.version = version
        self.status = status
        self.headers = headers
        self.body = body

    def header(self, name: str, default: str | None = None) -> str | None:
        lower = name.lower()
        for key, value in self.headers:
            if key.lower() == lower:
                return value
        return default

    def has_header(self, name: str) -> bool:
        return self.header(name) is not None

    def text(self) -> str:
        return self.body.decode("utf-8", "replace")


def body_text(response: Response) -> str:
    """Newline-normalized body text (fixture files are checked out with
    platform line endings; the server serves the bytes verbatim)."""
    return response.text().replace("\r\n", "\n")


class HttpClient:
    """One keep-alive HTTP/1.1 connection with sequential request support."""

    def __init__(self, port: int, timeout: float = 6.0):
        self.port = port
        self.timeout = timeout
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=timeout)

    def send_request(
        self,
        target: str,
        method: str = "GET",
        headers: list[tuple[str, str]] | None = None,
        body: bytes = b"",
        host: str = "static.example.com",
        version: str = "HTTP/1.1",
    ) -> None:
        lines = [f"{method} {target} {version}"]
        seen_connection = False
        for name, value in headers or []:
            if name.lower() == "host":
                continue
            if name.lower() == "connection":
                seen_connection = True
            lines.append(f"{name}: {value}")
        if host is not None and version != "HTTP/1.0":
            lines.append(f"Host: {host}")
        elif host is not None and version == "HTTP/1.0":
            lines.append(f"Host: {host}")
        if body:
            lines.append(f"Content-Length: {len(body)}")
        if not seen_connection:
            lines.append("Connection: keep-alive" if version == "HTTP/1.1" else "Connection: close")
        payload = ("\r\n".join(lines) + "\r\n\r\n").encode("ascii") + body
        self.sock.sendall(payload)

    def read_response(self) -> Response:
        return read_one_response(self.sock, self.timeout)

    def request(self, *args, **kwargs) -> Response:
        self.send_request(*args, **kwargs)
        return self.read_response()

    def close(self) -> None:
        try:
            self.sock.close()
        except OSError:
            pass


def _read_until_headers_end(sock: socket.socket, timeout: float) -> bytes:
    sock.settimeout(timeout)
    data = bytearray()
    while not data.endswith(b"\r\n\r\n"):
        chunk = sock.recv(1)
        if not chunk:
            if not data:
                raise EOFError("connection closed before response")
            raise EOFError(f"connection closed mid-headers after {data[:80]!r}")
        data.extend(chunk)
        if len(data) > 65536:
            raise RuntimeError("response headers exceeded probe bound")
    return bytes(data)


def _read_bytes(sock: socket.socket, length: int, timeout: float) -> bytes:
    sock.settimeout(timeout)
    data = bytearray()
    while len(data) < length:
        chunk = sock.recv(length - len(data))
        if not chunk:
            raise EOFError("connection closed mid-body")
        data.extend(chunk)
    return bytes(data)


def _read_chunked_body(sock: socket.socket, timeout: float, sink, on_first_byte=None) -> bytes:
    sock.settimeout(timeout)
    body = bytearray()
    first_at = None
    while True:
        line = bytearray()
        while not line.endswith(b"\r\n"):
            chunk = sock.recv(1)
            if not chunk:
                raise EOFError("connection closed in chunk header")
            line.extend(chunk)
        size = int(line.strip().split(b";")[0], 16)
        if size == 0:
            # consume trailer CRLF (and any trailer lines)
            while True:
                trailer = bytearray()
                while not trailer.endswith(b"\r\n"):
                    chunk = sock.recv(1)
                    if not chunk:
                        return bytes(body)
                    trailer.extend(chunk)
                if trailer in (b"\r\n", b"\n"):
                    return bytes(body)
        payload = _read_bytes(sock, size, timeout)
        if first_at is None and payload:
            first_at = time.monotonic()
            if on_first_byte:
                on_first_byte(first_at)
        sock.settimeout(timeout)
        crlf = _read_bytes(sock, 2, timeout)
        assert crlf == b"\r\n", f"bad chunk terminator {crlf!r}"
        body.extend(payload)
        sink(payload)


def read_one_response(sock: socket.socket, timeout: float, want_interim: bool = False):
    """Read one full response; optionally return a leading 100 interim first."""
    while True:
        head = _read_until_headers_end(sock, timeout)
        lines = head.decode("ascii", "replace").split("\r\n")
        version, status, _reason = lines[0].split(" ", 2)
        headers = []
        for line in lines[1:]:
            if not line:
                continue
            name, _, value = line.partition(":")
            headers.append((name.strip(), value.strip()))
        status_code = int(status)
        if 100 <= status_code < 200 and not (want_interim and status_code == 100):
            continue  # skip interim responses
        lower = {name.lower(): value for name, value in headers}
        body = b""
        if want_interim and status_code == 100:
            return Response(version, status_code, headers, b""), None
        if "transfer-encoding" in lower and "chunked" in lower["transfer-encoding"]:
            chunks: list[bytes] = []

            def sink(payload: bytes) -> None:
                chunks.append(payload)

            body = _read_chunked_body(sock, timeout, sink)
        elif "content-length" in lower:
            body = _read_bytes(sock, int(lower["content-length"]), timeout)
        else:
            # read until EOF
            sock.settimeout(timeout)
            data = bytearray()
            while True:
                try:
                    chunk = sock.recv(65536)
                except socket.timeout:
                    break
                if not chunk:
                    break
                data.extend(chunk)
            body = bytes(data)
        return Response(version, status_code, headers, body)


def http(
    port: int,
    target: str,
    method: str = "GET",
    headers: list[tuple[str, str]] | None = None,
    body: bytes = b"",
    host: str = "static.example.com",
    version: str = "HTTP/1.1",
    timeout: float = 6.0,
) -> Response:
    client = HttpClient(port, timeout=timeout)
    try:
        return client.request(target, method=method, headers=headers, body=body, host=host, version=version)
    finally:
        client.close()


# --------------------------------------------------------------------------
# Mock upstream backends (probe-owned)
# --------------------------------------------------------------------------


class MockHttpBackend(threading.Thread):
    """Small HTTP/1.1 upstream with per-path hit counters and routes."""

    def __init__(self, name: str, port: int):
        super().__init__(daemon=True, name=f"mock-{name}")
        self.name = name
        self.port = port
        self.hits: dict[str, int] = {}
        self.seen: list[dict] = []
        self.lock = threading.Lock()
        self.listener = socket.socket()
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(64)
        self.ready = threading.Event()

    def hit_count(self, path: str) -> int:
        with self.lock:
            return self.hits.get(path, 0)

    def requests_to(self, path: str) -> list[dict]:
        with self.lock:
            return [r for r in self.seen if r["path"] == path]

    def run(self) -> None:
        self.ready.set()
        while True:
            try:
                conn, _ = self.listener.accept()
            except OSError:
                return
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    def serve(self, conn: socket.socket) -> None:
        with conn:
            conn.settimeout(8)
            buffer = bytearray()
            while True:
                try:
                    req = self.read_request(conn, buffer)
                except (EOFError, socket.timeout, ConnectionError):
                    return
                if req is None:
                    return
                route_key = "/" + req["path"].lstrip("/").split("/")[0]
                with self.lock:
                    self.hits[route_key] = self.hits.get(route_key, 0) + 1
                    self.seen.append(req)
                keep_alive = self.respond(conn, req)
                if not keep_alive:
                    return

    def read_request(self, conn: socket.socket, buffer: bytearray):
        # read headers
        while b"\r\n\r\n" not in buffer:
            chunk = conn.recv(65536)
            if not chunk:
                return None
            buffer.extend(chunk)
        head, _, rest = bytes(buffer).partition(b"\r\n\r\n")
        lines = head.decode("latin-1").split("\r\n")
        method, target, version = lines[0].split(" ")
        headers: list[tuple[str, str]] = []
        for line in lines[1:]:
            name, _, value = line.partition(":")
            headers.append((name.strip().lower(), value.strip()))
        lower = dict(headers)
        body_source = bytearray(rest)
        # read body
        body = b""
        if "transfer-encoding" in lower and "chunked" in lower["transfer-encoding"]:
            while True:
                while b"\r\n" not in body_source:
                    body_source.extend(self.recv_more(conn))
                size_line, _, rest_after = bytes(body_source).partition(b"\r\n")
                size = int(size_line.split(b";")[0], 16)
                consumed = len(size_line) + 2
                del body_source[:consumed]
                if size == 0:
                    # Trailer lines until the empty terminating line.
                    while True:
                        while b"\r\n" not in body_source:
                            body_source.extend(self.recv_more(conn))
                        trailer_line, _, _ = bytes(body_source).partition(b"\r\n")
                        del body_source[: len(trailer_line) + 2]
                        if not trailer_line:
                            break
                    break
                while len(body_source) < size + 2:
                    body_source.extend(self.recv_more(conn))
                body += bytes(body_source[:size])
                del body_source[: size + 2]
        elif "content-length" in lower:
            length = int(lower["content-length"])
            while len(body_source) < length:
                body_source.extend(self.recv_more(conn))
            body = bytes(body_source[:length])
            del body_source[:length]
        buffer.clear()
        buffer.extend(body_source)
        return {
            "method": method,
            "target": target,
            "path": target.split("?")[0],
            "query": target.split("?")[1] if "?" in target else "",
            "version": version,
            "headers": lower,
            "headers_multi": headers,
            "body": body.decode("utf-8", "replace"),
        }

    @staticmethod
    def recv_more(conn: socket.socket) -> bytes:
        chunk = conn.recv(65536)
        if not chunk:
            raise EOFError("upstream connection closed mid-request")
        return chunk

    def respond(self, conn: socket.socket, req: dict) -> bool:
        lower = req["headers"]
        connection_tokens = {
            token.strip().lower()
            for token in lower.get("connection", "").split(",")
            if token.strip()
        }
        close = "close" in connection_tokens or req["version"] == "HTTP/1.0"
        path = req["path"]
        # proxy_pass replaces only the matched location prefix, so upstream
        # paths carry the request remainder (/who0, /text/x, ...). Dispatch
        # and hit accounting use the leading route segment.
        route_key = "/" + path.lstrip("/").split("/")[0]

        def send(status: str, content_type: str, payload: bytes, extra: list[tuple[str, str]] | None = None, chunked: bool = False) -> None:
            head = [f"HTTP/1.1 {status}", f"Content-Type: {content_type}"]
            if chunked:
                head.append("Transfer-Encoding: chunked")
            else:
                head.append(f"Content-Length: {len(payload)}")
            for name, value in extra or []:
                head.append(f"{name}: {value}")
            head.append("Connection: close" if close else "Connection: keep-alive")
            conn.sendall(("\r\n".join(head) + "\r\n\r\n").encode("latin-1"))
            if chunked:
                return  # body written by the route
            conn.sendall(payload)

        def record_upstream_request(endpoint: str) -> bytes:
            introspection = {
                "backend": self.name,
                "method": req["method"],
                "path": req["path"],
                "query": req["query"],
                "target_raw": req["target"],
                "headers": req["headers"],
                "body": req["body"],
                "endpoint": endpoint,
            }
            return json.dumps(introspection).encode()

        if lower.get("upgrade", "").lower() == "websocket":
            key = lower.get("sec-websocket-key", "")
            accept = base64.b64encode(hashlib.sha1((key + WEBSOCKET_GUID).encode()).digest()).decode()
            conn.sendall(
                (
                    "HTTP/1.1 101 Switching Protocols\r\n"
                    "Upgrade: websocket\r\n"
                    f"Sec-WebSocket-Accept: {accept}\r\n"
                    "Connection: Upgrade\r\n\r\n"
                ).encode("ascii")
            )
            self.websocket_echo(conn)
            return False
        if route_key == "/stream":
            send("200 OK", "text/plain", b"", chunked=True)
            for piece in (b"chunk-one\n", b"chunk-two\n", b"chunk-three\n"):
                conn.sendall(f"{len(piece):x}\r\n".encode() + piece + b"\r\n")
                time.sleep(0.35)
            conn.sendall(b"0\r\n\r\n")
            return close
        if route_key == "/slow":
            time.sleep(1.2)
            payload = json.dumps({"backend": self.name, "slow": True}).encode()
            send("200 OK", "application/json", payload)
            return close
        if route_key == "/text":
            send("200 OK", "text/plain", b"alpha alpha beta alpha")
            return close
        if route_key == "/who":
            payload = json.dumps({"backend": self.name, "path": req["target"], "hits": self.hit_count(route_key)}).encode()
            send("200 OK", "application/json", payload)
            return close
        if path.startswith("/status/"):
            code = path.split("/")[2]
            text = "induced"
            send(f"{code} Induced", "text/plain", text.encode())
            return close
        payload = record_upstream_request("introspect")
        send("200 OK", "application/json", payload)
        return close

    @staticmethod
    def websocket_echo(conn: socket.socket) -> None:
        conn.settimeout(5)
        while True:
            head = _read_bytes(conn, 2, 5)
            opcode = head[0] & 0x0F
            masked = bool(head[1] & 0x80)
            length = head[1] & 0x7F
            if length == 126:
                length = int.from_bytes(_read_bytes(conn, 2, 5), "big")
            elif length == 127:
                length = int.from_bytes(_read_bytes(conn, 8, 5), "big")
            mask = _read_bytes(conn, 4, 5) if masked else None
            payload = _read_bytes(conn, length, 5) if length else b""
            if mask:
                payload = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
            if opcode == 0x8:
                return
            if opcode in (0x1, 0x2):
                frame = bytes([0x80 | opcode]) + (
                    bytes([len(payload)]) if len(payload) < 126 else b"\x7e" + len(payload).to_bytes(2, "big")
                )
                conn.sendall(frame + payload)


class StreamEcho(threading.Thread):
    """Plain TCP echo upstream for the stream{} block."""

    def __init__(self, port: int):
        super().__init__(daemon=True, name="stream-echo")
        self.port = port
        self.listener = socket.socket()
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(16)
        self.ready = threading.Event()

    def run(self) -> None:
        self.ready.set()
        while True:
            try:
                conn, _ = self.listener.accept()
            except OSError:
                return
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    @staticmethod
    def serve(conn: socket.socket) -> None:
        with conn:
            conn.settimeout(8)
            while True:
                try:
                    data = conn.recv(65536)
                except (socket.timeout, OSError):
                    return
                if not data:
                    return
                conn.sendall(data)


# --------------------------------------------------------------------------
# TLS / WebSocket helpers
# --------------------------------------------------------------------------


def generate_cert(common_name: str, cert_path: Path, key_path: Path) -> None:
    subprocess.run(
        [
            "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
            "-keyout", str(key_path), "-out", str(cert_path), "-days", "2",
            "-subj", f"/CN={common_name}",
            "-addext", f"subjectAltName=DNS:{common_name},DNS:localhost,IP:127.0.0.1",
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env={**os.environ, "MSYS_NO_PATHCONV": "1"},
    )


def write_htpasswd(path: Path, username: str, password: str) -> None:
    digest = base64.b64encode(hashlib.sha1(password.encode()).digest()).decode()
    path.write_text(f"{username}:{{SHA}}{digest}\n", encoding="ascii")


def tls_get(port: int, server_hostname: str, target: str, alpn: list[str] | None = None):
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    if alpn:
        context.set_alpn_protocols(alpn)
    with socket.create_connection(("127.0.0.1", port), timeout=6) as tcp:
        with context.wrap_socket(tcp, server_hostname=server_hostname) as tls:
            selected = tls.selected_alpn_protocol()
            der = tls.getpeercert(binary_form=True)
            tls.sendall(
                (
                    f"GET {target} HTTP/1.1\r\n"
                    f"Host: {server_hostname}\r\n"
                    "Connection: close\r\n\r\n"
                ).encode("ascii")
            )
            response = read_one_response(tls, 6)
            return response, selected, der


def h2_settings_exchange(port: int, server_hostname: str) -> str:
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    context.set_alpn_protocols(["h2"])
    with socket.create_connection(("127.0.0.1", port), timeout=6) as tcp:
        with context.wrap_socket(tcp, server_hostname=server_hostname) as tls:
            if tls.selected_alpn_protocol() != "h2":
                return f"alpn={tls.selected_alpn_protocol()!r}"
            preface = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"
            empty_settings = b"\x00\x00\x00\x04\x00\x00\x00\x00\x00"
            tls.sendall(preface + empty_settings)
            deadline = time.monotonic() + 4
            saw_settings = False
            saw_ack = False
            while time.monotonic() < deadline and not (saw_settings and saw_ack):
                tls.settimeout(max(0.1, deadline - time.monotonic()))
                header = _read_bytes(tls, 9, 4)
                length = int.from_bytes(header[0:3], "big")
                frame_type = header[3]
                flags = header[4]
                payload = _read_bytes(tls, length, 4) if length else b""
                if frame_type == 0x4 and not (flags & 0x1):
                    saw_settings = True
                    ack = b"\x00\x00\x00\x04\x01" + header[5:9]
                    tls.sendall(ack)
                elif frame_type == 0x4 and flags & 0x1:
                    saw_ack = True
            if not saw_settings:
                return "no SETTINGS from server"
            if not saw_ack:
                return "no SETTINGS ACK from server"
            return "ok"


def websocket_roundtrip(port: int, host: str, target: str) -> str:
    with socket.create_connection(("127.0.0.1", port), timeout=6) as sock:
        sock.settimeout(6)
        key = base64.b64encode(os.urandom(16)).decode()
        request = (
            f"GET {target} HTTP/1.1\r\n"
            f"Host: {host}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n"
        )
        sock.sendall(request.encode("ascii"))
        head = _read_until_headers_end(sock, 6)
        status_line = head.decode("latin-1").split("\r\n", 1)[0]
        if " 101 " not in status_line + " ":
            return f"handshake: {status_line}"
        headers = {
            name.strip().lower(): value.strip()
            for name, _, value in (line.partition(":") for line in head.decode("latin-1").split("\r\n")[1:])
        }
        expected = base64.b64encode(hashlib.sha1((key + WEBSOCKET_GUID).encode()).digest()).decode()
        if headers.get("sec-websocket-accept") != expected:
            return f"bad Sec-WebSocket-Accept {headers.get('sec-websocket-accept')!r}"
        payload = b"hello-ws-full-surface"
        mask = os.urandom(4)
        masked = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
        frame = bytes([0x81, 0x80 | len(payload)]) + mask + masked
        sock.sendall(frame)
        echo_head = _read_bytes(sock, 2, 6)
        length = echo_head[1] & 0x7F
        echoed = _read_bytes(sock, length, 6) if length else b""
        if echoed != payload:
            return f"echo mismatch {echoed!r}"
        return "ok"


# --------------------------------------------------------------------------
# Case runner
# --------------------------------------------------------------------------


class CaseFailure(AssertionError):
    pass


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise CaseFailure(message)


class Context:
    def __init__(self, ports: dict[str, int]):
        self.ports = ports
        self.echo: MockHttpBackend | None = None
        self.target_a: MockHttpBackend | None = None
        self.target_b: MockHttpBackend | None = None
        self.target_backup: MockHttpBackend | None = None

    @property
    def main(self) -> int:
        return self.ports["main"]

    @property
    def lb(self) -> int:
        return self.ports["lb"]

    @property
    def tls(self) -> int:
        return self.ports["tls"]

    @property
    def stream(self) -> int:
        return self.ports["stream"]


def wait_static_root_ready(port: int) -> None:
    deadline = time.monotonic() + 10
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            response = http(port, "/index.html", host="static.example.com", timeout=1.0)
            if response.status == 200:
                return
        except OSError as error:
            last_error = error
        time.sleep(0.1)
    raise RuntimeError(f"server did not become ready: {last_error}")


# ---- routing / static / URI semantics ----


def case_default_server_unknown_host(ctx: Context) -> None:
    response = http(ctx.main, "/exact", host="nosuch.invalid")
    expect(response.status == 200, f"status {response.status}")
    expect(response.text() == "exact", f"body {response.text()!r}")


def case_http10_without_host(ctx: Context) -> None:
    with socket.create_connection(("127.0.0.1", ctx.main), timeout=5) as sock:
        sock.settimeout(5)
        sock.sendall(b"GET /exact HTTP/1.0\r\n\r\n")
        response = read_one_response(sock, 5)
    expect(response.status == 200, f"status {response.status}")
    expect(response.text() == "exact", f"body {response.text()!r}")


def case_exact_location_beats_prefix(ctx: Context) -> None:
    exact = http(ctx.main, "/exact")
    prefix = http(ctx.main, "/exact/sub")
    expect(exact.text() == "exact", f"exact body {exact.text()!r}")
    expect(prefix.text() == "prefix", f"prefix body {prefix.text()!r}")


def case_uri_normalization_matrix(ctx: Context) -> None:
    # Route selection uses the canonical (decoded, merged, dot-resolved)
    # path while the query string stays out of the match.
    canonical_variants = [
        "/norm",
        "/norm?a=1",
        "//norm",
        "/a/../norm",
        "/nor%6d",
    ]
    for raw in canonical_variants:
        response = http(ctx.main, raw)
        expect(response.status == 200, f"{raw}: status {response.status}")
        expect(response.text() == "norm-ok", f"{raw}: body {response.text()!r}")
    trailing = http(ctx.main, "/norm/")
    expect(trailing.status == 404, f"/norm/ status {trailing.status}")


def case_wildcard_vhost(ctx: Context) -> None:
    response = http(ctx.main, "/anything", host="foo.example.com")
    expect(response.status == 200, f"status {response.status}")
    expect(response.text() == "wild", f"body {response.text()!r}")


def case_regex_locations(ctx: Context) -> None:
    png = http(ctx.main, "/pic.png")
    ico = http(ctx.main, "/pic.ICO")
    png_upper = http(ctx.main, "/pic.PNG")
    expect(png.text() == "re-png", f"/pic.png body {png.text()!r}")
    expect(ico.text() == "re-ico", f"/pic.ICO body {ico.text()!r}")
    expect(png_upper.status == 404, f"/pic.PNG (case-sensitive) status {png_upper.status}")


def case_caret_tilde_suppresses_regex(ctx: Context) -> None:
    response = http(ctx.main, "/assets/pic.png")
    expect(response.status == 200, f"status {response.status}")
    expect(body_text(response) == "PNG-DATA\n", f"body {response.text()!r}")


def case_rewrite_last_with_captures(ctx: Context) -> None:
    response = http(ctx.main, "/cap/alpha/42")
    expect(response.status == 200, f"status {response.status}")
    introspection = json.loads(response.text())
    expect(
        introspection["path"] == "/echo/cap-alpha-num-42",
        f"upstream path {introspection['path']!r}",
    )


def case_rewrite_permanent_and_redirect(ctx: Context) -> None:
    permanent = http(ctx.main, "/perm/doc1")
    redirect = http(ctx.main, "/redir/doc2")
    expect(permanent.status == 301, f"permanent status {permanent.status}")
    expect(
        (permanent.header("Location") or "").endswith("/p/doc1"),
        f"permanent Location {permanent.header('Location')!r}",
    )
    expect(redirect.status == 302, f"redirect status {redirect.status}")
    expect(
        "/goto/doc2" in (redirect.header("Location") or ""),
        f"redirect Location {redirect.header('Location')!r}",
    )


def case_rewrite_break_serves_static(ctx: Context) -> None:
    response = http(ctx.main, "/brk/small.txt")
    expect(response.status == 200, f"status {response.status}")
    expect(body_text(response) == "tiny\n", f"body {response.text()!r}")


def case_try_files_spa_fallback(ctx: Context) -> None:
    fallback = http(ctx.main, "/app/nope.txt")
    real = http(ctx.main, "/app/real.txt")
    directory = http(ctx.main, "/app/")
    expect(fallback.status == 200 and "spa-shell" in fallback.text(), f"fallback {fallback.status} {fallback.text()!r}")
    expect(body_text(real) == "real-app-file\n", f"real body {real.text()!r}")
    expect(directory.status == 200 and "spa-shell" in directory.text(), f"dir index {directory.status} {directory.text()!r}")


def case_alias_substitution(ctx: Context) -> None:
    response = http(ctx.main, "/docs/readme.md")
    expect(response.status == 200, f"status {response.status}")
    expect(body_text(response) == "# full-surface docs\n", f"body {response.text()!r}")


def case_index_directive(ctx: Context) -> None:
    response = http(ctx.main, "/")
    expect(response.status == 200, f"status {response.status}")
    expect("full-surface-index" in response.text(), f"body {response.text()!r}")


def case_static_404(ctx: Context) -> None:
    response = http(ctx.main, "/no-such-file.txt")
    expect(response.status == 404, f"status {response.status}")


def case_static_head(ctx: Context) -> None:
    with socket.create_connection(("127.0.0.1", ctx.main), timeout=5) as sock:
        sock.settimeout(5)
        sock.sendall(b"HEAD /big.txt HTTP/1.1\r\nHost: static.example.com\r\nConnection: close\r\n\r\n")
        head = _read_until_headers_end(sock, 5)
        lower = {
            name.strip().lower(): value.strip()
            for name, _, value in (line.partition(":") for line in head.decode("latin-1").split("\r\n")[1:])
        }
        try:
            extra = sock.recv(4096)
        except socket.timeout:
            extra = b""
    expect(" 200 " in head.decode("latin-1").split("\r\n", 1)[0], "HEAD status not 200")
    expect(int(lower.get("content-length", "0")) > 0, f"HEAD Content-Length {lower.get('content-length')!r}")
    expect(extra == b"", f"HEAD must not carry a body, got {extra!r}")


def case_static_range(ctx: Context) -> None:
    response = http(ctx.main, "/big.txt", headers=[("Range", "bytes=0-3")])
    expect(response.status == 206, f"status {response.status}")
    expect(response.body == b"big-", f"body {response.body!r}")
    content_range = response.header("Content-Range") or ""
    expect(content_range.startswith("bytes 0-3/"), f"Content-Range {content_range!r}")


def case_static_conditional(ctx: Context) -> None:
    first = http(ctx.main, "/big.txt")
    etag = first.header("ETag")
    last_modified = first.header("Last-Modified")
    expect(etag is not None or last_modified is not None, "static response has neither ETag nor Last-Modified")
    if etag is not None:
        conditional = http(ctx.main, "/big.txt", headers=[("If-None-Match", etag)])
    else:
        conditional = http(ctx.main, "/big.txt", headers=[("If-Modified-Since", last_modified or "")])
    expect(conditional.status == 304, f"conditional status {conditional.status}")
    expect(conditional.body == b"", f"304 must not carry a body, got {conditional.body!r}")


def case_static_mime_types(ctx: Context) -> None:
    html = http(ctx.main, "/index.html")
    text = http(ctx.main, "/big.txt")
    markdown = http(ctx.main, "/docs/readme.md")
    expect("html" in (html.header("Content-Type") or ""), f"html Content-Type {html.header('Content-Type')!r}")
    expect((text.header("Content-Type") or "").startswith("text/plain"), f"txt Content-Type {text.header('Content-Type')!r}")
    expect("markdown" in (markdown.header("Content-Type") or ""), f"md Content-Type {markdown.header('Content-Type')!r}")


def _http_date(value: str) -> datetime.datetime:
    """Parse an HTTP-date response header into an aware UTC datetime."""
    return datetime.datetime.strptime(value, "%a, %d %b %Y %H:%M:%S GMT").replace(
        tzinfo=datetime.timezone.utc
    )


def _utc_now() -> datetime.datetime:
    return datetime.datetime.now(datetime.timezone.utc)


def case_expires_relative(ctx: Context) -> None:
    response = http(ctx.main, "/fresh/big.txt")
    expect(response.status == 200, f"status {response.status}")
    expect(
        response.header("Cache-Control") == "max-age=3600",
        f"Cache-Control {response.header('Cache-Control')!r}",
    )
    expires = response.header("Expires")
    expect(expires is not None, "`expires 1h` must emit an Expires header")
    delta = (_http_date(expires) - _utc_now()).total_seconds()
    expect(3595 <= delta <= 3600, f"Expires is {delta}s ahead, expected ~3600")


def case_expires_absolute_modes(ctx: Context) -> None:
    epoch = http(ctx.main, "/epoch/big.txt")
    expect(
        epoch.header("Expires") == "Thu, 01 Jan 1970 00:00:01 GMT",
        f"epoch Expires {epoch.header('Expires')!r}",
    )
    expect(
        epoch.header("Cache-Control") == "no-cache",
        f"epoch Cache-Control {epoch.header('Cache-Control')!r}",
    )
    maxed = http(ctx.main, "/max/big.txt")
    expect(
        maxed.header("Expires") == "Thu, 31 Dec 2037 23:55:55 GMT",
        f"max Expires {maxed.header('Expires')!r}",
    )
    expect(
        maxed.header("Cache-Control") == "max-age=315360000",
        f"max Cache-Control {maxed.header('Cache-Control')!r}",
    )


def case_expires_negative(ctx: Context) -> None:
    response = http(ctx.main, "/stale/big.txt")
    expect(response.status == 200, f"status {response.status}")
    expect(
        response.header("Cache-Control") == "no-cache",
        f"negative expires must be no-cache, got {response.header('Cache-Control')!r}",
    )
    expires = response.header("Expires")
    expect(expires is not None, "a negative expires still writes Expires in the past")
    delta = (_http_date(expires) - _utc_now()).total_seconds()
    expect(-3605 <= delta <= -3590, f"Expires is {delta}s ahead, expected ~-3600")


def case_expires_daily(ctx: Context) -> None:
    response = http(ctx.main, "/daily/big.txt")
    cache_control = response.header("Cache-Control") or ""
    expect(cache_control.startswith("max-age="), f"Cache-Control {cache_control!r}")
    max_age = int(cache_control.split("=", 1)[1])
    expect(0 < max_age <= 86400, f"daily max-age {max_age} must be inside one day")
    expires = _http_date(response.header("Expires"))
    expect(
        (expires.hour, expires.minute, expires.second) == (0, 0, 0),
        f"`expires @0` must land on midnight, got {expires.isoformat()}",
    )
    expect(
        abs((expires - _utc_now()).total_seconds() - max_age) <= 1,
        f"max-age {max_age} disagrees with Expires {expires.isoformat()}",
    )


def case_expires_modified(ctx: Context) -> None:
    # `expires modified` anchors on the document's Last-Modified, not on the
    # response time. The fixture document is checked in, so its mtime is
    # permanently in the past and `Last-Modified + 1h` is already stale. nginx
    # still publishes the computed `Expires` and only downgrades
    # `Cache-Control` to `no-cache` (ngx_http_set_expires formats the header
    # before testing `conf->expires_time < 0 || max_age < 0`), which is also
    # what distinguishes `modified` from `access` here: an `access` policy would
    # have published `now + 1h` instead of the document's own timestamp.
    # The fresh branch (positive remaining max-age) is pinned by the
    # `modified_expires_uses_last_modified_and_can_go_stale` unit test.
    response = http(ctx.main, "/fromlm/big.txt")
    expect(response.status == 200, f"status {response.status}")
    last_modified = response.header("Last-Modified")
    expect(last_modified is not None, "static response must carry Last-Modified")
    raw_expires = response.header("Expires")
    expect(raw_expires is not None, "a `modified` policy must still publish Expires")
    expected = _http_date(last_modified) + datetime.timedelta(hours=1)
    expires = _http_date(raw_expires)
    expect(
        expires == expected,
        f"Expires {expires.isoformat()} != Last-Modified + 1h {expected.isoformat()}",
    )
    cache_control = response.header("Cache-Control") or ""
    expect(
        cache_control == "no-cache",
        f"a stale `modified` policy must answer no-cache, got {cache_control!r}",
    )
    # Cross-check the anchoring: an `access` policy would expire a full hour
    # after the response, so a now-relative Expires means the mode was lost.
    now = _utc_now()
    expect(
        abs((expires - now).total_seconds()) > 60,
        f"a `modified` policy must not be relative to the response time ({expires.isoformat()})",
    )


def case_expires_zero_shortcut(ctx: Context) -> None:
    # nginx returns early when the parsed value is zero (and the mode is not
    # `daily`): `Expires` is the response time itself and `Cache-Control` is
    # `max-age=0`, never the ACCESS arithmetic. `expires -0` lands here too,
    # because the sign is applied after the zero test.
    before = _utc_now()
    response = http(ctx.main, "/zero/big.txt")
    after = _utc_now()
    expect(response.status == 200, f"status {response.status}")
    raw_expires = response.header("Expires")
    expect(raw_expires is not None, "the zero shortcut must publish Expires")
    expires = _http_date(raw_expires)
    expect(
        before - datetime.timedelta(seconds=2)
        <= expires
        <= after + datetime.timedelta(seconds=2),
        f"the zero shortcut must publish the response time, got {expires.isoformat()}",
    )
    cache_control = response.header("Cache-Control") or ""
    expect(cache_control == "max-age=0", f"Cache-Control {cache_control!r}")


def case_etag_off_suppresses_generation(ctx: Context) -> None:
    response = http(ctx.main, "/noetag/big.txt")
    expect(response.status == 200, f"status {response.status}")
    expect(
        not response.has_header("ETag"),
        f"`etag off` must suppress the entity tag, got {response.header('ETag')!r}",
    )
    expect(response.has_header("Last-Modified"), "Last-Modified is independent of etag")
    # With no tag to compare, a concrete If-None-Match cannot match.
    tagged = http(
        ctx.main, "/noetag/big.txt", headers=[("If-None-Match", 'W/"deadbeef-1"')]
    )
    expect(tagged.status == 200, f"an unmatched tag must serve the body, got {tagged.status}")
    # `*` still matches because the representation exists.
    star = http(ctx.main, "/noetag/big.txt", headers=[("If-None-Match", "*")])
    expect(star.status == 304, f"`If-None-Match: *` must still answer 304, got {star.status}")
    # The default is nginx's `etag on`.
    default = http(ctx.main, "/fresh/big.txt")
    expect(default.has_header("ETag"), "an undeclared location keeps etag on")


def case_if_modified_since_off_ignores_the_condition(ctx: Context) -> None:
    future = "Fri, 31 Dec 9999 23:59:59 GMT"
    ignored = http(ctx.main, "/ims-off/big.txt", headers=[("If-Modified-Since", future)])
    expect(
        ignored.status == 200,
        f"`if_modified_since off` must ignore the condition, got {ignored.status}",
    )
    # The same header under the default policy (nginx `exact`) must not produce
    # a 304 either, because the condition differs from Last-Modified.
    exact = http(ctx.main, "/ims-exact/big.txt", headers=[("If-Modified-Since", future)])
    expect(
        exact.status == 200,
        f"`exact` must not treat a different date as unchanged, got {exact.status}",
    )


def case_if_modified_since_exact_default(ctx: Context) -> None:
    first = http(ctx.main, "/ims-exact/big.txt")
    last_modified = first.header("Last-Modified")
    expect(last_modified is not None, "static response must carry Last-Modified")
    same = http(
        ctx.main, "/ims-exact/big.txt", headers=[("If-Modified-Since", last_modified)]
    )
    expect(same.status == 304, f"the exact date must answer 304, got {same.status}")
    older = _http_date(last_modified) - datetime.timedelta(seconds=1)
    stale = http(
        ctx.main,
        "/ims-exact/big.txt",
        headers=[("If-Modified-Since", older.strftime("%a, %d %b %Y %H:%M:%S GMT"))],
    )
    expect(
        stale.status == 200,
        f"an older condition must serve the body under `exact`, got {stale.status}",
    )


def case_proxy_response_carries_expires(ctx: Context) -> None:
    response = http(ctx.main, "/expires-proxy/echo/any", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    expect(
        response.header("Cache-Control") == "max-age=7200",
        f"proxied Cache-Control {response.header('Cache-Control')!r}",
    )
    delta = (_http_date(response.header("Expires")) - _utc_now()).total_seconds()
    expect(7195 <= delta <= 7200, f"proxied Expires is {delta}s ahead, expected ~7200")


def case_add_header_present(ctx: Context) -> None:
    response = http(ctx.main, "/exact")
    expect(response.header("X-Served-By") == "full-surface", f"X-Served-By {response.header('X-Served-By')!r}")


def case_secure_link_secret(ctx: Context) -> None:
    digest = hashlib.md5(b"s3cretreport.pdf").hexdigest()
    valid = http(ctx.main, f"/files/{digest}/report.pdf")
    invalid = http(ctx.main, "/files/beef/report.pdf")
    expect(valid.status == 200, f"valid status {valid.status}")
    expect(body_text(valid) == "PDF-DATA\n", f"valid body {valid.text()!r}")
    expect(invalid.status == 403, f"invalid status {invalid.status}")


# ---- proxying ----


def case_proxy_path_and_query_preserved(ctx: Context) -> None:
    response = http(ctx.main, "/echo/deep/path?q=1&x=%20", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    introspection = json.loads(response.text())
    expect(introspection["path"] == "/echo/deep/path", f"path {introspection['path']!r}")
    expect(introspection["query"] == "q=1&x=%20", f"query {introspection['query']!r}")


def case_proxy_encoded_slash_preserved(ctx: Context) -> None:
    response = http(ctx.main, "/echo/a%2Fb", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    introspection = json.loads(response.text())
    expect("%2F" in introspection["target_raw"], f"raw target {introspection.get('target_raw')!r}")


def case_proxy_pass_uri_replacement(ctx: Context) -> None:
    response = http(ctx.main, "/api/users?id=7", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    introspection = json.loads(response.text())
    expect(introspection["path"] == "/v1/users", f"upstream path {introspection['path']!r}")
    expect(introspection["query"] == "id=7", f"upstream query {introspection['query']!r}")


def case_proxy_forwards_method_and_body(ctx: Context) -> None:
    response = http(ctx.main, "/echo/submit", method="POST", body=b"hello-proxy", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    introspection = json.loads(response.text())
    expect(introspection["method"] == "POST", f"method {introspection['method']!r}")
    expect(introspection["body"] == "hello-proxy", f"body {introspection['body']!r}")


def case_proxy_default_forwarded_headers(ctx: Context) -> None:
    response = http(ctx.main, "/echo/defaults", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    introspection = json.loads(response.text())
    headers = introspection["headers"]
    xff = headers.get("x-forwarded-for", "")
    expect("127.0.0.1" in xff, f"x-forwarded-for {xff!r}")
    expect(headers.get("x-forwarded-proto") == "http", f"x-forwarded-proto {headers.get('x-forwarded-proto')!r}")
    expect(headers.get("te") == "trailers", f"te {headers.get('te')!r}")


def case_proxy_set_header_server_level(ctx: Context) -> None:
    response = http(ctx.main, "/hdr-inherit/one", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    headers = json.loads(response.text())["headers"]
    expect(headers.get("x-level") == "server", f"x-level {headers.get('x-level')!r}")
    expect(headers.get("x-custom-hello") == "h-proxy.example.com", f"x-custom-hello {headers.get('x-custom-hello')!r}")


def case_proxy_set_header_child_replaces(ctx: Context) -> None:
    response = http(ctx.main, "/hdr-loc/one", host="proxy.example.com")
    expect(response.status == 200, f"status {response.status}")
    headers = json.loads(response.text())["headers"]
    expect(headers.get("x-level") == "location", f"x-level {headers.get('x-level')!r}")
    expect("x-custom-hello" not in headers, f"inherited header leaked: {headers.get('x-custom-hello')!r}")


def case_proxy_pass_request_headers_off(ctx: Context) -> None:
    response = http(
        ctx.main,
        "/hdr-strip/one",
        headers=[("X-Client-Echo", "ping")],
        host="proxy.example.com",
    )
    expect(response.status == 200, f"status {response.status}")
    headers = json.loads(response.text())["headers"]
    expect("x-client-echo" not in headers, f"client header leaked upstream: {headers.get('x-client-echo')!r}")


def case_proxy_hop_by_hop_stripped(ctx: Context) -> None:
    response = http(
        ctx.main,
        "/echo/hops",
        headers=[("TE", "trailers"), ("Keep-Alive", "timeout=5"), ("X-Client-Echo", "hop")],
        host="proxy.example.com",
    )
    expect(response.status == 200, f"status {response.status}")
    headers = json.loads(response.text())["headers"]
    expect("te" not in headers or headers.get("te") == "trailers", f"client TE leaked: {headers.get('te')!r}")
    expect("keep-alive" not in headers, f"Keep-Alive leaked: {headers.get('keep-alive')!r}")
    expect(headers.get("x-client-echo") == "hop", f"end-to-end header lost: {headers.get('x-client-echo')!r}")


def case_variable_proxy_pass(ctx: Context) -> None:
    response = http(
        ctx.main,
        "/dyn/echo",
        headers=[("X-Upstream-Authority", f"127.0.0.1:{ctx.ports['echo']}")],
        host="proxy.example.com",
    )
    expect(response.status == 200, f"status {response.status} body {response.text()!r}")
    introspection = json.loads(response.text())
    expect(introspection["path"] == "/dyn/echo", f"upstream path {introspection['path']!r}")


def case_request_body_limit_413(ctx: Context) -> None:
    response = http(ctx.main, "/upload/big", method="POST", body=b"x" * 2048, host="proxy.example.com")
    expect(response.status == 413, f"status {response.status}")


def case_chunked_request_body(ctx: Context) -> None:
    with socket.create_connection(("127.0.0.1", ctx.main), timeout=6) as sock:
        sock.settimeout(6)
        pieces = [b"chunk-a ", b"chunk-b"]
        head = (
            "POST /chunk/x HTTP/1.1\r\n"
            "Host: proxy.example.com\r\n"
            "Transfer-Encoding: chunked\r\n"
            "Connection: close\r\n\r\n"
        )
        sock.sendall(head.encode("ascii"))
        for piece in pieces:
            sock.sendall(f"{len(piece):x}\r\n".encode() + piece + b"\r\n")
        sock.sendall(b"0\r\n\r\n")
        response = read_one_response(sock, 6)
    expect(response.status == 200, f"status {response.status}")
    introspection = json.loads(response.text())
    expect(introspection["body"] == "chunk-a chunk-b", f"upstream body {introspection['body']!r}")


def case_expect_100_continue(ctx: Context) -> None:
    with socket.create_connection(("127.0.0.1", ctx.main), timeout=6) as sock:
        sock.settimeout(6)
        sock.sendall(
            (
                "POST /echo/continue HTTP/1.1\r\n"
                "Host: proxy.example.com\r\n"
                "Content-Length: 5\r\n"
                "Expect: 100-continue\r\n"
                "Connection: close\r\n\r\n"
            ).encode("ascii")
        )
        interim, _ = read_one_response(sock, 4, want_interim=True)
        expect(interim.status == 100, f"interim status {interim.status}")
        sock.sendall(b"hello")
        response = read_one_response(sock, 6)
    expect(response.status == 200, f"final status {response.status}")


def case_proxy_cache(ctx: Context) -> None:
    echo = ctx.echo
    assert echo is not None
    before = echo.hit_count("/who")
    first = http(ctx.main, "/cached/u1", host="proxy.example.com")
    second = http(ctx.main, "/cached/u1", host="proxy.example.com")
    miss = http(ctx.main, "/cached/u1?v=2", host="proxy.example.com")
    expect(first.status == 200 and second.status == 200 and miss.status == 200, "cache responses not 200")
    after = echo.hit_count("/who")
    expect(after == before + 2, f"upstream hits before={before} after={after} (want +2: one miss + one keyed miss)")


def case_proxy_streaming_incremental(ctx: Context) -> None:
    # The upstream emits chunked framing with producer gaps. nginx with
    # default buffering relays the complete body; assert relay fidelity
    # (chunk order, bytes, terminal framing) rather than arrival timing,
    # which depends on the engine's bounded buffer thresholds.
    with socket.create_connection(("127.0.0.1", ctx.main), timeout=8) as sock:
        sock.settimeout(8)
        sock.sendall(
            (
                "GET /stream/x HTTP/1.1\r\n"
                "Host: proxy.example.com\r\n"
                "Connection: close\r\n\r\n"
            ).encode("ascii")
        )
        arrivals: list[float] = []

        def sink(payload: bytes) -> None:
            arrivals.append(time.monotonic())

        head = _read_until_headers_end(sock, 8)
        status_line = head.split(b"\r\n", 1)[0].decode("latin-1")
        expect(" 200 " in status_line, f"stream status {status_line!r}")
        lower = {
            name.strip().lower(): value.strip()
            for name, _, value in (line.partition(":") for line in head.decode("latin-1").split("\r\n")[1:])
        }
        if "transfer-encoding" in lower and "chunked" in lower["transfer-encoding"]:
            body = _read_chunked_body(sock, 8, sink)
        else:
            body = _read_bytes(sock, int(lower.get("content-length", "0")), 8)
    expect(
        body == b"chunk-one\nchunk-two\nchunk-three\n",
        f"relayed body {body!r}",
    )


def case_websocket_tunnel(ctx: Context) -> None:
    result = websocket_roundtrip(ctx.main, "proxy.example.com", "/ws/chat")
    expect(result == "ok", f"websocket roundtrip: {result}")


# ---- load balancing ----


def case_weighted_round_robin(ctx: Context) -> None:
    counts = {"target-a": 0, "target-b": 0}
    for index in range(8):
        response = http(ctx.lb, f"/rr/{index}", host="lb.example.com")
        expect(response.status == 200, f"request {index} status {response.status}")
        backend = json.loads(response.text())["backend"]
        expect(backend in counts, f"unexpected backend {backend!r}")
        counts[backend] += 1
    expect(counts == {"target-a": 6, "target-b": 2}, f"weight 3:1 over 8 gave {counts}")


def case_least_conn_avoids_busy_target(ctx: Context) -> None:
    holder: dict[str, object] = {}

    def hold() -> None:
        holder["response"] = http(ctx.lb, "/slow/hold", host="lb.example.com", timeout=8)

    thread = threading.Thread(target=hold, daemon=True)
    thread.start()
    time.sleep(0.4)
    busy_backend = None
    fast_backends = []
    for index in range(3):
        response = http(ctx.lb, f"/lc/fast-{index}", host="lb.example.com")
        expect(response.status == 200, f"fast {index} status {response.status}")
        fast_backends.append(json.loads(response.text())["backend"])
    thread.join(timeout=8)
    held = holder.get("response")
    expect(isinstance(held, Response) and held.status == 200, f"held request failed: {held!r}")
    if isinstance(held, Response):
        busy_backend = json.loads(held.text())["backend"]
    other = "target-b" if busy_backend == "target-a" else "target-a"
    expect(
        all(name == other for name in fast_backends),
        f"busy={busy_backend} fast went to {fast_backends}",
    )


def case_ip_hash_affinity(ctx: Context) -> None:
    backends = set()
    for index in range(6):
        response = http(ctx.lb, f"/iphash/{index}", host="lb.example.com")
        expect(response.status == 200, f"request {index} status {response.status}")
        backends.add(json.loads(response.text())["backend"])
    expect(len(backends) == 1, f"ip_hash split one client across {sorted(backends)}")


def case_hash_consistent_affinity(ctx: Context) -> None:
    for key in ("stable-1", "stable-2"):
        seen = set()
        for _ in range(2):
            response = http(ctx.lb, f"/hash/{key}", host="lb.example.com")
            expect(response.status == 200, f"{key} status {response.status}")
            seen.add(json.loads(response.text())["backend"])
        expect(len(seen) == 1, f"hash {key} not stable: {sorted(seen)}")


def case_backup_failover(ctx: Context) -> None:
    first = http(ctx.lb, "/backup/one", host="lb.example.com", timeout=10)
    second = http(ctx.lb, "/backup/two", host="lb.example.com")
    expect(first.status == 200, f"first status {first.status} body {first.text()!r}")
    expect(second.status == 200, f"second status {second.status} body {second.text()!r}")
    expect(json.loads(first.text())["backend"] == "target-backup", f"first backend {first.text()!r}")
    expect(json.loads(second.text())["backend"] == "target-backup", f"second backend {second.text()!r}")


# ---- response filters ----


def case_gzip_large_text(ctx: Context) -> None:
    import gzip as gzip_module

    response = http(ctx.main, "/gz/big.txt", headers=[("Accept-Encoding", "gzip")], host="filters.example.com")
    expect(response.status == 200, f"status {response.status}")
    encoding = response.header("Content-Encoding")
    expect(encoding == "gzip", f"Content-Encoding {encoding!r}")
    payload = response.body
    content_length_gzip = response.header("Content-Length")
    if content_length_gzip is not None and int(content_length_gzip) != len(payload):
        raise CaseFailure("Content-Length does not match body length")
    decoded = gzip_module.decompress(payload)
    expect(decoded.decode().startswith("gz-"), f"decoded body {decoded[:20]!r}")


def case_gzip_below_min_length(ctx: Context) -> None:
    response = http(ctx.main, "/gz/small.txt", headers=[("Accept-Encoding", "gzip")], host="filters.example.com")
    expect(response.status == 200, f"status {response.status}")
    expect(not response.has_header("Content-Encoding"), "small body must not be gzipped")


def case_gzip_identity(ctx: Context) -> None:
    response = http(ctx.main, "/gz/big.txt", headers=[("Accept-Encoding", "identity")], host="filters.example.com")
    expect(response.status == 200, f"status {response.status}")
    expect(not response.has_header("Content-Encoding"), "identity negotiation must not gzip")


def case_sub_filter(ctx: Context) -> None:
    response = http(ctx.main, "/sub/x", headers=[("Accept-Encoding", "identity")], host="filters.example.com")
    expect(response.status == 200, f"status {response.status}")
    expect(response.text() == "omega omega beta omega", f"body {response.text()!r}")


def case_limit_req_burst(ctx: Context) -> None:
    statuses = []
    for index in range(12):
        response = http(ctx.main, f"/rate/{index}", host="filters.example.com", timeout=4)
        statuses.append(response.status)
    passed = statuses.count(200)
    rejected = statuses.count(503)
    expect(
        passed + rejected == 12,
        f"unexpected statuses {sorted(set(statuses))}",
    )
    expect(4 <= passed <= 9, f"burst pass count {passed} ({statuses})")
    expect(rejected >= 2, f"burst rejection count {rejected} ({statuses})")


def case_limit_conn(ctx: Context) -> None:
    holder: dict[str, object] = {}

    def hold() -> None:
        holder["response"] = http(ctx.main, "/conn/one", host="filters.example.com", timeout=8)

    thread = threading.Thread(target=hold, daemon=True)
    thread.start()
    time.sleep(0.4)
    second = http(ctx.main, "/conn/two", host="filters.example.com", timeout=8)
    thread.join(timeout=8)
    held = holder.get("response")
    expect(second.status == 503, f"second concurrent status {second.status}")
    expect(isinstance(held, Response) and held.status == 200, f"held request failed: {held!r}")


def case_allow_deny_allowed(ctx: Context) -> None:
    response = http(ctx.main, "/admin/x", host="filters.example.com")
    expect(response.status == 200, f"status {response.status}")


def case_deny_all(ctx: Context) -> None:
    response = http(ctx.main, "/blocked/x", host="filters.example.com")
    expect(response.status == 403, f"status {response.status}")


def case_unmatched_rule_denies(ctx: Context) -> None:
    response = http(ctx.main, "/mixed/x", host="filters.example.com")
    expect(response.status == 403, f"status {response.status}")


def case_auth_basic(ctx: Context) -> None:
    challenge = http(ctx.main, "/private/x", host="filters.example.com")
    expect(challenge.status == 401, f"no-credential status {challenge.status}")
    www_auth = challenge.header("WWW-Authenticate") or ""
    expect("Basic" in www_auth and "Full Surface" in www_auth, f"WWW-Authenticate {www_auth!r}")
    wrong = http(
        ctx.main,
        "/private/x",
        headers=[("Authorization", "Basic " + base64.b64encode(b"alice:wrongpass").decode())],
        host="filters.example.com",
    )
    expect(wrong.status == 401, f"wrong-password status {wrong.status}")
    good = http(
        ctx.main,
        "/private/x",
        headers=[("Authorization", "Basic " + base64.b64encode(b"alice:secret123").decode())],
        host="filters.example.com",
    )
    expect(good.status == 200, f"valid-password status {good.status} body {good.text()!r}")


# ---- TLS / HTTP2 ----


def case_tls_sni_certificate_selection(ctx: Context) -> None:
    response_a, _, der_a = tls_get(ctx.tls, "a.example.local", "/")
    response_b, _, der_b = tls_get(ctx.tls, "b.example.local", "/")
    expect(response_a.status == 200 and response_a.text() == "tls-a", f"a: {response_a.status} {response_a.text()!r}")
    expect(response_b.status == 200 and response_b.text() == "tls-b", f"b: {response_b.status} {response_b.text()!r}")
    expect(der_a is not None and der_b is not None and der_a != der_b, "SNI did not select distinct certificates")


def case_http2_alpn_settings(ctx: Context) -> None:
    result = h2_settings_exchange(ctx.tls, "a.example.local")
    expect(result == "ok", f"h2 handshake: {result}")


# ---- stream ----


def case_stream_tcp_proxy(ctx: Context) -> None:
    with socket.create_connection(("127.0.0.1", ctx.stream), timeout=6) as sock:
        sock.settimeout(6)
        for round_index in range(3):
            message = f"ping-full-surface-{round_index}\n".encode()
            sock.sendall(message)
            echoed = b""
            while len(echoed) < len(message):
                chunk = sock.recv(4096)
                if not chunk:
                    raise EOFError("stream echo closed")
                echoed += chunk
            expect(echoed == message, f"round {round_index}: echoed {echoed!r}")


# ---- http wire ----


def case_keepalive_two_requests(ctx: Context) -> None:
    client = HttpClient(ctx.main)
    try:
        first = client.request("/exact")
        second = client.request("/exact")
        expect(first.status == 200 and first.text() == "exact", f"first {first.status}")
        expect(second.status == 200 and second.text() == "exact", f"second {second.status}")
    finally:
        client.close()


def case_connection_close(ctx: Context) -> None:
    with socket.create_connection(("127.0.0.1", ctx.main), timeout=5) as sock:
        sock.settimeout(5)
        sock.sendall(
            (
                "GET /exact HTTP/1.1\r\n"
                "Host: static.example.com\r\n"
                "Connection: close\r\n\r\n"
            ).encode("ascii")
        )
        response = read_one_response(sock, 5)
        expect(response.header("Connection", "").lower() == "close", f"Connection {response.header('Connection')!r}")
        try:
            trailing = sock.recv(4096)
        except (socket.timeout, OSError):
            trailing = b"sentinel"
        expect(trailing in (b"", b"sentinel") or trailing == b"", f"data after close response: {trailing!r}")


def case_pipelining(ctx: Context) -> None:
    with socket.create_connection(("127.0.0.1", ctx.main), timeout=6) as sock:
        sock.settimeout(6)
        payload = (
            "GET /exact HTTP/1.1\r\nHost: static.example.com\r\n\r\n"
            "GET /exact HTTP/1.1\r\nHost: static.example.com\r\n\r\n"
            "GET /exact HTTP/1.1\r\nHost: static.example.com\r\n\r\n"
        ).encode("ascii")
        sock.sendall(payload)
        for index in range(3):
            response = read_one_response(sock, 6)
            expect(response.status == 200, f"pipelined response {index} status {response.status}")
            expect(response.text() == "exact", f"pipelined response {index} body {response.text()!r}")




# ---- second alignment round: server-level contexts + error_page ----


def case_server_level_gzip_scoping(ctx: Context) -> None:
    # gzip is declared at the filters server level; the static host (no gzip
    # directives, app-level off) must serve uncompressed bodies.
    response = http(ctx.main, "/big.txt", headers=[("Accept-Encoding", "gzip")])
    expect(response.status == 200, f"status {response.status}")
    expect(not response.has_header("Content-Encoding"), "static host must not gzip without server-level gzip")


def case_server_level_rewrite(ctx: Context) -> None:
    # Server-level `rewrite ^/srv/(.*)$ /$1 last;` runs before locations.
    response = http(ctx.main, "/srv/small.txt")
    expect(response.status == 200, f"status {response.status}")
    expect(body_text(response) == "tiny\n", f"body {response.text()!r}")


def case_error_page_default_status(ctx: Context) -> None:
    # error_page 404 /notfound.html keeps the original 404 status.
    response = http(ctx.main, "/no-such-file.txt")
    expect(response.status == 404, f"status {response.status}")
    expect("full-surface-notfound" in response.text(), f"body {response.text()!r}")


def case_error_page_response_code_override(ctx: Context) -> None:
    # error_page 416 =200 /notfound.html replaces the response status.
    response = http(ctx.main, "/big.txt", headers=[("Range", "bytes=999999999-")])
    expect(response.status == 200, f"status {response.status}")
    expect("full-surface-notfound" in response.text(), f"body {response.text()!r}")


def case_server_level_sub_filter(ctx: Context) -> None:
    # /sub-srv/ has no location sub_filter: the server-level beta->delta
    # mapping applies. /sub/ declares its own family, so the server mapping
    # must NOT leak into it (replace-on-declare).
    inherited = http(ctx.main, "/sub-srv/x", host="filters.example.com")
    expect(inherited.status == 200, f"status {inherited.status}")
    expect(inherited.text() == "alpha alpha delta alpha", f"body {inherited.text()!r}")
    own = http(ctx.main, "/sub/x", host="filters.example.com")
    expect(own.text() == "omega omega beta omega", f"body {own.text()!r}")


def case_location_gzip_off(ctx: Context) -> None:
    # Location-level `gzip off` overrides the host policy for this route;
    # the host-level gzip stays on for /gz/.
    plain = http(ctx.main, "/gzplain/big.txt", headers=[("Accept-Encoding", "gzip")], host="filters.example.com")
    expect(plain.status == 200, f"status {plain.status}")
    expect(not plain.has_header("Content-Encoding"), f"route gzip off leaked: {plain.header('Content-Encoding')!r}")
    compressed = http(ctx.main, "/gz/big.txt", headers=[("Accept-Encoding", "gzip")], host="filters.example.com")
    expect(compressed.header("Content-Encoding") == "gzip", f"host gzip broke: {compressed.header('Content-Encoding')!r}")


def case_route_level_error_page(ctx: Context) -> None:
    # /lost/ maps 404 to /alt.html (route-level set replaces the host set
    # that maps 404 to /notfound.html).
    response = http(ctx.main, "/lost/nope")
    expect(response.status == 404, f"status {response.status}")
    expect("full-surface-alt" in response.text(), f"body {response.text()!r}")


def case_proxy_intercept_errors(ctx: Context) -> None:
    # With proxy_intercept_errors on, the mapped upstream 503 is replaced by
    # the error page (status kept, no =code override).
    intercepted = http(ctx.main, "/perr/x", host="proxy.example.com")
    expect(intercepted.status == 503, f"status {intercepted.status}")
    expect("full-surface-down" in intercepted.text(), f"body {intercepted.text()!r}")
    # Control: without the flag the upstream error passes through verbatim.
    passed = http(ctx.main, "/perr-pass/x", host="proxy.example.com")
    expect(passed.status == 503, f"status {passed.status}")
    expect(passed.text() == "induced", f"body {passed.text()!r}")


def case_recursive_error_pages(ctx: Context) -> None:
    # /chain/ maps 404 to a MISSING target (/missing-1.html); the 404 from
    # that hop re-enters mapping with the host-level 404 page. Non-recursive
    # engines would surface the raw "not found" error body instead.
    response = http(ctx.main, "/chain/nope")
    expect(response.status == 404, f"status {response.status}")
    expect("full-surface-notfound" in response.text(), f"body {response.text()!r}")


CASES: list[tuple[str, object]] = [
    ("routing.default-server-unknown-host", case_default_server_unknown_host),
    ("routing.http1-0-without-host", case_http10_without_host),
    ("routing.exact-beats-prefix", case_exact_location_beats_prefix),
    ("routing.uri-normalization-matrix", case_uri_normalization_matrix),
    ("routing.wildcard-vhost", case_wildcard_vhost),
    ("routing.regex-locations", case_regex_locations),
    ("routing.caret-tilde-suppresses-regex", case_caret_tilde_suppresses_regex),
    ("routing.rewrite-last-captures", case_rewrite_last_with_captures),
    ("routing.rewrite-permanent-redirect", case_rewrite_permanent_and_redirect),
    ("routing.rewrite-break-static", case_rewrite_break_serves_static),
    ("routing.try-files-spa-fallback", case_try_files_spa_fallback),
    ("routing.alias-substitution", case_alias_substitution),
    ("routing.index-directive", case_index_directive),
    ("static.404", case_static_404),
    ("static.head", case_static_head),
    ("static.range", case_static_range),
    ("static.conditional-304", case_static_conditional),
    ("static.mime-types", case_static_mime_types),
    ("static.add-header", case_add_header_present),
    ("static.secure-link-secret", case_secure_link_secret),
    ("static.expires-relative", case_expires_relative),
    ("static.expires-absolute-modes", case_expires_absolute_modes),
    ("static.expires-negative", case_expires_negative),
    ("static.expires-daily", case_expires_daily),
    ("static.expires-modified", case_expires_modified),
    ("static.expires-zero-shortcut", case_expires_zero_shortcut),
    ("static.etag-off", case_etag_off_suppresses_generation),
    ("static.if-modified-since-off", case_if_modified_since_off_ignores_the_condition),
    ("static.if-modified-since-exact-default", case_if_modified_since_exact_default),
    ("proxy.expires-applied", case_proxy_response_carries_expires),
    ("proxy.path-query-preserved", case_proxy_path_and_query_preserved),
    ("proxy.encoded-slash-preserved", case_proxy_encoded_slash_preserved),
    ("proxy.pass-uri-replacement", case_proxy_pass_uri_replacement),
    ("proxy.method-body-forwarded", case_proxy_forwards_method_and_body),
    ("proxy.default-forwarded-headers", case_proxy_default_forwarded_headers),
    ("proxy.set-header-server-level", case_proxy_set_header_server_level),
    ("proxy.set-header-child-replaces", case_proxy_set_header_child_replaces),
    ("proxy.pass-request-headers-off", case_proxy_pass_request_headers_off),
    ("proxy.hop-by-hop-stripped", case_proxy_hop_by_hop_stripped),
    ("proxy.variable-proxy-pass", case_variable_proxy_pass),
    ("proxy.body-limit-413", case_request_body_limit_413),
    ("proxy.chunked-request-body", case_chunked_request_body),
    ("proxy.expect-100-continue", case_expect_100_continue),
    ("proxy.cache-hit", case_proxy_cache),
    ("proxy.chunked-response-relay", case_proxy_streaming_incremental),
    ("proxy.websocket-tunnel", case_websocket_tunnel),
    ("lb.weighted-round-robin", case_weighted_round_robin),
    ("lb.least-conn-avoids-busy", case_least_conn_avoids_busy_target),
    ("lb.ip-hash-affinity", case_ip_hash_affinity),
    ("lb.hash-consistent-affinity", case_hash_consistent_affinity),
    ("lb.backup-failover", case_backup_failover),
    ("filters.gzip-large-text", case_gzip_large_text),
    ("filters.gzip-below-min-length", case_gzip_below_min_length),
    ("filters.gzip-identity", case_gzip_identity),
    ("filters.sub-filter", case_sub_filter),
    ("filters.limit-req-burst", case_limit_req_burst),
    ("filters.limit-conn-503", case_limit_conn),
    ("filters.allow-deny-allowed", case_allow_deny_allowed),
    ("filters.deny-all", case_deny_all),
    ("filters.unmatched-rule-denies", case_unmatched_rule_denies),
    ("filters.auth-basic", case_auth_basic),
    ("tls.sni-certificate-selection", case_tls_sni_certificate_selection),
    ("tls.http2-alpn-settings", case_http2_alpn_settings),
    ("stream.tcp-proxy", case_stream_tcp_proxy),
    ("wire.keepalive-two-requests", case_keepalive_two_requests),
    ("wire.connection-close", case_connection_close),
    ("wire.pipelining", case_pipelining),
    ("routing.server-level-gzip-scoping", case_server_level_gzip_scoping),
    ("routing.server-level-rewrite", case_server_level_rewrite),
    ("error-page.default-status", case_error_page_default_status),
    ("error-page.response-code-override", case_error_page_response_code_override),
    ("error-page.route-level-override", case_route_level_error_page),
    ("error-page.recursive-chain", case_recursive_error_pages),
    ("proxy.intercept-errors", case_proxy_intercept_errors),
    ("filters.server-level-sub-filter", case_server_level_sub_filter),
    ("filters.location-gzip-off", case_location_gzip_off),
]


# --------------------------------------------------------------------------
# Server lifecycle
# --------------------------------------------------------------------------


def find_server_bin(explicit: str | None) -> tuple[Path, bool] | None:
    """Return (binary, needs_serve_nginx_flag). The full gateway binary needs
    the `management` feature; the dedicated serve-nginx-compat bin is the
    no-management fallback with the identical data plane."""
    if explicit:
        candidate = Path(explicit)
        return (candidate, True) if candidate.is_file() else None
    names = (
        ("sdkwork-api-webserver-standalone-gateway", True),
        ("serve-nginx-compat", False),
    )
    for profile in ("debug", "release"):
        for name, needs_flag in names:
            for suffix in (".exe", ""):
                candidate = REPO_ROOT / "target" / profile / f"{name}{suffix}"
                if candidate.is_file():
                    return (candidate, needs_flag)
    return None


def posix_root(path: Path) -> str:
    """Render a path the nginx-compat root validator accepts (POSIX leading /).

    Windows drive paths are reduced to their drive-relative absolute form
    (leading /): the server process resolves them against its working drive,
    which the probe keeps on the repository drive. On POSIX hosts the path is
    already absolute and passes through unchanged.
    """
    rendered = str(path.resolve()).replace("\\", "/")
    if re.match(r"^[A-Za-z]:/", rendered):
        rendered = rendered[2:]
    return rendered


def render_conf() -> Path:
    """Render nginx.conf placeholders into nginx.rendered.conf (staging step)."""
    substitutions = {
        "@PUBLIC@": posix_root(FIXTURE_DIR / "public"),
        "@CACHE@": posix_root(FIXTURE_DIR / "cache"),
        "@HTPASSWD@": posix_root(FIXTURE_DIR / "htpasswd"),
    }
    text = (FIXTURE_DIR / "nginx.conf").read_text(encoding="utf-8")
    for placeholder, value in substitutions.items():
        text = text.replace(placeholder, value)
    rendered = FIXTURE_DIR / "nginx.rendered.conf"
    rendered.write_text(text, encoding="utf-8")
    return rendered


def spawn_server(binary: Path, needs_flag: bool, conf: Path) -> subprocess.Popen:
    SERVER_LOG.parent.mkdir(parents=True, exist_ok=True)
    log_handle = SERVER_LOG.open("wb")
    command = [str(binary), *(["serve-nginx"] if needs_flag else []), str(conf)]
    return subprocess.Popen(
        command,
        cwd=str(REPO_ROOT),
        stdout=log_handle,
        stderr=subprocess.STDOUT,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port-base", type=int, default=19890)
    parser.add_argument("--server-bin", default=None, help="gateway binary; auto-detected under target/ when omitted")
    parser.add_argument("--skip-server-spawn", action="store_true", help="attach to an already-running fixture server")
    parser.add_argument("--case", default=None, help="only run cases whose id contains this substring")
    args = parser.parse_args()

    ports = {
        "main": args.port_base,
        "echo": args.port_base + 1,
        "lb-a": args.port_base + 2,
        "lb-b": args.port_base + 3,
        "lb-backup": args.port_base + 4,
        "lb": args.port_base + 5,
        "stream": args.port_base + 6,
        "stream-echo": args.port_base + 7,
        "tls": args.port_base + 8,
    }
    # lb_backup's dead primary sits on port_base + 9 (never bound).

    ctx = Context(ports)

    server: subprocess.Popen | None = None
    server_info = find_server_bin(args.server_bin)
    if not args.skip_server_spawn:
        if server_info is None:
            print("FAIL: gateway binary not found; build with cargo build -p sdkwork-api-webserver-standalone-gateway", file=sys.stderr)
            return 2
        write_htpasswd(FIXTURE_DIR / "htpasswd", "alice", "secret123")
        generate_cert("a.example.local", FIXTURE_DIR / "cert-a.pem", FIXTURE_DIR / "key-a.pem")
        generate_cert("b.example.local", FIXTURE_DIR / "cert-b.pem", FIXTURE_DIR / "key-b.pem")
        # The proxy-cache disk directory is probe-owned runtime state; a stale
        # entry from a previous run would serve /cached/* without an upstream
        # request and break the cache-hit accounting.
        cache_dir = FIXTURE_DIR / "cache"
        if cache_dir.is_dir():
            shutil.rmtree(cache_dir)
    backends = [
        MockHttpBackend("echo", ports["echo"]),
        MockHttpBackend("target-a", ports["lb-a"]),
        MockHttpBackend("target-b", ports["lb-b"]),
        MockHttpBackend("target-backup", ports["lb-backup"]),
        StreamEcho(ports["stream-echo"]),
    ]
    for backend in backends:
        backend.start()
    ctx.echo = backends[0]
    ctx.target_a = backends[1]
    ctx.target_b = backends[2]
    ctx.target_backup = backends[3]

    try:
        if not args.skip_server_spawn:
            server = spawn_server(server_info[0], server_info[1], render_conf())
        wait_static_root_ready(ports["main"])
    except Exception as error:  # noqa: BLE001 - probe boundary reports any startup failure
        print(f"FAIL: server startup: {error}", file=sys.stderr)
        if server is not None:
            server.terminate()
        return 2

    failures: list[tuple[str, str]] = []
    selected = [case for case in CASES if args.case is None or args.case in case[0]]
    for name, case_fn in selected:
        started = time.monotonic()
        try:
            case_fn(ctx)
            print(f"PASS {name} ({time.monotonic() - started:.2f}s)")
        except Exception as error:  # noqa: BLE001 - the report is the deliverable
            failures.append((name, f"{type(error).__name__}: {error}"))
            print(f"FAIL {name} ({time.monotonic() - started:.2f}s) -> {error}")
            if os.environ.get("PROBE_TRACEBACK"):
                traceback.print_exc()

    print()
    print("=" * 72)
    print(f"FULL-SURFACE REGRESSION: {len(selected) - len(failures)}/{len(selected)} passed")
    if failures:
        print("-" * 72)
        for name, message in failures:
            print(f"FAILED {name}")
            print(f"       {message}")
    if server is not None and failures:
        print("-" * 72)
        print("server log tail (logs/server.log):")
        try:
            tail = SERVER_LOG.read_text("utf-8", "replace").splitlines()[-30:]
            print("\n".join(tail))
        except OSError:
            pass
    print("=" * 72)

    if server is not None:
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
