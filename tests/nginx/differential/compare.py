"""Compare two battery captures and report only behavioural differences.

The two endpoints are separate processes measured seconds apart, so absolute
times cannot be compared directly. Every freshness value is therefore
normalised against the `Date` header of the *same response*:

    expires_offset = Expires - Date        (stable for every `expires` mode)
    max_age        = N in `Cache-Control: max-age=N`

A clock-skew tolerance absorbs the few seconds between the two runs, while any
semantic error (a wrong mode, a lost header, a different status) is off by far
more than that and still fails. One mode needs the elapsed gap as well: `expires
@<time>` (DAILY) publishes the seconds remaining until the next occurrence, so it
counts down in real time, and the elapsed-shifted value is accepted for it.

Differences that are deliberate, and that the deployment specifications require,
are listed in `KNOWN_DIFFERENCES` with their rationale. They are reported as
`known`, never silently ignored, and `--strict` treats them as failures so the
allowlist can be audited on its own.

    python compare.py --expected nginx.json --actual ours.json
    python compare.py --expected nginx.json --actual ours.json --strict

Exit status is 0 when every case matches or is a known difference, 1 otherwise.
"""

from __future__ import annotations

import argparse
import datetime
import fnmatch
import json
import re
import sys

# The two batteries run within a minute of each other; anything that moves by
# more than this is a semantic difference, not clock skew.
TOLERANCE_SECONDS = 120

EXACT_FIELDS = (
    "status",
    "body_len",
    "etag",
    "last-modified",
    "accept-ranges",
    "content-range",
    "content-type",
    "content-length",
    "vary",
)

# `date` is not compared: the two endpoints are separate processes measured
# seconds apart, so it only ever differs by the run time. It is still recorded,
# because every freshness value is normalised against it.

MAX_AGE = re.compile(r"^max-age=(\d+)$")

# nginx renders its built-in error page for a bare precondition or range
# failure. Its reason phrase is part of the body, so the page width is a
# per-status fact: "412 Precondition Failed" is 173 bytes while "416 Requested
# Range Not Satisfiable" is 197. The lengths are pinned per case to what nginx
# 1.29.6 answered, so a change on either side is reported rather than absorbed
# by a tolerant rule. Only the body differs -- status, Content-Range and every
# cache header still match exactly.
ERROR_PAGE_BODY_BYTES = {
    "cond.if-match-miss": 173,
    "cond.ius-older": 173,
    "noetag.if-match": 173,
    "cond.range-unsatisfiable": 197,
}

ERROR_PAGE_REASON = (
    "nginx renders its built-in error page; the runtime answers the same status "
    "with an empty body because nginx's version banner is part of the page. "
    "Status codes, Content-Range and every cache header still match exactly."
)

KNOWN_DIFFERENCES = (
    {
        "cases": ("*",),
        "field": "cache-control",
        "expected": None,
        "actual": "public, no-cache",
        "reason": (
            "deployment freshness policy (ENVIRONMENT_SPEC section 13): a static "
            "response with no `expires` declares `public, no-cache` so shared "
            "caches revalidate instead of caching heuristically. nginx has no "
            "equivalent default. `expires` replaces the value whenever it is "
            "configured, which is why every exp-* case matches byte for byte."
        ),
    },
    # The error-page body is checked by `error_page_difference` below, which
    # pins the measured length per case rather than a single width.
)


def http_date(value):
    if not value:
        return None
    try:
        return datetime.datetime.strptime(
            value, "%a, %d %b %Y %H:%M:%S GMT"
        ).replace(tzinfo=datetime.timezone.utc)
    except ValueError:
        return None


def freshness_seconds(record, field):
    """`field` expressed as seconds relative to the response's own Date."""
    value = record.get(field)
    if value is None:
        return None
    match = MAX_AGE.match(value)
    if match:
        return int(match.group(1))
    moment = http_date(value)
    date = http_date(record.get("date"))
    if moment is None or date is None:
        return None
    return int((moment - date).total_seconds())


