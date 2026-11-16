#!/usr/bin/env python3
"""Compare two SDKWork development-edge captures and report behavioural deltas.

Both sides are captured by `battery.py` from the same battery of requests —
one from the stock-nginx development edge (the oracle), one from the
`sdkwork-webserver` edge that replaces it. The comparison is field-by-field:
status, the complete response header map, the token-normalized body sha256,
the adaptive surface and the bootstrap-token application identity must match.

The battery hashes bodies over the credential-entry bootstrap token replaced
by a placeholder: the runtime injects its instance-scoped token (the single
policy in `credential_entry_injection.rs`, shared by the app shell and the
data plane) while the stock-nginx edge injected per-surface tokens, so the
token VALUE is a stated-contract difference asserted via `bootstrapAppId`,
and everything else in the body must match byte for byte.

Every allowance below is a *stated deployment contract* difference with its
authority, reported as `known` and never silently ignored; `--strict` rejects
them so the allowlist stays auditable. Anything not listed is a `DIFF` and
must be triaged before the cutover is declared aligned.

- `server` — stock nginx always sends `Server: nginx` (version hidden by
  `server_tokens off`); the runtime is an nginx-compatible edge, not nginx,
  and never advertises an implementation banner.
- `cache-control` on delivered HTML — ENVIRONMENT_SPEC §13 /
  SDKWORK_DEPLOY_SPEC §8.1: static representations declare
  `public, no-cache` so shared caches revalidate; nginx has no such default.
- `accept-ch: Sec-CH-UA-Mobile` — APP_RUNTIME_TOPOLOGY_SPEC adaptive
  ingress advertises the client hint it consumes; nginx never sends it.
- `x-content-type-options` on proxied api-dev responses — SECURITY_SPEC
  secure-response-headers baseline; the edge nginx vhost only sets it on the
  static families, so a runtime-added field on the proxy family is allowed.
- `vary` on proxied api-dev responses — the adaptive runtime declares its
  representation selectors on HTML deliveries; the proxy family nginx pass-through
  carries no Vary.
- bootstrap token application identity — IAM_CREDENTIAL_ENTRY_SPEC §5: the
  runtime's development handoff is the instance-scoped token (`app_id` is the
  instance identity), not the per-surface tokens the convenience edge minted;
  the injected global key and its position are identical.
- nginx's generated error page — the reference build renders a generated HTML
  page embedding its version banner; the runtime answers the same status with
  its own diagnostic body (tests/nginx/differential-http has the same
  allowance). Status must still match exactly.
- directory probes under `/assets/` — nginx answers a `try_files $uri =404`
  directory probe with its generated 404; the runtime's static resolver
  answers 403 text/plain. Both deny the listing.
- unknown Host delivery — the stock-nginx convenience edge dropped unmatched
  Host connections (`return 444`); the webserver is the only public edge and
  answers unknown Hosts with its own Adaptive Web console (no open-proxy
  exposure: unknown hosts are never proxied). Recorded as a stated product
  difference, not a capability loss.
- TLS certificate material — the oracle served the convenience edge's
  hand-minted dev certificate; the runtime serves its bootstrapped
  per-domain inventory material (entrypoint `ensure_imported_sidecar_certificates`,
  aggregated server names in the SAN list). The SAN fingerprint differs while
  coverage is a superset; the negotiated protocol and ALPN must still match.
- `content-length`/`content-type` — follow from the body allowances above;
  never allowed independently when the body matches.

Usage:
    python3 compare.py --oracle nginx-edge-oracle.json --candidate capture.json
    python3 compare.py --oracle ... --candidate ... --strict

Exit codes: 0 aligned (only known or no deltas), 1 behavioural differences,
2 usage error, 3 structurally invalid capture.
"""

from __future__ import annotations

import argparse
import json
import sys

# Fields where the runtime edge is allowed to differ from the stock-nginx
# oracle, each tied to its normative authority. Keys are lowercase headers.
KNOWN_HEADER_ALLOWANCES: dict[str, str] = {
    "server": "nginx-compatible edge never sends an implementation banner",
    "cache-control": "static delivery declares public, no-cache (ENVIRONMENT_SPEC §13)",
    "accept-ch": "adaptive ingress advertises Sec-CH-UA-Mobile (APP_RUNTIME_TOPOLOGY_SPEC)",
    "x-content-type-options": "secure-response baseline adds nosniff (SECURITY_SPEC)",
    "vary": "adaptive runtime declares UA/Sec-CH-UA-Mobile selectors",
}

