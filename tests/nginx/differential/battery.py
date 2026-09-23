"""Canonical conditional-request / cache-policy battery.

One script, two endpoints: run it against real nginx and against the Rust data
plane, then diff the two JSON documents. Only the observable response contract
is recorded — status, the cache validators, and whether the representation was
sent — so a difference is always a real behavioural difference and never a
header-formatting artefact.

    python3 battery.py --port 20990 --out nginx.json    # oracle
    python  battery.py --port 20991 --out ours.json     # Rust data plane

Every conditional header is derived from the validator the endpoint itself just
returned, so neither side needs a hard-coded tag. Both sides read the same
fixture document, whose mtime is pinned, so `ETag` (strong, `<mtime>-<size>`) and
`Last-Modified` are byte-identical and *are* compared exactly: a divergence in
tag strength or timestamp is a real defect, not an artefact.

`Date` is recorded but not compared — the two runs happen seconds apart, and
compare.py normalises every freshness value against the response's own `Date`.
Identity metadata lives under the `_`-prefixed keys (`_server`, `_port`,
`_etag`, `_last_modified`) so a comparison can prove which implementation
answered on each port.
"""

from __future__ import annotations

import argparse
import datetime
import json
import socket
import sys

HOST_HEADER = "diff.example.com"

# Header names whose presence/value form the observable contract.
TRACKED = (
    "cache-control",
    "date",
    "expires",
    "etag",
    "last-modified",
    "accept-ranges",
    "content-range",
    "content-type",
    "content-length",
    "vary",
)


def http_date(moment: datetime.datetime) -> str:
    return moment.astimezone(datetime.timezone.utc).strftime(
        "%a, %d %b %Y %H:%M:%S GMT"
    )


def parse_http_date(value: str) -> datetime.datetime:
    return datetime.datetime.strptime(value, "%a, %d %b %Y %H:%M:%S GMT").replace(
        tzinfo=datetime.timezone.utc
    )


def request(port: int, path: str, headers):
    lines = ["GET %s HTTP/1.1" % path, "Host: %s" % HOST_HEADER, "Connection: close"]
    for name, value in headers or []:
        lines.append("%s: %s" % (name, value))
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
    return status, parsed, len(body)


def canonical(status, headers, body_len):
    record = {"status": status, "body_len": body_len}
    for name in TRACKED:
        record[name] = headers.get(name)
    return record


