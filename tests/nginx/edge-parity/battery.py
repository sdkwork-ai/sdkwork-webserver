#!/usr/bin/env python3
"""Capture the SDKWork development-edge battery from a live edge endpoint.

The api-dev host runs a development edge that serves three virtual-host
families on host ports 80/443:

- `api-dev.<brand>` — reverse proxy to the development gateway, with
  `/metrics` and `/livez` blocked, unknown Host connections dropped;
- `mall-dev.<brand>` — adaptive (PC/H5) static delivery with SPA fallback,
  `runtime-env.json` no-store, immutable fingerprinted assets and the
  development-only credential-entry bootstrap injection;
- `platform-dev.<brand>` — the same adaptive static contract for the
  cloudrouter development preview.

This battery records, per case, the status, the ordered response header map
(minus hop-by-hop and time-varying fields), the body sha256 and the body
facts that carry contract meaning: which adaptive surface answered and which
application identity the bootstrap token claims. It intentionally records
reality instead of asserting expectations — `compare.py` turns two captures
into a verdict, so the same battery can capture a stock-nginx oracle before
the edge cutover and the runtime capture after it.

Usage:
    python3 battery.py --http 127.0.0.1:80 --tls 127.0.0.1:443 \
        --out capture.json [--label stock-nginx-edge]

Exit codes: 0 captured, 2 usage error, 3 target unreachable.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import socket
import ssl
import sys
import time
from urllib.parse import urlsplit

# The battery must never shrink silently: compare.py rejects captures below
# this floor (run-nginx-differential.mjs provenance discipline).
CASE_FLOOR = 24

HOP_BY_HOP = {
    "connection",
    "keep-alive",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
}
TIME_VARYING = {"date"}

DESKTOP_UA = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/120.0.0.0 Safari/537.36"
)
IPHONE_UA = (
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) "
    "AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
)
IPAD_UA = (
    "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) "
    "AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
)

BOOTSTRAP_TOKEN_RE = re.compile(
    r'__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__\s*=\s*"([^"]+)"'
)
# The full injected handoff: nginx's sub_filter and the runtime injector use
# slightly different script wrappers around the same canonical global, so the
# body hash normalizes the whole injected element while the token value inside
# it is still extracted for the application-identity assertion.
BOOTSTRAP_INJECTION_RE = re.compile(
    r'<script>\s*globalThis\.__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__'
    r'\s*=\s*"[^"]*";?\s*</script>'
)
NGINX_ERROR_PAGE_RE = re.compile(
    r"<title>\d{3} [^<]*</title>.*<center>nginx</center>", re.DOTALL
)
# Request-scoped trace identifiers: the gateway's ProblemDetail body embeds
# the per-request trace id (dash-separated UUID or bare 32-hex), so the body
# hash normalizes both forms.
TRACE_ID_RE = re.compile(
    r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b|\b[0-9a-f]{32}\b"
)
ASSET_PATH_RE = re.compile(r'(?:src|href)="(/assets/[^"]+)"')

API_DEV_HOSTS = ["api-dev.birdcoder.com", "api-dev.sdkwork.com"]
MALL_HOST = "mall-dev.birdcoder.com"
PLATFORM_HOST = "platform-dev.birdcoder.com"
UNKNOWN_HOST = "unknown.example.com"


def decode_bootstrap_app_id(token: str) -> str | None:
    """Return the JWT `app_id` claim of a bootstrap token, or None."""
    parts = token.split(".")
    if len(parts) < 2:
        return None
    padded = parts[1] + "=" * (-len(parts[1]) % 4)
    try:
        claims = json.loads(base64.urlsafe_b64decode(padded))
    except (ValueError, json.JSONDecodeError):
        return None
    app_id = claims.get("app_id")
    return app_id if isinstance(app_id, str) else None


class CapturedResponse:
    """One recorded HTTP exchange or connection-level outcome."""

    def __init__(self) -> None:
        self.dropped = False
        self.drop_phase: str | None = None  # connect | request | response
        self.status: int | None = None
        self.headers: dict[str, str] = {}
        self.body_sha256: str | None = None
        self.body_bytes: int | None = None
        self.body_prefix: str | None = None
        self.nginx_error_page = False
        self.surface: str | None = None
        self.bootstrap_app_id: str | None = None
        self.tls: dict[str, object] | None = None

    def to_json(self) -> dict[str, object]:
        out: dict[str, object] = {"dropped": self.dropped}
        if self.drop_phase:
            out["dropPhase"] = self.drop_phase
        if self.status is not None:
            out["status"] = self.status
        if self.headers:
            out["headers"] = self.headers
        if self.body_sha256:
            out["bodySha256"] = self.body_sha256
        if self.body_bytes is not None:
            out["bodyBytes"] = self.body_bytes
        if self.body_prefix is not None:
            out["bodyPrefix"] = self.body_prefix
        if self.nginx_error_page:
            out["nginxErrorPage"] = True
        if self.surface:
            out["surface"] = self.surface
        if self.bootstrap_app_id:
            out["bootstrapAppId"] = self.bootstrap_app_id
        if self.tls is not None:
            out["tls"] = self.tls
        return out


def classify_body(body: bytes) -> tuple[str | None, str | None]:
    """Extract the adaptive surface and bootstrap app identity from a body."""
    text = body.decode("utf-8", errors="replace")
    surface = None
    bootstrap_app_id = None
    token_match = BOOTSTRAP_TOKEN_RE.search(text)
    if token_match:
        bootstrap_app_id = decode_bootstrap_app_id(token_match.group(1))
    # Surface detection: the delivered index.html references its own bundle
    # layout; instead of guessing from content, ask the Vary-bearing headers
    # is unreliable across implementations, so the battery marks the surface
    # by which marker the served HTML carries. Adaptive bundles differ, so the
    # sha256 comparison already catches a wrong-surface answer; the marker is
    # a diagnostic aid only.
    lowered = text.lower()
    if "/h5-assets/" in lowered or "h5" in (bootstrap_app_id or ""):
        surface = "h5"
    elif bootstrap_app_id:
        surface = "pc"
    return surface, bootstrap_app_id


def decode_chunked(raw: bytes) -> bytes:
    """Decode a chunked transfer-encoded payload (RFC 9112 §7.1)."""
    out = bytearray()
    pos = 0
    while pos < len(raw):
        line_end = raw.find(b"\r\n", pos)
        if line_end < 0:
            break
        try:
            size = int(raw[pos:line_end].split(b";")[0].strip() or b"0", 16)
        except ValueError:
            return bytes(raw)  # not actually chunked: keep as-is
        if size == 0:
            break
        out += raw[line_end + 2 : line_end + 2 + size]
        pos = line_end + 2 + size + 2
    return bytes(out)


def record_response(response: CapturedResponse, body: bytes) -> None:
    # The development credential-entry bootstrap token is injected into SPA
    # documents by whichever edge serves them; the runtime injects its
    # instance-scoped token (credential_entry_injection.rs single-policy) while
    # the stock-nginx edge injected per-surface tokens. The injected element is
    # a stated-contract difference asserted separately via `bootstrapAppId`, and
    # per-request gateway trace ids vary by construction, so the body hash is
    # computed over the normalized document: everything else must match byte
    # for byte.
    text = body.decode("utf-8", errors="replace")
    normalized = BOOTSTRAP_INJECTION_RE.sub("__SDKWORK_BOOTSTRAP_HANDOFF__", text)
    normalized = TRACE_ID_RE.sub("__TRACE_ID__", normalized)
    response.body_sha256 = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
    response.body_bytes = len(body)
    response.body_prefix = body[:240].decode("utf-8", errors="replace")
    response.nginx_error_page = bool(NGINX_ERROR_PAGE_RE.search(text))
    response.surface, response.bootstrap_app_id = classify_body(body)


def http_capture(
    target_host: str,
    target_port: int,
    host_header: str,
    path: str,
    extra_headers: dict[str, str] | None = None,
    method: str = "GET",
    send_host_header: bool = True,
) -> CapturedResponse:
    """One raw HTTP/1.1 request; connection-level failures become drops."""
    captured = CapturedResponse()
    request_lines = [f"{method} {path} HTTP/1.1"]
    if send_host_header:
        request_lines.append(f"Host: {host_header}")
    for key, value in (extra_headers or {}).items():
        request_lines.append(f"{key}: {value}")
    request_lines += [
        "User-Agent: sdkwork-edge-parity-battery/1",
        "Accept: */*",
        # Keep the exchange one-shot: keep-alive servers would otherwise hold
        # the socket open and every case would wait out the recv timeout.
        "Connection: close",
    ]
    payload = ("\r\n".join(request_lines) + "\r\n\r\n").encode("utf-8")

    try:
        sock = socket.create_connection((target_host, target_port), timeout=10)
    except OSError as exc:
        captured.dropped = True
        captured.drop_phase = f"connect:{type(exc).__name__}"
        return captured
    try:
        sock.sendall(payload)
        chunks = []
        try:
            while True:
                chunk = sock.recv(65536)
                if not chunk:
                    break
                chunks.append(chunk)
                if sum(len(c) for c in chunks) > 4_000_000:
                    break
        except (ConnectionResetError, ConnectionAbortedError, socket.timeout):
            pass
        raw = b"".join(chunks)
        if not raw:
            captured.dropped = True
            captured.drop_phase = "response"
            return captured
        head, _, body = raw.partition(b"\r\n\r\n")
        lines = head.decode("latin-1").split("\r\n")
        if not lines or not lines[0].startswith("HTTP/"):
            captured.dropped = True
            captured.drop_phase = "response"
            return captured
        captured.status = int(lines[0].split()[1])
        ordered: list[tuple[str, str]] = []
        for line in lines[1:]:
            if ":" in line:
                name, _, value = line.partition(":")
                ordered.append((name.strip().lower(), value.strip()))
        captured.headers = {
            name: value
            for name, value in ordered
            if name not in HOP_BY_HOP and name not in TIME_VARYING
        }
        if any(name == "transfer-encoding" and "chunked" in value for name, value in ordered):
            body = decode_chunked(body)
        record_response(captured, body)
    finally:
        sock.close()
    return captured


def tls_capture(
    target_host: str,
    target_port: int,
    sni: str | None,
    path: str = "/",
    host_header: str | None = None,
) -> CapturedResponse:
    """TLS handshake plus one HTTP/1.1 round trip over it."""
    captured = CapturedResponse()
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    # The battery compares HTTP/1.1 semantics. Offering h2 makes an
    # http2-enabled oracle switch protocols after the handshake and drop the
    # plain HTTP/1.1 exchange, so the ALPN offer is http/1.1-only; the
    # negotiated protocol is still recorded per case.
    context.set_alpn_protocols(["http/1.1"])
    try:
        raw_sock = socket.create_connection((target_host, target_port), timeout=10)
    except OSError as exc:
        captured.dropped = True
        captured.drop_phase = f"connect:{type(exc).__name__}"
        return captured
    try:
        try:
            server_hostname = sni if sni and not _is_ip(sni) else None
            sock = context.wrap_socket(
                raw_sock, server_hostname=server_hostname, do_handshake_on_connect=True
            )
        except (ssl.SSLError, OSError, ValueError) as exc:
            captured.dropped = True
            captured.drop_phase = f"handshake:{type(exc).__name__}"
            return captured
        captured.tls = {
            "protocol": sock.version(),
            "cipher": sock.cipher()[0] if sock.cipher() else None,
            "alpn": sock.selected_alpn_protocol(),
            "sni": sni,
            "certSubjectSha256": _cert_fingerprint(sock),
            "certSanSha256": _cert_san_fingerprint(sock),
        }
        return http_over_socket(captured, sock, host_header or sni or "", path)
    finally:
        try:
            raw_sock.close()
        except OSError:
            pass


def _is_ip(value: str) -> bool:
    return re.fullmatch(r"[0-9.]+", value) is not None


def _cert_fingerprint(sock: ssl.SSLSocket) -> str | None:
    cert = sock.getpeercert(binary_form=True)
    return hashlib.sha256(cert).hexdigest() if cert else None


def _cert_san_fingerprint(sock: ssl.SSLSocket) -> str | None:
    """Fingerprint the SAN list only, so CN changes do not shadow it."""
    try:
        from cryptography import x509  # type: ignore[import-not-found]

        cert_der = sock.getpeercert(binary_form=True)
        if not cert_der:
            return None
        cert = x509.load_der_x509_certificate(cert_der)
        sans: list[str] = []
        try:
            ext = cert.extensions.get_extension_for_class(x509.SubjectAlternativeName)
            sans = sorted(ext.value.get_values_for_type(x509.DNSName))
        except x509.ExtensionNotFound:
            pass
        return hashlib.sha256(",".join(sans).encode("utf-8")).hexdigest()
    except ImportError:
        # cryptography unavailable: fall back to the whole-certificate
        # fingerprint, which still detects cert swaps (a CN-only change also
        # changes it, which is acceptable for a dev-edge oracle).
        return _cert_fingerprint(sock)


def http_over_socket(
    captured: CapturedResponse, sock: ssl.SSLSocket, host_header: str, path: str
) -> CapturedResponse:
    payload = (
        f"GET {path} HTTP/1.1\r\nHost: {host_header}\r\n"
        f"User-Agent: sdkwork-edge-parity-battery/1\r\nAccept: */*\r\n"
        f"Connection: close\r\n\r\n"
    ).encode("utf-8")
    sock.sendall(payload)
    chunks = []
    try:
        while True:
            chunk = sock.recv(65536)
            if not chunk:
                break
            chunks.append(chunk)
    except (ConnectionResetError, ConnectionAbortedError, ssl.SSLError, socket.timeout):
        pass
    raw = b"".join(chunks)
    if not raw:
        captured.dropped = True
        captured.drop_phase = "response"
        return captured
    head, _, body = raw.partition(b"\r\n\r\n")
    lines = head.decode("latin-1").split("\r\n")
    if not lines or not lines[0].startswith("HTTP/"):
        captured.dropped = True
        captured.drop_phase = "response"
        return captured
    captured.status = int(lines[0].split()[1])
    ordered: list[tuple[str, str]] = []
    for line in lines[1:]:
        if ":" in line:
            name, _, value = line.partition(":")
            ordered.append((name.strip().lower(), value.strip()))
    captured.headers = {
        name: value
        for name, value in ordered
        if name not in HOP_BY_HOP and name not in TIME_VARYING
    }
    if any(name == "transfer-encoding" and "chunked" in value for name, value in ordered):
        body = decode_chunked(body)
    record_response(captured, body)
    return captured


def first_asset_path(index_body: bytes) -> str | None:
    match = ASSET_PATH_RE.search(index_body.decode("utf-8", errors="replace"))
    return match.group(1) if match else None


def build_cases(http: tuple[str, int], tls: tuple[str, int]) -> list[dict[str, object]]:
    """The ordered battery. Dynamic asset paths are resolved inside capture()."""
    cases: list[dict[str, object]] = []

    def add(case_id: str, **kwargs: object) -> None:
        cases.append({"id": case_id, **kwargs})

    # -- api-dev proxy family ------------------------------------------------
    add("api-dev-birdcoder-http-root", kind="http", host=API_DEV_HOSTS[0], path="/")
    add("api-dev-birdcoder-http-metrics", kind="http", host=API_DEV_HOSTS[0], path="/metrics")
    add("api-dev-birdcoder-http-livez", kind="http", host=API_DEV_HOSTS[0], path="/livez")
    add("api-dev-sdkwork-http-root", kind="http", host=API_DEV_HOSTS[1], path="/")
    add("unknown-host-http-drop", kind="http", host=UNKNOWN_HOST, path="/")
    add("no-host-header-http-drop", kind="http", host="", path="/", send_host_header=False)

    # -- mall adaptive static family ----------------------------------------
    add("mall-desktop-index", kind="http", host=MALL_HOST, path="/", ua=DESKTOP_UA)
    add("mall-mobile-ua-index", kind="http", host=MALL_HOST, path="/", ua=IPHONE_UA)
    add(
        "mall-client-hint-h5",
        kind="http",
        host=MALL_HOST,
        path="/",
        ua=DESKTOP_UA,
        extra_headers={"Sec-CH-UA-Mobile": "?1"},
    )
    add("mall-ipad-stays-pc", kind="http", host=MALL_HOST, path="/", ua=IPAD_UA)
    add("mall-runtime-env-json", kind="http", host=MALL_HOST, path="/runtime-env.json")
    add("mall-assets-dir", kind="http", host=MALL_HOST, path="/assets/")
    add("mall-missing-asset", kind="http", host=MALL_HOST, path="/assets/definitely-missing.js")
    add("mall-spa-fallback", kind="http", host=MALL_HOST, path="/some/random/path", ua=DESKTOP_UA)
    add("mall-head-index", kind="http", host=MALL_HOST, path="/", ua=DESKTOP_UA, method="HEAD")

    # -- platform adaptive static family ------------------------------------
    add("platform-desktop-index", kind="http", host=PLATFORM_HOST, path="/", ua=DESKTOP_UA)
    add("platform-mobile-ua-index", kind="http", host=PLATFORM_HOST, path="/", ua=IPHONE_UA)
    add("platform-runtime-env-json", kind="http", host=PLATFORM_HOST, path="/runtime-env.json")
    add("platform-runtime-env-js", kind="http", host=PLATFORM_HOST, path="/runtime-env.js")
    add("platform-spa-fallback", kind="http", host=PLATFORM_HOST, path="/v1/iam/public/bootstrap")
    add("platform-random-path", kind="http", host=PLATFORM_HOST, path="/deep/nested/route", ua=DESKTOP_UA)

    # -- TLS family ----------------------------------------------------------
    for sni in API_DEV_HOSTS + [MALL_HOST, PLATFORM_HOST]:
        add(f"tls-{sni}-root", kind="tls", sni=sni, path="/")
    add("tls-unknown-sni-drop", kind="tls", sni=UNKNOWN_HOST, path="/")
    add("tls-no-sni-drop", kind="tls", sni=None, path="/")
    return cases


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--http", default="127.0.0.1:80", help="host:port of the plain-HTTP edge")
    parser.add_argument("--tls", default="127.0.0.1:443", help="host:port of the TLS edge")
    parser.add_argument("--out", required=True, help="capture JSON output path")
    parser.add_argument("--label", default="", help="provenance label recorded in the capture")
    args = parser.parse_args()

    http_target = urlsplit(f"//{args.http}", scheme="http")
    tls_target = urlsplit(f"//{args.tls}", scheme="https")
    http = (http_target.hostname or "127.0.0.1", http_target.port or 80)
    tls = (tls_target.hostname or "127.0.0.1", tls_target.port or 443)

    results: dict[str, dict[str, object]] = {}
    started = time.time()
    for case in build_cases(http, tls):
        case_id = str(case["id"])
        if case["kind"] == "http":
            extra = {"User-Agent": str(case.get("ua", DESKTOP_UA))}
            if "extra_headers" in case:
                extra.update(case["extra_headers"])  # type: ignore[arg-type]
            response = http_capture(
                http[0],
                http[1],
                host_header=str(case["host"]),
                path=str(case["path"]),
                extra_headers=extra,
                method=str(case.get("method", "GET")),
                send_host_header=bool(case.get("send_host_header", True)),
            )
        else:
            response = tls_capture(
                tls[0],
                tls[1],
                sni=case.get("sni"),  # type: ignore[arg-type]
                path=str(case["path"]),
            )
        results[case_id] = response.to_json()

    # Dynamic immutable-asset probe: fingerprinted Vite assets carry the
    # immutable cache contract; resolve one real path from the desktop index.
    desktop_index = results.get("mall-desktop-index", {})
    asset_probe_added = 0
    for index_case, host in (
        ("mall-desktop-index", MALL_HOST),
        ("platform-desktop-index", PLATFORM_HOST),
    ):
        prefix = results.get(index_case, {}).get("bodyPrefix", "") or ""
        match = ASSET_PATH_RE.search(prefix)
        if match:
            asset_path = match.group(1)
            results[f"{index_case}-asset"] = http_capture(
                http[0], http[1], host_header=host, path=asset_path
            ).to_json()
            asset_probe_added += 1

    capture = {
        "schema": 1,
        "kind": "sdkwork-edge-parity-capture",
        "label": args.label,
        "capturedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "target": {"http": f"{http[0]}:{http[1]}", "tls": f"{tls[0]}:{tls[1]}"},
        "caseFloor": CASE_FLOOR,
        "caseCount": len(results),
        "dynamicAssetProbes": asset_probe_added,
        "durationSeconds": round(time.time() - started, 2),
        "cases": results,
    }
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(capture, handle, indent=2, sort_keys=True)
        handle.write("\n")

    dropped = sum(1 for case in results.values() if case.get("dropped"))
    statuses: dict[int, int] = {}
    for case in results.values():
        status = case.get("status")
        if status is not None:
            statuses[int(status)] = statuses.get(int(status), 0) + 1
    print(f"capture: {len(results)} cases ({asset_probe_added} dynamic asset probes)")
    print(f"dropped connections: {dropped}")
    print(f"status histogram: {json.dumps(statuses, sort_keys=True)}")
    if len(results) < CASE_FLOOR:
        print(f"capture below case floor ({len(results)} < {CASE_FLOOR})", file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
