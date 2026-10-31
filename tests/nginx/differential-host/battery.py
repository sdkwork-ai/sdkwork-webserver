"""Canonical host-selection battery (PRD-nginx-compatibility §5/§6).

One script, two endpoints: run it against real nginx and against the Rust data
plane, then diff the two JSON documents. Only the observable selection contract
is recorded — the HTTP status and the response body of the selected virtual
host / named location — so a difference is always a host-selection or
internal-redirect difference and never a header-formatting artefact.

    python3 battery.py --port 21010 --out nginx-oracle.json    # oracle
    python3 battery.py --port 21011 --out ours.json            # Rust data plane

The battery owns a deterministic mock upstream (``--mock-port``) used by the
`try_files … @named-proxy` case; both runs start the identical mock on the same
port, so a proxied body is comparable byte for byte.

`Date`, `Server`, and every other header are recorded only for identity
(`_server`) and are never compared: the two endpoints are separate processes
measured seconds apart, and the comparison contract of this slice is selection,
not transport metadata. compare.py documents every intentional difference.

`_`-prefixed keys carry identity metadata so a comparison can prove which
implementation answered on each port.
"""

from __future__ import annotations

import argparse
import json
import socket
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HTTP_10 = "HTTP/1.0"
HTTP_11 = "HTTP/1.1"


class MockUpstream(BaseHTTPRequestHandler):
    """Deterministic proxy target: echoes the received request path."""

    protocol_version = "HTTP/1.1"

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        body = ("mock %s\n" % self.path).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args) -> None:  # noqa: D401 - silence test noise
        pass


def start_mock(port: int) -> ThreadingHTTPServer:
    ThreadingHTTPServer.allow_reuse_address = True
    server = ThreadingHTTPServer(("127.0.0.1", port), MockUpstream)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def request(port: int, target: str, host, version: str = HTTP_11):
    lines = ["GET %s %s" % (target, version)]
    if host is not None:
        lines.append("Host: %s" % host)
    lines.append("Connection: close")
    payload = ("\r\n".join(lines) + "\r\n\r\n").encode("latin-1")
    with socket.create_connection(("127.0.0.1", port), timeout=10) as sock:
        sock.settimeout(10)
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
    parsed = {}
    for line in head_lines[1:]:
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        parsed.setdefault(name.strip().lower(), value.strip())
    return status, parsed, body.decode("latin-1")


# (label, request target, Host authority (None = no Host header), HTTP version)
CASES = [
    ("host.exact", "/", "exact.example.com", HTTP_11),
    ("host.exact-uppercase", "/", "EXACT.EXAMPLE.COM", HTTP_11),
    ("host.exact-port", "/", "exact.example.com:8443", HTTP_11),
    ("host.exact-trailing-dot", "/", "exact.example.com.", HTTP_11),
    ("host.exact-beats-leading-wildcard", "/", "exact.example.com", HTTP_11),
    ("host.leading-wildcard", "/", "a.lead.example.com", HTTP_11),
    ("host.leading-wildcard-any-depth", "/", "a.b.lead.example.com", HTTP_11),
    ("host.leading-wildcard-longest", "/", "x.deep.lead.example.com", HTTP_11),
    ("host.leading-wildcard-bare-suffix", "/", "lead.example.com", HTTP_11),
    ("host.trailing-wildcard", "/", "www.trail.example.com", HTTP_11),
    ("host.trailing-wildcard-any-suffix", "/", "www.trail.example.co.uk", HTTP_11),
    ("host.trailing-wildcard-longest", "/", "www.trail.example.com", HTTP_11),
    ("host.trailing-wildcard-prefix-miss", "/", "xwww.trail.example.com", HTTP_11),
    ("host.trailing-wildcard-bare-prefix", "/", "trail.example.com", HTTP_11),
    ("host.dot-prefix-bare", "/", "dot.example", HTTP_11),
    ("host.dot-prefix-subdomain", "/", "a.dot.example", HTTP_11),
    ("host.dot-prefix-deep", "/", "a.b.c.dot.example", HTTP_11),
    ("host.dot-prefix-miss", "/", "xdot.example", HTTP_11),
    ("host.exact-beats-trailing-wildcard", "/", "exact.example.com", HTTP_11),
    ("host.regex-first-authored", "/", "order.example.com", HTTP_11),
    ("host.regex-second-not-reached", "/", "order.other", HTTP_11),
    ("host.regex-uppercase-caseless", "/", "upper.case.example", HTTP_11),
    ("host.leading-wildcard-beats-regex", "/", "a.mix.example", HTTP_11),
    ("host.trailing-wildcard-beats-regex", "/", "trail.mix.other", HTTP_11),
    ("host.default-unknown", "/", "nobody.invalid", HTTP_11),
    ("host.default-no-host-http10", "/", None, HTTP_10),
    ("named.static-hit", "/static/file.txt", "named.example.com", HTTP_11),
    ("named.static-miss-to-named-return", "/static/missing", "named.example.com", HTTP_11),
    ("named.try-files-hit", "/nf/file.txt", "named.example.com", HTTP_11),
    ("named.try-files-miss-to-named-return", "/nf/missing", "named.example.com", HTTP_11),
    ("named.try-files-proxy", "/proxy/echo?x=1", "named.example.com", HTTP_11),
    ("named.error-page-default-status", "/err-default/missing", "named.example.com", HTTP_11),
    ("named.error-page-static-body", "/custom/missing", "named.example.com", HTTP_11),
    ("named.access-allow", "/gate/ok", "named.example.com", HTTP_11),
    ("named.return-skips-access", "/deny/ok", "named.example.com", HTTP_11),
    ("named.static-target-access-allow", "/sallow/missing", "named.example.com", HTTP_11),
    ("named.static-target-access-deny", "/sdeny/ok", "named.example.com", HTTP_11),
    ("named.redirect", "/redir/x", "named.example.com", HTTP_11),
    ("static.builtin-404", "/plain/missing", "plain.example.com", HTTP_11),
]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--mock-port", type=int, default=21012)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    try:
        mock = start_mock(args.mock_port)
    except OSError as error:
        print(
            "battery: cannot bind the mock upstream on 127.0.0.1:%d: %s"
            % (args.mock_port, error),
            file=sys.stderr,
        )
        return 3

    results = {}
    identity = None
    try:
        for label, target, host, version in CASES:
            status, headers, body = request(args.port, target, host, version)
            results[label] = {"status": status, "body": body}
            if identity is None:
                identity = headers.get("server")
        # Identity witness: every comparison must prove which implementation
        # answered. WSL localhost forwarding can silently route a Windows
        # connection into a different listener.
        if identity and identity.lower().startswith("nginx/"):
            results["_server"] = identity
        elif identity is None:
            results["_server"] = None
        else:
            results["_server"] = identity
        results["_port"] = args.port
        results["_cases"] = len(CASES)
        with open(args.out, "w", encoding="utf-8") as handle:
            json.dump(results, handle, indent=1, sort_keys=True)
            handle.write("\n")
    finally:
        mock.shutdown()
        mock.server_close()
    print("battery: captured %d host-selection cases on port %d" % (len(CASES), args.port))
    return 0


if __name__ == "__main__":
    sys.exit(main())
