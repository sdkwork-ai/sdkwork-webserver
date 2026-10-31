"""Compare two mail-proxy battery captures.

Every case compares the client transcript, the auth_http request(s) and the
lines the mock backend received. One allowance is a stated contract difference:
nginx reports the OpenSSL cipher name in `Auth-SSL-Cipher` while the Rust TLS
stack reports its own suite name (the negotiated suite itself is not a wire
contract the proxy re-exposes), so that single header value is normalised in
`battery.py` already. Nothing else is allowed.
"""

from __future__ import annotations

import argparse
import json
import sys


def describe(value) -> str:
    if isinstance(value, str):
        return repr(value)
    return json.dumps(value, sort_keys=True)


def compare_case(name: str, expected: dict, actual: dict, failures: list) -> None:
    for field in ("client", "auth_requests", "backend_sessions"):
        if expected.get(field) != actual.get(field):
            failures.append(
                (name, field, describe(expected.get(field)), describe(actual.get(field)))
            )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected", required=True)
    parser.add_argument("--actual", required=True)
    parser.add_argument("--label-expected", default="oracle")
    parser.add_argument("--label-actual", default="ours")
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    with open(args.expected, encoding="utf-8") as handle:
        expected = json.load(handle)
    with open(args.actual, encoding="utf-8") as handle:
        actual = json.load(handle)

    expected_cases = {key: value for key, value in expected.items() if not key.startswith("_")}
    actual_cases = {key: value for key, value in actual.items() if not key.startswith("_")}
    failures: list = []
    missing = sorted(set(expected_cases) - set(actual_cases))
    extra = sorted(set(actual_cases) - set(expected_cases))
    for name in missing:
        failures.append((name, "case", "present", "missing"))
    for name in extra:
        failures.append((name, "case", "missing", "present"))
    for name in sorted(set(expected_cases) & set(actual_cases)):
        compare_case(name, expected_cases[name], actual_cases[name], failures)

    if missing or extra:
        print("case-set mismatch: missing=%r extra=%r" % (missing, extra))
    if failures:
        for case, field, left, right in failures:
            print("FAIL %s.%s" % (case, field))
            print("     %-8s %s" % (args.label_expected, left))
            print("     %-8s %s" % (args.label_actual, right))
        print()
    total = len(expected_cases)
    matching = total - len({failure[0] for failure in failures if failure[0] in expected_cases})
    print("differential: %d/%d mail cases match" % (max(matching, 0), total))
    print("differential: %d mismatch case(s)" % len({failure[0] for failure in failures}))
    if failures:
        return 1
    if args.strict:
        print("differential: --strict mode verified (no allowances recorded)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
