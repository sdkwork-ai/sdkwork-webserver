#!/usr/bin/env python3
"""Compare two stream-battery captures (exact connection-level equality)."""

from __future__ import annotations

import argparse
import json
import sys


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected", required=True)
    parser.add_argument("--actual", required=True)
    args = parser.parse_args()
    with open(args.expected, encoding="utf-8") as handle:
        expected = json.load(handle)["cases"]
    with open(args.actual, encoding="utf-8") as handle:
        actual = json.load(handle)["cases"]

    problems: list[str] = []
    for name in sorted(set(expected) | set(actual)):
        if name not in actual:
            problems.append(f"{name}: missing from the runtime capture")
            continue
        if name not in expected:
            problems.append(f"{name}: extra case not in the oracle capture")
            continue
        if expected[name] != actual[name]:
            problems.append(
                f"{name}: nginx {json.dumps(expected[name])} != runtime {json.dumps(actual[name])}"
            )
    if problems:
        for problem in problems:
            print(f"FAIL {problem}")
        print(f"differential-stream: {len(problems)} unexpected difference(s)")
        sys.exit(1)
    print(f"differential-stream: {len(expected)} case(s) match real nginx")
    print(f"differential-stream: 0 unexpected difference(s)")


if __name__ == "__main__":
    main()
