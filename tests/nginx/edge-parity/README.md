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

## Oracle provenance (v2 re-capture)

The committed oracle carries the `stock-nginx-edge` provenance label and was
re-captured on 2026-11-20 from the retired convenience edge recreated from its
pinned image (`nginx@sha256:332d0bee…`, 1.31.7) with the same bind mounts
(config snippets, certs, `/srv/mall` content) on alternate ports
`:8080`/`:8443` — only the `listen` directives were rewritten, upstreams and
served content are untouched. The re-capture exists because `3faee914`
upgraded the battery (chunk decoding, handoff/trace normalization) after the
original capture, which left the v1 oracle body hashes incomparable with v2
candidate captures for every injected or chunked body. Both sides of the
comparison are therefore recorded by the same battery version.

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

## Deployment channel regression

`deployment-regression.py` verifies the *deployment channels* themselves, on
the edge host, after (re)deployment:

- `--mode installer` — the native installer channel
  (`docs/guides/operator/deb-install.md`): package integrity, systemd units,
  the production edge takeover of host :80/:443, ACME HTTP-01 webroot,
  HTTP→HTTPS redirect, TLS/ALPN h2 semantics, edge domain certificate
  identity, gateway behind the edge, direct gateway health, edge config
  self-validation, and the host-level absence of stock nginx
  (SDKWORK_WEBSERVER_SPEC §0.1).
- `--mode full-edge` — the multi-vhost development edge: the full battery
  capture compared against the committed oracle with zero behavioural
  differences, plus derived assertions (drop semantics, identity banner,
  bootstrap handoff presence).

```bash
# Installer channel (after `bin/apps-deploy.sh server` / apt install)
python3 tests/nginx/edge-parity/deployment-regression.py --mode installer \
    --environment production --domain server.sdkwork.com

# Container channel (after `bin/docker-deploy.sh install --environment development`)
python3 tests/nginx/edge-parity/deployment-regression.py --mode full-edge
```

`run-remote-regression.mjs` runs the same regression from a workstation over
SSH against the pulled checkout on the edge host and propagates the remote
exit code:

```bash
node tests/nginx/edge-parity/run-remote-regression.mjs \
    --host api-dev.birdcoder.com --mode full-edge \
    [--json-out target/evidence/full-edge-verdict.json]
```

