# Edge parity battery: replacing the stock-nginx development edge

The api-dev host runs a development edge on host ports 80/443 that serves
three virtual-host families: `api-dev.<brand>` (reverse proxy to the
development gateway), `mall-dev.<brand>` (adaptive PC/H5 static delivery) and
`platform-dev.<brand>` (the cloudrouter development preview). Per
`SDKWORK_WEBSERVER_SPEC.md` §0.1 the webserver itself is the only supported
public edge, so this fixture pins the exact behaviour the runtime must
reproduce when the stock-nginx convenience container is retired.

## Provenance discipline

The battery records reality; it never asserts expectations. The committed
`nginx-edge-oracle.json` is captured from the live stock-nginx edge **before**
the cutover (provenance label records which implementation answered). After
the webserver takes over host 80/443, the same battery is re-captured and
`compare.py` turns the two captures into a verdict. Field-by-field parity is
required for: status, response header map, body sha256, adaptive surface,
bootstrap-token application identity, TLS protocol/ALPN and certificate SAN
fingerprint. Connection-drop (`444`) semantics are assertions, not
allowances.

The allowance list in `compare.py` mirrors the audited allowances of
`tests/nginx/differential-http/compare.py` (implementation banner, static
`Cache-Control` baseline, adaptive `Accept-CH`/`Vary`, secure-response
`nosniff`); each entry cites its normative authority and `--strict` rejects
them so the list stays auditable.

## Capture (on the edge host)

```bash
# Oracle: the live stock-nginx edge (before cutover)
python3 tests/nginx/edge-parity/battery.py \
    --http 127.0.0.1:80 --tls 127.0.0.1:443 \
    --label stock-nginx-edge \
    --out tests/nginx/edge-parity/nginx-edge-oracle.json

# Candidate: the webserver edge (after cutover, same ports)
python3 tests/nginx/edge-parity/battery.py \
    --http 127.0.0.1:80 --tls 127.0.0.1:443 \
    --label sdkwork-webserver-edge \
    --out /tmp/webserver-edge.json
```

## Compare

```bash
python3 tests/nginx/edge-parity/compare.py \
    --oracle tests/nginx/edge-parity/nginx-edge-oracle.json \
    --candidate /tmp/webserver-edge.json
```

Exit 0 means aligned (only allowlisted deltas). Exit 1 lists behavioural
differences that must be fixed forward or proven to be stated-contract
differences and added to the audited allowlist with their authority.

## Pre-cutover sanity run

The same battery can target the webserver's development instance (host ports
18898/28430) before the cutover to enumerate the gaps while nginx is still
live:

```bash
python3 tests/nginx/edge-parity/battery.py \
    --http 127.0.0.1:18898 --tls 127.0.0.1:28430 \
    --label pre-cutover-webserver \
    --out /tmp/pre-cutover.json
```
