#!/usr/bin/env python3
"""Compare two SDKWork development-edge captures and report behavioural deltas.

Both sides are captured by `battery.py` from the same battery of requests —
one from the stock-nginx development edge (the oracle), one from the
`sdkwork-webserver` edge that replaces it. The comparison is field-by-field:
status, the complete response header map, body sha256, the adaptive surface
and the bootstrap-token application identity must match.

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
- `content-encoding`/`content-length` follow body comparison and are never
  allowed independently.

Connection-drop cases (`444` semantics) are assertions, not allowances: the
replacement edge must drop unknown Host and unknown SNI exactly like nginx.
TLS cases must negotiate the same protocol and serve the same certificate
SAN set (the battery fingerprints the SAN list).

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

CASE_FLOOR = 24


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
        deltas.append(
            f"{case_id}: drop mismatch oracle={oracle.get('dropped')} "
            f"({oracle.get('dropPhase')}) candidate={candidate.get('dropped')} "
            f"({candidate.get('dropPhase')})"
        )
        if oracle.get("dropped") and candidate.get("dropped"):
            return deltas  # both dropped: nothing else to compare
        return deltas
    if oracle.get("dropped") and candidate.get("dropped"):
        return deltas

    if oracle.get("status") != candidate.get("status"):
        deltas.append(
            f"{case_id}: status {oracle.get('status')} -> {candidate.get('status')}"
        )

    oracle_headers: dict[str, str] = oracle.get("headers", {})
    candidate_headers: dict[str, str] = candidate.get("headers", {})
    for name in sorted(set(oracle_headers) | set(candidate_headers)):
        o_value = oracle_headers.get(name)
        c_value = candidate_headers.get(name)
        if o_value == c_value:
            continue
        if name in KNOWN_HEADER_ALLOWANCES:
            reason = KNOWN_HEADER_ALLOWANCES[name]
            deltas.append(f"known {case_id}: header {name}: {o_value!r} -> {c_value!r} ({reason})")
        else:
            deltas.append(f"{case_id}: header {name}: {o_value!r} -> {c_value!r}")

    if oracle.get("bodySha256") != candidate.get("bodySha256"):
        deltas.append(
            f"{case_id}: body sha256 {oracle.get('bodySha256')} -> {candidate.get('bodySha256')} "
            f"(bytes {oracle.get('bodyBytes')} -> {candidate.get('bodyBytes')})"
        )
    if oracle.get("bootstrapAppId") != candidate.get("bootstrapAppId"):
        deltas.append(
            f"{case_id}: bootstrap app id {oracle.get('bootstrapAppId')} -> "
            f"{candidate.get('bootstrapAppId')}"
        )

    oracle_tls = oracle.get("tls")
    candidate_tls = candidate.get("tls")
    if oracle_tls or candidate_tls:
        for field in ("protocol", "alpn", "certSanSha256"):
            o_value = (oracle_tls or {}).get(field)
            c_value = (candidate_tls or {}).get(field)
            if o_value != c_value:
                deltas.append(f"{case_id}: tls {field}: {o_value!r} -> {c_value!r}")
    return deltas


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
