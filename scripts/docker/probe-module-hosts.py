#!/usr/bin/env python3
"""Probe module hostnames on the local import data-plane port.

Defaults to development nginx sidecars and host port 13808.
Pass --environment test|production (and matching --port) for other stacks.
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

ROOT = os.environ.get("SDKWORK_SPACE_CHECKOUT", "/opt/deploy/sdkwork-space")
DEFAULT_PORTS = {
    "development": "13808",
    "test": "18898",
    "production": "18098",
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--environment",
        default="development",
        choices=sorted(DEFAULT_PORTS),
    )
    parser.add_argument("--port", default=None)
    parser.add_argument("--timeout", default="3")
    args = parser.parse_args()
    port = args.port or os.environ.get(
        "SDKWORK_IMPORT_PORT", DEFAULT_PORTS[args.environment]
    )
    conf_name = f"nginx.standalone.{args.environment}.conf"

    mods = sorted(
        d
        for d in os.listdir(ROOT)
        if d.startswith("sdkwork-")
        and os.path.isdir(os.path.join(ROOT, d, "deployments/webserver"))
        and d != "sdkwork-webserver"
    )
    buckets: dict[str, list[tuple[str, str, str, str]]] = {
        "real": [],
        "placeholder": [],
        "missing": [],
        "other": [],
    }
    for mod in mods:
        conf = os.path.join(ROOT, mod, "deployments/webserver", conf_name)
        if not os.path.isfile(conf):
            continue
        text = open(conf, encoding="utf-8", errors="replace").read()
        match = re.search(r"server_name\s+([^;]+);", text)
        if not match:
            continue
        # Prefer the first *.sdkwork.com host for local probes.
        hosts = match.group(1).split()
        host = next((h for h in hosts if h.endswith(".sdkwork.com")), hosts[0])
        proc = subprocess.run(
            [
                "curl",
                "--noproxy",
                "*",
                "-sS",
                "-o",
                "/tmp/sdkwork-probe.html",
                "-w",
                "%{http_code}",
                "--max-time",
                str(args.timeout),
                "-H",
                f"Host: {host}",
                f"http://127.0.0.1:{port}/",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        code = proc.stdout.strip()
        body = open("/tmp/sdkwork-probe.html", "rb").read()[:800].decode(
            "utf-8", "replace"
        )
        title_match = re.search(r"<title>([^<]+)", body)
        title = (
            title_match.group(1)
            if title_match
            else body[:80].replace("\n", " ")
        )
        if code == "200" and "placeholder" in body.lower():
            kind = "placeholder"
        elif code == "200":
            kind = "real"
        elif "not available" in body.lower() or code == "404":
            kind = "missing"
        else:
            kind = "other"
        buckets[kind].append((mod, host, code, title[:60]))
        print(f"{code:4} {kind:12} {mod:28} {host:36} {title[:50]}")
    print("---")
    for key, rows in buckets.items():
        print(f"{key}: {len(rows)}")
        if key != "real" and rows:
            print("  " + ", ".join(row[0] for row in rows))
    return 0


if __name__ == "__main__":
    sys.exit(main())