def case(port, label, path, headers=None, results=None):
    status, headers, body_len = request(port, path, headers)
    results[label] = canonical(status, headers, body_len)
    return status, headers, body_len


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    port = args.port

    results = {}

    _, base_headers, _ = request(port, "/cond/big.txt", [])
    # Identity witness. WSL localhost forwarding can silently route a Windows
    # connection into the nginx running inside WSL, which would make the two
    # batteries compare nginx against itself. Every comparison must therefore
    # prove which implementation answered.
    identity = base_headers.get("server")
    if identity and identity.lower().startswith("nginx/"):
        print(
            "WARNING: endpoint %d answered as %r; this is real nginx, not the "
            "Rust data plane" % (port, identity),
            file=sys.stderr,
        )
    etag = base_headers.get("etag")
    last_modified = base_headers.get("last-modified")
    if etag is None or last_modified is None:
        print("baseline is missing a validator: %r" % (base_headers,), file=sys.stderr)
        return 2
    lm = parse_http_date(last_modified)
    older = http_date(lm - datetime.timedelta(hours=1))
    newer = http_date(lm + datetime.timedelta(hours=1))

    ifrm = [("If-Modified-Since", older)]
    case(port, "cond.baseline", "/cond/big.txt", None, results)
    case(port, "cond.inm-match", "/cond/big.txt", [("If-None-Match", etag)], results)
    case(port, "cond.inm-star", "/cond/big.txt", [("If-None-Match", "*")], results)
    case(port, "cond.inm-miss", "/cond/big.txt", [('If-None-Match', '"nope"')], results)
    case(port, "cond.ims-equal", "/cond/big.txt", [("If-Modified-Since", last_modified)], results)
    case(port, "cond.ims-older", "/cond/big.txt", [("If-Modified-Since", older)], results)
    case(port, "cond.ims-newer", "/cond/big.txt", [("If-Modified-Since", newer)], results)
    case(
        port,
        "cond.inm-match+ims-older",
        "/cond/big.txt",
        [("If-None-Match", etag), ("If-Modified-Since", older)],
        results,
    )
    case(
        port,
        "cond.inm-match+ims-equal",
        "/cond/big.txt",
        [("If-None-Match", etag), ("If-Modified-Since", last_modified)],
        results,
    )
    case(
        port,
        "cond.inm-miss+ims-equal",
        "/cond/big.txt",
        [('If-None-Match', '"nope"'), ("If-Modified-Since", last_modified)],
        results,
    )
    case(port, "cond.if-match-hit", "/cond/big.txt", [("If-Match", etag)], results)
    case(port, "cond.if-match-miss", "/cond/big.txt", [('If-Match', '"nope"')], results)
    case(port, "cond.if-match-star", "/cond/big.txt", [("If-Match", "*")], results)
    case(port, "cond.ius-newer", "/cond/big.txt", [("If-Unmodified-Since", newer)], results)
    case(port, "cond.ius-older", "/cond/big.txt", [("If-Unmodified-Since", older)], results)
    case(port, "cond.range", "/cond/big.txt", [("Range", "bytes=0-3")], results)
    case(
        port,
        "cond.range-unsatisfiable",
        "/cond/big.txt",
        [("Range", "bytes=99999-")],
        results,
    )
    case(
        port,
        "cond.range+if-range-date-equal",
        "/cond/big.txt",
        [("Range", "bytes=0-3"), ("If-Range", last_modified)],
        results,
    )
    case(
        port,
        "cond.range+if-range-date-older",
        "/cond/big.txt",
        [("Range", "bytes=0-3"), ("If-Range", older)],
        results,
    )
    case(port, "cond.ius-equal", "/cond/big.txt", [("If-Unmodified-Since", last_modified)], results)
    case(
        port,
        "cond.range+inm-match",
        "/cond/big.txt",
        [("Range", "bytes=0-3"), ("If-None-Match", etag)],
        results,
    )
    case(
        port,
        "cond.range+ims-equal",
        "/cond/big.txt",
        [("Range", "bytes=0-3"), ("If-Modified-Since", last_modified)],
        results,
    )
    case(
        port,
        "cond.range+if-range-match",
        "/cond/big.txt",
        [("Range", "bytes=0-3"), ("If-Range", etag)],
        results,
    )
    case(
        port,
        "cond.range+if-range-miss",
        "/cond/big.txt",
        [("Range", "bytes=0-3"), ('If-Range', '"nope"')],
        results,
    )

    case(port, "noetag.baseline", "/noetag/big.txt", None, results)
    case(port, "noetag.inm-star", "/noetag/big.txt", [("If-None-Match", "*")], results)
    case(port, "noetag.inm-miss", "/noetag/big.txt", [('If-None-Match', '"nope"')], results)
    case(port, "noetag.if-match", "/noetag/big.txt", [('If-Match', '"nope"')], results)

    case(port, "ims-off.newer", "/ims-off/big.txt", [("If-Modified-Since", newer)], results)
    case(port, "ims-off.older", "/ims-off/big.txt", [("If-Modified-Since", older)], results)
    case(port, "ims-off.equal", "/ims-off/big.txt", [("If-Modified-Since", last_modified)], results)

    case(port, "ims-before.equal", "/ims-before/big.txt", [("If-Modified-Since", last_modified)], results)
    case(port, "ims-before.newer", "/ims-before/big.txt", [("If-Modified-Since", newer)], results)
    case(port, "ims-before.older", "/ims-before/big.txt", [("If-Modified-Since", older)], results)

    for name in (
        "exp-off",
        "exp-1h",
        "exp-max",
        "exp-epoch",
        "exp-neg",
        "exp-daily",
        "exp-mod",
        "exp-zero",
    ):
        case(port, name, "/%s/big.txt" % name, None, results)
        # The policy must also apply to a 304, and must not turn a 304 into a
        # miss: `If-None-Match` is evaluated against the served representation.
        case(port, name + "+inm-star", "/%s/big.txt" % name, [("If-None-Match", "*")], results)

    document = {
        "_server": identity,
        "_port": port,
        "_etag": etag,
        "_last_modified": last_modified,
    }
    document.update(dict(sorted(results.items())))
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print("wrote %s (%d cases)" % (args.out, len(results)))
    print("identity: server=%r" % (identity,))
    print("validators: etag=%r last-modified=%r" % (etag, last_modified))
    return 0


if __name__ == "__main__":
    sys.exit(main())