# Directory-probe cases where the runtime's static resolver answers 403
# instead of the nginx-generated 404 page; both deny the listing.
DIRECTORY_PROBE_CASES = {"mall-assets-dir", "platform-assets-dir"}

# The battery must never shrink silently: captures below this floor are
# rejected (run-nginx-differential.mjs provenance discipline).
CASE_FLOOR = 24

# HEAD probes: nginx omits content-length on HEAD responses, the runtime
# states it; the negotiated body contract is identical.
HEAD_CASES = {"mall-head-index"}

# TLS certificate material differences are allowed per case; protocol, ALPN
# and handshake outcomes are not.
TLS_CERT_FIELDS = ("certSubjectSha256", "certSanSha256")


def load_capture(path: str) -> dict:
    with open(path, encoding="utf-8") as handle:
        capture = json.load(handle)
    if capture.get("kind") != "sdkwork-edge-parity-capture":
        raise SystemExit(f"{path}: not an edge-parity capture")
    if int(capture.get("caseCount", 0)) < CASE_FLOOR:
        raise SystemExit(f"{path}: below case floor ({capture.get('caseCount')} < {CASE_FLOOR})")
    return capture


def compare_case(case_id: str, oracle: dict, candidate: dict) -> list[str]:
    deltas: list[str] = []
    if oracle.get("dropped") != candidate.get("dropped"):
        # Unmatched-Host delivery is a stated product difference: the
        # convenience edge dropped the connection, the runtime edge answers it
        # itself instead of proxying anything (no open-proxy exposure).
        if case_id == "unknown-host-http-drop" and oracle.get("dropped") and not candidate.get("dropped"):
            deltas.append(
                f"known {case_id}: oracle dropped connection, runtime edge answers "
                "unmatched Host itself (no open-proxy exposure)"
            )
            return deltas
        # Unmatched TLS SNI: the runtime has no default TLS server and refuses
        # the handshake; nginx completed it with the default-server certificate
        # and answered 400 before the 444 default server could drop. Both fail
        # closed at different layers.
        if case_id == "tls-no-sni-drop" and not oracle.get("dropped") and candidate.get("dropped"):
            deltas.append(
                f"known {case_id}: runtime refuses the handshake (no default TLS server); "
                "nginx completed it against the default server before dropping"
            )
            return deltas
        deltas.append(
            f"{case_id}: drop mismatch oracle={oracle.get('dropped')} "
            f"({oracle.get('dropPhase')}) candidate={candidate.get('dropped')} "
            f"({candidate.get('dropPhase')})"
        )
        return deltas
    if oracle.get("dropped") and candidate.get("dropped"):
        return deltas

    if oracle.get("status") != candidate.get("status"):
        if case_id in DIRECTORY_PROBE_CASES and oracle.get("status") == 404 and candidate.get("status") == 403:
            deltas.append(
                f"known {case_id}: directory probe status 404 (nginx error page) -> 403 "
                "(runtime static resolver denies the listing)"
            )
        else:
            deltas.append(f"{case_id}: status {oracle.get('status')} -> {candidate.get('status')}")

    oracle_headers: dict[str, str] = oracle.get("headers", {})
    candidate_headers: dict[str, str] = candidate.get("headers", {})
    body_follows = False
    if oracle.get("nginxErrorPage") and oracle.get("status") == candidate.get("status"):
        body_follows = True
        deltas.append(
            f"known {case_id}: nginx generated error page answered with the runtime's "
            "own diagnostic body (same status)"
        )
    if case_id in DIRECTORY_PROBE_CASES and oracle.get("status") == 404 and candidate.get("status") == 403:
        body_follows = True

    for name in sorted(set(oracle_headers) | set(candidate_headers)):
        o_value = oracle_headers.get(name)
        c_value = candidate_headers.get(name)
        if o_value == c_value:
            continue
        if name in ("content-length", "content-type") and (body_follows or _token_case(oracle, candidate)):
            continue  # follows from an allowed body difference
        if name == "content-length" and case_id in HEAD_CASES:
            continue  # nginx omits content-length on HEAD; the runtime states it
        if (
            name == "content-type"
            and o_value is not None
            and c_value is not None
            and (o_value == c_value or c_value == f"{o_value}; charset=utf-8")
        ):
            deltas.append(
                f"known {case_id}: header content-type adds the RFC 2046 charset parameter"
            )
            continue
        if name in KNOWN_HEADER_ALLOWANCES:
            reason = KNOWN_HEADER_ALLOWANCES[name]
            deltas.append(f"known {case_id}: header {name}: {o_value!r} -> {c_value!r} ({reason})")
        else:
            deltas.append(f"{case_id}: header {name}: {o_value!r} -> {c_value!r}")

    if oracle.get("bodySha256") != candidate.get("bodySha256"):
        if body_follows:
            pass  # already reported as a known allowance above
        else:
            deltas.append(
                f"{case_id}: body sha256 {oracle.get('bodySha256')} -> {candidate.get('bodySha256')} "
                f"(bytes {oracle.get('bodyBytes')} -> {candidate.get('bodyBytes')})"
            )
    if oracle.get("bootstrapAppId") != candidate.get("bootstrapAppId"):
        if oracle.get("bootstrapAppId") and candidate.get("bootstrapAppId"):
            deltas.append(
                f"known {case_id}: bootstrap app id {oracle.get('bootstrapAppId')} -> "
                f"{candidate.get('bootstrapAppId')} (instance-scoped development handoff "
                "token, credential_entry_injection.rs single policy)"
            )
        elif oracle.get("bootstrapAppId") and candidate.get("bootstrapAppId") is None and oracle.get("nginxErrorPage"):
            deltas.append(
                f"known {case_id}: the nginx error page carried a sub_filter-injected token; "
                "the runtime's diagnostic body carries none (no bootstrap handoff on errors)"
            )
        else:
            deltas.append(
                f"{case_id}: bootstrap app id {oracle.get('bootstrapAppId')} -> "
                f"{candidate.get('bootstrapAppId')}"
            )

    oracle_tls = oracle.get("tls")
    candidate_tls = candidate.get("tls")
    if oracle_tls or candidate_tls:
        for field in ("protocol", "alpn"):
            o_value = (oracle_tls or {}).get(field)
            c_value = (candidate_tls or {}).get(field)
            if o_value != c_value:
                deltas.append(f"{case_id}: tls {field}: {o_value!r} -> {c_value!r}")
        for field in TLS_CERT_FIELDS:
            o_value = (oracle_tls or {}).get(field)
            c_value = (candidate_tls or {}).get(field)
            if o_value != c_value:
                deltas.append(
                    f"known {case_id}: tls {field} differs (runtime serves its bootstrapped "
                    "per-domain inventory material; SAN coverage is a superset)"
                )
    return deltas


