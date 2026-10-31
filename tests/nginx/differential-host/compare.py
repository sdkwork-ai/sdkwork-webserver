"""Compare two host-selection battery captures.

The comparison contract of this slice is virtual-host / named-location
selection: the HTTP status and the response body. Headers are not compared
(the two endpoints run seconds apart; `Date`, `Server`, ordering, and framing
headers are transport metadata, not selection) and `_`-prefixed identity keys
are skipped.

Intentional differences are listed in `KNOWN_DIFFERENCES` with their rationale
and are reported as `known`, never silently ignored; `--strict` treats them as
failures so the allowlist can be audited on its own.

    python3 compare.py --expected nginx-oracle.json --actual ours.json
    python3 compare.py --expected nginx-oracle.json --actual ours.json --strict

Exit status is 0 when every case matches or is a known difference, 1 otherwise.
"""

from __future__ import annotations

import argparse
import json
import sys

# nginx renders its built-in error page (including the pinned version banner)
# for an unmapped 404, while the runtime answers the same status with a fixed,
# bounded text body. Selection is identical; only the diagnostic body differs.
NGINX_BUILTIN_404 = (
    "<html>\r\n"
    "<head><title>404 Not Found</title></head>\r\n"
    "<body>\r\n"
    "<center><h1>404 Not Found</h1></center>\r\n"
    "<hr><center>nginx</center>\r\n"
    "</body>\r\n"
    "</html>"
)

RUNTIME_404_BODY = "static path is not available\n"

# The same built-in-page difference for the named location's `deny all`:
# nginx renders the 403 page, the runtime answers the same status with a fixed
# text body. Status (the access decision) is compared exactly.
NGINX_BUILTIN_403_PREFIX = (
    "<html>\r\n"
    "<head><title>403 Forbidden</title></head>\r\n"
)
RUNTIME_403_BODY = "access denied\n"

KNOWN_DIFFERENCES = (
    {
        "cases": ("static.builtin-404",),
        "field": "body",
        "reason": (
            "nginx's default 404 body is its built-in error page and embeds the "
            "server banner; the runtime returns a fixed bounded text body. The "
            "selection decision is identical (404 from the same virtual host), "
            "and the difference is deliberately not hidden by matching the nginx "
            "page shape."
        ),
    },
)


def known_reason(case, field, expected, actual):
    if (
        case == "named.redirect"
        and field == "body"
        and expected.startswith("<html>\r\n")
        and "302 Found" in expected
        and actual == ""
    ):
        return (
            "nginx renders a built-in redirect page body for `return 302`; the "
            "runtime sends the same 302 with an empty body. The status and "
            "Location header (not compared here) carry the selection contract."
        )
    if (
        case == "named.static-target-access-deny"
        and field == "body"
        and expected.startswith(NGINX_BUILTIN_403_PREFIX)
        and actual == RUNTIME_403_BODY
    ):
        return (
            "nginx renders its built-in 403 page for a named location's "
            "`deny all`; the runtime answers the same 403 with a fixed text "
            "body. The access decision (403) is compared exactly, so the "
            "filter is still proven to run on the internal redirect."
        )
    if (
        case == "static.builtin-404"
        and field == "body"
        and expected.startswith(NGINX_BUILTIN_404.split("<hr>")[0])
        and actual == RUNTIME_404_BODY
    ):
        return KNOWN_DIFFERENCES[0]["reason"]
    for entry in KNOWN_DIFFERENCES:
        if case not in entry["cases"] or field != entry["field"]:
            continue
        if "expected" in entry and entry["expected"] != expected:
            continue
        if "actual" in entry and entry["actual"] != actual:
            continue
        return entry["reason"]
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected", required=True)
    parser.add_argument("--actual", required=True)
    parser.add_argument("--label-expected", default="nginx")
    parser.add_argument("--label-actual", default="ours")
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    expected = json.load(open(args.expected, encoding="utf-8"))
    actual = json.load(open(args.actual, encoding="utf-8"))

    cases = sorted(
        key
        for key in set(expected) | set(actual)
        if not key.startswith("_")
    )
    failures = []
    known = []
    for case in cases:
        if case not in expected or case not in actual:
            failures.append((case, "<case>", case in expected, case in actual))
            continue
        for field in ("status", "body"):
            left = expected[case].get(field)
            right = actual[case].get(field)
            if left == right:
                continue
            reason = known_reason(case, field, left, right)
            if reason is None:
                failures.append((case, field, left, right))
            else:
                known.append((case, field, left, right, reason))

    print("identity: %s server=%r port=%r" % (
        args.label_expected,
        expected.get("_server"),
        expected.get("_port"),
    ))
    print("identity: %s server=%r port=%r" % (
        args.label_actual,
        actual.get("_server"),
        actual.get("_port"),
    ))
    print()

    if known:
        for case, field, _left, _right, reason in known:
            print("known difference: %s.%s" % (case, field))
            print("  %s" % reason)
        print()

    if failures:
        for case, field, left, right in failures:
            print("FAIL %s.%s" % (case, field))
            print("     %-8s %r" % (args.label_expected, left))
            print("     %-8s %r" % (args.label_actual, right))
        print()

    matching = len(cases) - len({failure[0] for failure in failures})
    print("differential: %d/%d host-selection cases match" % (matching, len(cases)))
    print("differential: %d known difference(s)" % len(known))
    print("differential: %d unexpected difference(s)" % len(failures))

    if failures:
        return 1
    if args.strict and known:
        print("differential: --strict rejects the known differences above")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