def error_page_difference(case, field, expected, actual):
    """The one documented error-page allowance, matched against pinned widths."""
    length = ERROR_PAGE_BODY_BYTES.get(case)
    if length is None:
        return None
    if field == "body_len" and expected == length and actual == 0:
        return ERROR_PAGE_REASON
    if field == "content-length" and expected == str(length) and actual == "0":
        return ERROR_PAGE_REASON
    if field == "content-type" and expected == "text/html" and actual is None:
        return ERROR_PAGE_REASON
    return None


def known_reason(case, field, expected, actual):
    allowed = error_page_difference(case, field, expected, actual)
    if allowed is not None:
        return allowed
    for entry in KNOWN_DIFFERENCES:
        if not any(fnmatch.fnmatch(case, pattern) for pattern in entry["cases"]):
            continue
        if entry["field"] != field:
            continue
        if entry["expected"] == expected and entry["actual"] == actual:
            return entry["reason"]
    return None


def record(failures, known, case, field, expected, actual):
    reason = known_reason(case, field, expected, actual)
    if reason is None:
        failures.append((case, field, expected, actual))
    else:
        known.append((case, field, expected, actual, reason))


def compare_case(case, expected, actual, failures, known):
    for field in EXACT_FIELDS:
        left = expected.get(field)
        right = actual.get(field)
        if left != right:
            record(failures, known, case, field, left, right)

    # The two captures are separate processes, so a countdown value moves between
    # them by the gap between their `Date` headers.
    expected_date = http_date(expected.get("date"))
    actual_date = http_date(actual.get("date"))
    elapsed = (
        (actual_date - expected_date).total_seconds()
        if expected_date is not None and actual_date is not None
        else 0.0
    )

    for field in ("cache-control", "expires"):
        left = expected.get(field)
        right = actual.get(field)
        if left == right:
            continue
        left_age = freshness_seconds(expected, field)
        right_age = freshness_seconds(actual, field)
        if left_age is not None and right_age is not None:
            # A fixed delta (`1h`, `modified 1h`, a negative value, `max`, `epoch`)
            # resolves against the response time, so its offset is stable across
            # runs. A DAILY value resolves against the next occurrence, so it also
            # shrinks by `elapsed`.
            if abs(left_age - right_age) <= TOLERANCE_SECONDS:
                continue
            if abs(left_age - right_age - elapsed) <= TOLERANCE_SECONDS:
                continue
        record(failures, known, case, field, left, right)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected", required=True)
    parser.add_argument("--actual", required=True)
    parser.add_argument("--label-expected", default="nginx")
    parser.add_argument("--label-actual", default="ours")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="treat the documented differences as failures, to audit the allowlist",
    )
    args = parser.parse_args()

    expected = json.load(open(args.expected, encoding="utf-8"))
    actual = json.load(open(args.actual, encoding="utf-8"))

    cases = sorted(
        key for key in set(expected) | set(actual) if not key.startswith("_")
    )
    failures = []
    known = []
    for case in cases:
        if case not in expected or case not in actual:
            failures.append((case, "<case>", case in expected, case in actual))
            continue
        compare_case(case, expected[case], actual[case], failures, known)

    print("identity: %s server=%r" % (args.label_expected, expected.get("_server")))
    print("identity: %s server=%r" % (args.label_actual, actual.get("_server")))
    print("validators: %s etag=%r" % (args.label_expected, expected.get("_etag")))
    print("validators: %s etag=%r" % (args.label_actual, actual.get("_etag")))
    print()

    if known:
        by_reason = {}
        for case, field, left, right, reason in known:
            by_reason.setdefault(reason, []).append((case, field, left, right))
        print("known differences (%d):" % len(known))
        for reason, entries in sorted(by_reason.items()):
            print("  %d case(s): %s" % (len(entries), reason))
            for case, field, left, right in sorted(entries)[:2]:
                print("     e.g. %s.%s %r -> %r" % (case, field, left, right))
        print()

    if failures:
        for case, field, left, right in failures:
            print("FAIL %s.%s" % (case, field))
            print("     %-8s %r" % (args.label_expected, left))
            print("     %-8s %r" % (args.label_actual, right))
        print()

    matching = len(cases) - len({failure[0] for failure in failures})
    print("differential: %d/%d cases match" % (matching, len(cases)))
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