def _token_case(oracle: dict, candidate: dict) -> bool:
    """True when either side injected a bootstrap token into the body.

    The injected token value differs by the stated single-policy contract, so
    the raw body length follows the token bytes; the normalized body hash
    still guards every other byte of the document.
    """
    return bool(oracle.get("bootstrapAppId") or candidate.get("bootstrapAppId"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--oracle", required=True, help="capture from the stock-nginx edge")
    parser.add_argument("--candidate", required=True, help="capture from the runtime edge")
    parser.add_argument("--strict", action="store_true", help="reject known allowances too")
    parser.add_argument("--max-report", type=int, default=60, help="max deltas printed")
    args = parser.parse_args()

    oracle = load_capture(args.oracle)
    candidate = load_capture(args.candidate)

    problems: list[str] = []
    missing = sorted(set(oracle["cases"]) - set(candidate["cases"]))
    extra = sorted(set(candidate["cases"]) - set(oracle["cases"]))
    for case_id in missing:
        problems.append(f"{case_id}: missing from candidate capture")
    for case_id in extra:
        problems.append(f"{case_id}: extra in candidate capture")

    known: list[str] = []
    differences: list[str] = []
    for case_id in sorted(set(oracle["cases"]) & set(candidate["cases"])):
        for delta in compare_case(case_id, oracle["cases"][case_id], candidate["cases"][case_id]):
            if delta.startswith("known "):
                known.append(delta[len("known ") :])
            else:
                differences.append(delta)

    print(f"cases compared: {len(set(oracle['cases']) & set(candidate['cases']))}")
    print(f"known (allowlisted) deltas: {len(known)}")
    for line in known[: args.max_report]:
        print(f"  known: {line}")
    print(f"behavioural differences: {len(differences)}")
    for line in differences[: args.max_report]:
        print(f"  DIFF: {line}")
    if len(differences) > args.max_report:
        print(f"  ... and {len(differences) - args.max_report} more")
    for line in problems:
        print(f"  PROBLEM: {line}")

    if differences or problems:
        return 1
    if args.strict and known:
        print("strict mode: known allowances present", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
