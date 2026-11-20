#!/usr/bin/env python3
"""Formal deployment capability regression for the sdkwork-webserver edge.

Where `battery.py` records the multi-vhost development edge against the
committed stock-nginx oracle, this runner verifies the *deployment channels*
themselves, per the delivery contract each channel owns:

- `--mode installer` — the native `.deb`/`.rpm` installer channel
  (`docs/guides/operator/deb-install.md`): package integrity, systemd units,
  the production edge takeover of host :80/:443 by `sdkwork-webserver-edge`
  (SDKWORK_WEBSERVER_SPEC §0.1: stock nginx must not serve SDKWork public
  domains), ACME HTTP-01 webroot, HTTP→HTTPS redirect, TLS/ALPN semantics,
  gateway behind the edge, direct gateway health, edge config
  self-validation, and the host-level absence of stock nginx.
- `--mode full-edge` — the multi-vhost development edge (the container
  channel on the api-dev host): the full 27-case edge-parity battery capture
  compared against the committed stock-nginx oracle with zero behavioural
  differences, plus derived assertions (drop semantics, identity banner).

Every check prints one `PASS`/`FAIL` line with evidence and the runner exits
non-zero when any check fails, so a deployment is "capability aligned" only
when every line is green.

Usage (on the edge host):
    python3 deployment-regression.py --mode installer \
        [--environment production] [--domain server.sdkwork.com]
    python3 deployment-regression.py --mode full-edge \
        [--http 127.0.0.1:80] [--tls 127.0.0.1:443]

Exit codes: 0 all checks passed, 1 at least one check failed,
2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import ssl
import socket
import subprocess
import sys
import tempfile
from pathlib import Path
from urllib.parse import urlsplit

HOP_BY_HOP = {
    "connection",
    "keep-alive",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
}

IDENTITY_BANNER = "sdkwork-webserver"

GATEWAY_BINARY = "sdkwork-api-webserver-standalone-gateway"
INSTALL_BINARY_ROOT = "/usr/lib/sdkwork/webserver/bin"
EDGE_CONF = {
    "production": "/etc/sdkwork/webserver/nginx.production.conf",
    "test": "/etc/sdkwork/webserver/nginx.test.conf",
}
GATEWAY_SERVICE = {
    "production": "sdkwork-webserver",
    "test": "sdkwork-webserver-test",
}
DEFAULT_DOMAIN = {
    "production": "server.sdkwork.com",
    "test": "server-test.sdkwork.com",
}
DEFAULT_GATEWAY_BIND = {
    "production": "127.0.0.1:8080",
    "test": "127.0.0.1:8888",
}


class CheckResult:
    """One named verification with human-readable evidence."""

    def __init__(self, check_id: str, ok: bool, evidence: str) -> None:
        self.check_id = check_id
        self.ok = ok
        self.evidence = evidence

    def line(self) -> str:
        verb = "PASS" if self.ok else "FAIL"
        return f"{verb} {self.check_id} — {self.evidence}"


class RawResponse:
    """Minimal HTTP response captured over a raw socket."""

    def __init__(self) -> None:
        self.status: int | None = None
        self.headers: dict[str, str] = {}
        self.body = b""
        self.error: str | None = None


def raw_request(
    target: str,
    path: str,
    host_header: str,
    extra_headers: dict[str, str] | None = None,
    method: str = "GET",
) -> RawResponse:
    """One one-shot HTTP/1.1 request; transport failures become errors."""
    response = RawResponse()
    host, _, port_text = target.partition(":")
    port = int(port_text or "80")
    lines = [f"{method} {path} HTTP/1.1", f"Host: {host_header}"]
    for key, value in (extra_headers or {}).items():
        lines.append(f"{key}: {value}")
    lines += [
        "User-Agent: sdkwork-deployment-regression/1",
        "Accept: */*",
        "Connection: close",
    ]
    payload = ("\r\n".join(lines) + "\r\n\r\n").encode("utf-8")
    try:
        sock = socket.create_connection((host, port), timeout=10)
    except OSError as exc:
        response.error = f"connect: {type(exc).__name__}: {exc}"
        return response
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
            response.error = "response: empty (connection closed without data)"
            return response
        head, _, body = raw.partition(b"\r\n\r\n")
        text_lines = head.decode("latin-1").split("\r\n")
        if not text_lines or not text_lines[0].startswith("HTTP/"):
            response.error = f"response: malformed status line {text_lines[:1]}"
            return response
        response.status = int(text_lines[0].split()[1])
        for line in text_lines[1:]:
            if ":" in line:
                name, _, value = line.partition(":")
                name = name.strip().lower()
                if name not in HOP_BY_HOP:
                    response.headers[name] = value.strip()
        response.body = body
    finally:
        sock.close()
    return response


def tls_probe(
    target: str,
    sni: str,
    alpn: list[str],
    path: str = "/",
    host_header: str | None = None,
) -> tuple[RawResponse, dict[str, object] | None]:
    """TLS handshake (+ optional HTTP round trip) returning TLS facts."""
    response = RawResponse()
    host, _, port_text = target.partition(":")
    port = int(port_text or "443")
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    context.set_alpn_protocols(alpn)
    try:
        raw_sock = socket.create_connection((host, port), timeout=10)
    except OSError as exc:
        response.error = f"connect: {type(exc).__name__}: {exc}"
        return response, None
    try:
        try:
            sock = context.wrap_socket(raw_sock, server_hostname=sni)
        except (ssl.SSLError, OSError, ValueError) as exc:
            response.error = f"handshake: {type(exc).__name__}: {exc}"
            return response, None
        cert = sock.getpeercert(binary_form=True)
        tls_facts: dict[str, object] = {
            "protocol": sock.version(),
            "cipher": sock.cipher()[0] if sock.cipher() else None,
            "alpn": sock.selected_alpn_protocol(),
        }
        if alpn != ["http/1.1"]:
            # Only the plain handshake probes return without an HTTP exchange.
            return response, tls_facts
        if cert:
            tls_facts["certSha256"] = hashlib.sha256(cert).hexdigest()
        # One-shot HTTP/1.1 exchange over the established TLS socket.
        lines = [f"GET {path} HTTP/1.1", f"Host: {host_header or sni}"]
        lines += [
            "User-Agent: sdkwork-deployment-regression/1",
            "Accept: */*",
            "Connection: close",
        ]
        payload = ("\r\n".join(lines) + "\r\n\r\n").encode("utf-8")
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
        data = b"".join(chunks)
        if not data:
            response.error = "response: empty over TLS"
            return response, tls_facts
        head, _, body = data.partition(b"\r\n\r\n")
        text_lines = head.decode("latin-1").split("\r\n")
        if not text_lines or not text_lines[0].startswith("HTTP/"):
            response.error = "response: malformed status line over TLS"
            return response, tls_facts
        response.status = int(text_lines[0].split()[1])
        for line in text_lines[1:]:
            if ":" in line:
                name, _, value = line.partition(":")
                name = name.strip().lower()
                if name not in HOP_BY_HOP:
                    response.headers[name] = value.strip()
        response.body = body
        return response, tls_facts
    finally:
        raw_sock.close()


def run_command(argv: list[str]) -> tuple[int, str]:
    """Run a command, returning (exit_code, combined_output)."""
    try:
        proc = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            timeout=60,
        )
    except FileNotFoundError:
        return 127, f"command not found: {argv[0]}"
    except subprocess.TimeoutExpired:
        return 124, f"timeout: {' '.join(argv)}"
    output = (proc.stdout or "") + (proc.stderr or "")
    return proc.returncode, output.strip()


# ---------------------------------------------------------------------------
# Installer-mode checks (native .deb/.rpm channel)
# ---------------------------------------------------------------------------


def check_package_installed(environment: str) -> CheckResult:
    names = ["sdkwork-webserver"] if environment == "production" else ["sdkwork-webserver-test"]
    for name in names:
        code, out = run_command(
            ["dpkg-query", "-W", "-f=${Status} ${Version}", name]
        )
        if code == 0 and "install ok installed" in out:
            return CheckResult("package-installed", True, f"dpkg: {name}: {out}")
    code, out = run_command(["rpm", "-q", *names])
    if code == 0:
        return CheckResult("package-installed", True, f"rpm: {out}")
    return CheckResult(
        "package-installed",
        False,
        f"no installed package found (dpkg-query exit {code}: {out or 'empty'})",
    )


def check_unit_active(unit: str, required: bool = True) -> CheckResult:
    code, out = run_command(["systemctl", "is-active", unit])
    active = out.strip() == "active"
    ok = active or not required
    return CheckResult(
        f"unit-{unit}-active",
        ok,
        f"systemctl is-active {unit} -> {out.strip() or code}",
    )


def check_edge_ports_bound(http_target: str, tls_target: str) -> CheckResult:
    code, out = run_command(["ss", "-tln"])
    if code != 0:
        return CheckResult("edge-ports-bound", False, f"ss failed: {out}")
    problems = []
    for label, target in (("http", http_target), ("tls", tls_target)):
        port = target.partition(":")[2]
        if f":{port}" not in out:
            problems.append(f"no listener on :{port}")
    if problems:
        return CheckResult("edge-ports-bound", False, "; ".join(problems))
    return CheckResult("edge-ports-bound", True, f"listeners present on {http_target} and {tls_target}")


def check_edge_http_answers(http_target: str, domain: str) -> CheckResult:
    response = raw_request(http_target, "/", domain)
    if response.error:
        return CheckResult("edge-http-answers", False, response.error)
    return CheckResult(
        "edge-http-answers",
        True,
        f"status={response.status} server={response.headers.get('server', 'none')}",
    )


def check_edge_http_redirect(http_target: str, domain: str) -> CheckResult:
    response = raw_request(http_target, "/sdkwork-regression-path", domain)
    if response.error:
        return CheckResult("edge-http-redirect", False, response.error)
    location = response.headers.get("location", "")
    ok = response.status in (301, 302, 307, 308) and location.startswith(
        f"https://{domain}"
    )
    return CheckResult(
        "edge-http-redirect",
        ok,
        f"status={response.status} location={location or 'none'}",
    )


def check_edge_acme_webroot(http_target: str, domain: str) -> CheckResult:
    response = raw_request(
        http_target, "/.well-known/acme-challenge/sdkwork-regression-probe", domain
    )
    if response.error:
        return CheckResult("edge-acme-webroot", False, response.error)
    ok = response.status in (200, 403, 404)
    return CheckResult(
        "edge-acme-webroot",
        ok,
        f"status={response.status} (ACME HTTP-01 webroot answered, no redirect)",
    )


def check_edge_tls_handshake(tls_target: str, domain: str) -> CheckResult:
    _, facts = tls_probe(tls_target, domain, ["http/1.1"], path="/healthz")
    if facts is None:
        return CheckResult("edge-tls-handshake", False, "handshake failed")
    return CheckResult(
        "edge-tls-handshake",
        True,
        f"protocol={facts.get('protocol')} cipher={facts.get('cipher')}",
    )


def check_edge_tls_alpn_h2(tls_target: str, domain: str) -> CheckResult:
    _, facts = tls_probe(tls_target, domain, ["h2", "http/1.1"])
    if facts is None:
        return CheckResult("edge-tls-alpn-h2", False, "handshake failed")
    alpn = facts.get("alpn")
    return CheckResult(
        "edge-tls-alpn-h2",
        alpn == "h2",
        f"alpn={alpn} (offer [h2, http/1.1])",
    )


def check_edge_tls_cert_domain(tls_target: str, domain: str) -> CheckResult:
    """The leaf certificate must name the edge domain (bootstrap self-signed
    material is a designed cold-start state; the identity, not the trust
    chain, is the deployment contract here)."""
    host, _, port_text = tls_target.partition(":")
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    try:
        raw_sock = socket.create_connection((host, int(port_text or "443")), timeout=10)
        sock = context.wrap_socket(raw_sock, server_hostname=domain)
    except (OSError, ssl.SSLError, ValueError) as exc:
        return CheckResult("edge-tls-cert-domain", False, f"handshake failed: {exc}")
    try:
        der = sock.getpeercert(binary_form=True)
    finally:
        raw_sock.close()
    if not der:
        return CheckResult("edge-tls-cert-domain", False, "no certificate presented")
    # Minimal DER scan for the domain in the SAN/CN printable strings.
    needle = domain.encode("ascii")
    ok = needle in der
    return CheckResult(
        "edge-tls-cert-domain",
        ok,
        f"leaf certificate {'contains' if ok else 'does not contain'} {domain}",
    )


def check_edge_https_gateway(tls_target: str, domain: str) -> CheckResult:
    response, facts = tls_probe(
        tls_target, domain, ["http/1.1"], path="/", host_header=domain
    )
    if response.error:
        return CheckResult("edge-https-gateway", False, response.error)
    server = response.headers.get("server", "")
    ok = (
        response.status is not None
        and response.status < 500
        and response.status not in (502, 504)
        and server == IDENTITY_BANNER
    )
    return CheckResult(
        "edge-https-gateway",
        ok,
        f"status={response.status} server={server or 'none'} alpn={facts.get('alpn') if facts else '?'}",
    )


def check_identity_banner(http_target: str, domain: str) -> CheckResult:
    """The edge identifies itself as sdkwork-webserver, never as nginx."""
    response = raw_request(http_target, "/", domain)
    server = response.headers.get("server", "")
    ok = server == IDENTITY_BANNER
    return CheckResult(
        "edge-identity-banner",
        ok,
        f"server header={server or 'none'} (expected {IDENTITY_BANNER})",
    )


def check_gateway_health(gateway_target: str) -> CheckResult:
    response = raw_request(gateway_target, "/healthz", "localhost")
    if response.error:
        return CheckResult("gateway-direct-health", False, response.error)
    return CheckResult(
        "gateway-direct-health",
        response.status == 200,
        f"GET http://{gateway_target}/healthz -> {response.status}",
    )


def check_edge_config_validates(environment: str) -> CheckResult:
    binary = f"{INSTALL_BINARY_ROOT}/{GATEWAY_BINARY}"
    conf = EDGE_CONF[environment]
    if not Path(binary).exists():
        return CheckResult("edge-config-validates", False, f"missing {binary}")
    if not Path(conf).exists():
        return CheckResult("edge-config-validates", False, f"missing {conf}")
    code, out = run_command([binary, "validate-nginx", conf])
    ok = code == 0
    return CheckResult(
        "edge-config-validates",
        ok,
        f"validate-nginx {conf} -> exit {code} {out[:160]}",
    )


def check_no_stock_nginx() -> CheckResult:
    """Standalone-only: stock nginx must not exist on the host."""
    findings = []
    if shutil.which("nginx"):
        findings.append("nginx binary on PATH")
    code, out = run_command(["dpkg-query", "-W", "-f=${Status} ${Package}\n", "nginx", "nginx-core"])
    if code == 0:
        for line in out.splitlines():
            if "install ok installed" in line:
                findings.append(f"dpkg: {line}")
    code, out = run_command(["systemctl", "is-active", "nginx"])
    if out.strip() == "active":
        findings.append("nginx systemd unit active")
    ok = not findings
    return CheckResult(
        "no-stock-nginx",
        ok,
        "; ".join(findings) if findings else "no nginx binary/package/active unit on host",
    )


# ---------------------------------------------------------------------------
# Full-edge-mode checks (multi-vhost development edge, oracle parity)
# ---------------------------------------------------------------------------


def run_full_edge_checks(args: argparse.Namespace) -> list[CheckResult]:
    script_dir = Path(__file__).resolve().parent
    repo_root = Path(args.repo_root) if args.repo_root else script_dir.parents[2]
    battery = script_dir / "battery.py"
    compare = script_dir / "compare.py"
    oracle = script_dir / "nginx-edge-oracle.json"
    candidate = Path(tempfile.mkstemp(suffix="-edge-capture.json")[1])

    results: list[CheckResult] = []
    code, out = run_command(
        [
            sys.executable,
            str(battery),
            "--http",
            args.http,
            "--tls",
            args.tls,
            "--label",
            "deployment-regression-candidate",
            "--out",
            str(candidate),
        ]
    )
    if code != 0:
        results.append(
            CheckResult("battery-capture", False, f"battery.py exit {code}: {out[-400:]}")
        )
        return results
    capture = json.loads(candidate.read_text(encoding="utf-8"))
    case_count = capture.get("caseCount", 0)
    results.append(
        CheckResult(
            "battery-capture",
            case_count >= 24,
            f"{case_count} cases captured from {args.http}/{args.tls}",
        )
    )

    code, out = run_command(
        [
            sys.executable,
            str(compare),
            "--oracle",
            str(oracle),
            "--candidate",
            str(candidate),
        ]
    )
    if code == 0:
        results.append(
            CheckResult(
                "oracle-parity",
                True,
                "compare.py exit 0: zero behavioural differences vs stock-nginx oracle",
            )
        )
    else:
        tail = "\n".join(out.splitlines()[-12:])
        results.append(
            CheckResult(
                "oracle-parity",
                False,
                f"compare.py exit {code}; tail:\n{tail}",
            )
        )

    cases = capture.get("cases", {})
    dropped = [name for name, case in cases.items() if case.get("dropped")]
    results.append(
        CheckResult(
            "drop-semantics",
            len(dropped) >= 2,
            f"dropped connections: {sorted(dropped) or 'none'}",
        )
    )

    nginx_banner = [
        name
        for name, case in cases.items()
        if not case.get("dropped")
        and case.get("headers", {}).get("server", "").lower().startswith("nginx")
    ]
    results.append(
        CheckResult(
            "identity-banner",
            not nginx_banner,
            (
                "every answered case carries the sdkwork-webserver banner"
                if not nginx_banner
                else f"cases answering with an nginx banner: {nginx_banner}"
            ),
        )
    )

    bootstrap_cases = {
        name: case.get("bootstrapAppId")
        for name, case in cases.items()
        if case.get("bootstrapAppId")
    }
    results.append(
        CheckResult(
            "bootstrap-handoff",
            len(bootstrap_cases) >= 3,
            f"bootstrap token app identities: {bootstrap_cases}",
        )
    )
    return results


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------


def parse_target(value: str) -> str:
    parts = urlsplit(f"//{value}")
    if not parts.hostname or not parts.port:
        raise argparse.ArgumentTypeError(f"expected host:port, got {value!r}")
    return f"{parts.hostname}:{parts.port}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--mode", choices=("installer", "full-edge"), required=True)
    parser.add_argument("--environment", choices=("production", "test"), default="production")
    parser.add_argument("--domain", default=None, help="edge domain (installer mode)")
    parser.add_argument("--http", type=parse_target, default="127.0.0.1:80")
    parser.add_argument("--tls", type=parse_target, default="127.0.0.1:443")
    parser.add_argument("--gateway", type=parse_target, default=None)
    parser.add_argument("--repo-root", default=None, help="repository root (full-edge mode)")
    parser.add_argument("--json", dest="json_out", default=None, help="write a JSON verdict")
    args = parser.parse_args()

    results: list[CheckResult]
    if args.mode == "full-edge":
        results = run_full_edge_checks(args)
    else:
        domain = args.domain or DEFAULT_DOMAIN[args.environment]
        gateway_target = args.gateway or DEFAULT_GATEWAY_BIND[args.environment]
        results = [
            check_package_installed(args.environment),
            check_unit_active(GATEWAY_SERVICE[args.environment]),
        ]
        if args.environment == "production":
            results.append(check_unit_active("sdkwork-webserver-edge"))
            results.append(check_unit_active("sdkwork-webserver-certificate-worker"))
            results.append(check_edge_ports_bound(args.http, args.tls))
            results.append(check_edge_http_answers(args.http, domain))
            results.append(check_edge_http_redirect(args.http, domain))
            results.append(check_edge_acme_webroot(args.http, domain))
            results.append(check_edge_tls_handshake(args.tls, domain))
            results.append(check_edge_tls_alpn_h2(args.tls, domain))
            results.append(check_edge_tls_cert_domain(args.tls, domain))
            results.append(check_edge_https_gateway(args.tls, domain))
            results.append(check_identity_banner(args.http, domain))
            results.append(check_gateway_health(gateway_target))
            results.append(check_edge_config_validates(args.environment))
        else:
            results.append(check_gateway_health(gateway_target))
        results.append(check_no_stock_nginx())

    for result in results:
        print(result.line())
    failed = [result for result in results if not result.ok]
    verdict = "aligned" if not failed else "drift"
    print(f"\nverdict: {verdict} ({len(results) - len(failed)}/{len(results)} checks passed)")

    if args.json_out:
        payload = {
            "mode": args.mode,
            "environment": getattr(args, "environment", None),
            "verdict": verdict,
            "checks": [
                {
                    "id": result.check_id,
                    "ok": result.ok,
                    "evidence": result.evidence,
                }
                for result in results
            ],
        }
        Path(args.json_out).write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
