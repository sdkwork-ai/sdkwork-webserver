# Production Readiness Evidence

Operational evidence for the standalone `sdkwork-webserver` edge. Every entry names the command that
produced it and whether the evidence is a **gate** (CI-enforced) or a **measurement** (run locally /
on the target host, recorded here with its environment).

Measurements are not claims of production capacity: dev-profile numbers on a developer workstation
are lower than a release build on server hardware. Use them as the regression baseline and re-run the
same commands on the target host before a launch decision.

## 1. Capacity and latency

Harness: `crates/sdkwork-api-webserver-standalone-gateway/tests/capacity_smoke.rs` (ignored by
default). It starts the real data plane with one `respond` resource and one catch-all route, warms
the pool, then drives keep-alive HTTP/1.1 requests from 32 concurrent workers. It fails on any
non-200 response and on p99 ≥ 1 s.

```bash
pnpm test:capacity

# 5-second sustained run
SDKWORK_CAPACITY_DURATION_SECONDS=5 pnpm test:capacity

# 24-hour soak (operator run; same assertions, throughput recorded)
SDKWORK_CAPACITY_DURATION_SECONDS=86400 pnpm test:capacity

# fixed request count
SDKWORK_CAPACITY_REQUESTS=100000 pnpm test:capacity
```

Recorded on 2026-10-21 (Windows, dev profile, 4 runtime workers, local loopback):

| Run | Requests | Failures | Throughput | p50 | p95 | p99 | max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| fixed 32,000 | 32,000 | 0 | 11,251 req/s | 2.34 ms | 5.32 ms | 10.95 ms | 61.25 ms |
| 5 s duration | 153,772 | 0 | 30,743 req/s | 1.01 ms | 1.55 ms | 1.86 ms | 5.59 ms |
| fixed 32,000 (warm rerun) | 32,000 | 0 | 33,299 req/s | 0.92 ms | 1.44 ms | 1.79 ms | 4.40 ms |
| fixed 32,000 (after 2026-10-21 dependency refresh) | 32,000 | 0 | 18,475 req/s | 1.32 ms | 3.83 ms | 7.49 ms | 38.85 ms |
| 10-minute sustained soak | 24,549,146 | 0 | 40,907 req/s | 0.75 ms | 1.17 ms | 1.46 ms | 112.61 ms |

Not yet proven here:

- **100,000 idle keep-alive connections** (PRD §8.1). The harness holds and verifies a configurable
  count (`pnpm test:connections`); the 2026-10-21 workstation runs held **5,000**, **10,000**, and
  **15,000** idle sockets while a fresh request still returned 200. Reaching 100k needs the target
  host's file-descriptor/ephemeral-port budget and, for same-host loopback, multiple source
  addresses; document the host tuning with the release profile.
- **24-hour soak**: the command exists and the 5-second path is exercised; the 24 h run must be
  scheduled on the target host and its output attached to the release record.
- **10,000 routes compiled in ≤ 1 s**: verified by `pnpm test:route-compile` — the real config
  load + validation + compile path built 10,000 exact routes in **33 ms** after the route-overlap
  index replaced the O(N²) scan (previously 349–438 ms). Run with
  `SDKWORK_ROUTE_COMPILE_MAX_MS=1000` to make the budget an assertion.

Route-compile harness:

```bash
pnpm test:route-compile
SDKWORK_ROUTE_COMPILE_MAX_MS=1000 SDKWORK_ROUTE_COMPILE_ROUTES=10000 pnpm test:route-compile
```

Idle-connection harness:

```bash
pnpm test:connections

# target-host attempt; raise the OS limits first
SDKWORK_CAPACITY_CONNECTIONS=100000 pnpm test:connections
```

| Held idle sockets | Connect wall time | Fresh request while held |
| --- | --- | --- |
| 5,000 | 1.8 s | 200 |
| 10,000 | 8.4 s | 200 |
| 15,000 | 27.6–38.7 s | 200 |

The workstation ceiling is the kernel, not the data plane: rotating source addresses across
`127.0.0.0/8` still failed at **16,101** accepted sockets with Windows `WSAENOBUFS` (os error
10055), i.e. the loopback buffer/control-block budget. Reaching 100k needs the target host's
`netsh int ipv4 set dynamicport` range / Linux `net.ipv4.ip_local_port_range`, descriptor limits,
and (for one host) multiple source addresses; record that tuning with the release profile.

## 2. Database lifecycle and repository correctness

Both are mandatory CI gates (`pnpm test:postgres:required`, wired into the release workflow before
`pnpm verify`).

| Gate | Command | Result |
| --- | --- | --- |
| baseline + ordered migrations + seed + drift | `cargo test -p sdkwork-webserver-database-host --test postgres_lifecycle -- --ignored --exact` | passed on PostgreSQL 18.3 (disposable schema) |
| repository transactions / tenant isolation / idempotency / keyset / rollback / certificate lifecycle | `cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test repository_parity -- --ignored --exact` | passed on PostgreSQL 18.3 |
| Snowflake node lease allocation (two replicas receive different node ids) | `cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test node_registry_postgres -- --ignored --exact` | passed |

## 3. Protocol and standards gates

| Gate | Command |
| --- | --- |
| Rust workspace compile | `cargo check --workspace --all-targets` |
| Rust formatting | `cargo fmt -- --check` |
| API operation patterns / envelope / pagination | `check-api-operation-patterns.mjs`, `check-api-response-envelope.mjs`, `check-pagination.mjs` |
| Database framework contract | `check-database-framework-standard.mjs` |
| Generated SDKs up to date (14 targets) | `node tools/generate_web_sdks.mjs --check` |
| Repository docs standard | `check-repository-docs-standard.mjs` |
| Repository contract suite | `node --test tests/contract/*.contract.test.mjs` |

## 4. Data plane behaviour

- `cargo test -p sdkwork-api-webserver-standalone-gateway --lib` — 347 unit tests (admission, cache,
  timeouts, TLS material, shutdown, usage metering).
- `tests/data_plane_integration.rs` — 59 end-to-end cases (HTTP/1 semantics, cache, ACME challenge,
  admission, shutdown).
- `cargo test --workspace --no-fail-fast` — 125 test targets, zero failures (2026-10-21, after
  making the nginx include/webroot tests spell paths portably on Windows).
- `tests/tunnel_relay.rs` — 8 tunnel relay cases.
- `tests/stream_proxy.rs` / `tests/http1_connection_semantics.rs` / `tests/sub_filter.rs` — TLS
  termination, pipelining, half-close, substitution.

### 4.1 Nginx compatibility evidence (`http-core-v1`)

The compatibility profile, reference build and known differences are assessed in
`docs/reports/2026-10-30-nginx-commercial-compatibility-assessment.md`; the directive-level authority
is `specs/nginx-directive-support.catalog.json` rendered as
`docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`. All of it is gated:

| Gate | Command | 2026-10-30 result |
| --- | --- | --- |
| Aggregate release gate (wired into `_sdkwork:verify` as `check:nginx-compat`) | `node tools/run-nginx-compat-gate.mjs` | PASS — static gates, in-process corpus and all three differential slices executed |
| In-process full-surface behavioural corpus | `python3 tests/nginx/full-surface/probe.py` | 76/76 |
| Conditional/range/cache differential vs nginx 1.29.6 | `node tools/run-nginx-differential.mjs` | 50/50, 0 unexpected (44 documented allowances) |
| Broad HTTP differential vs nginx 1.29.6 | `node tools/run-nginx-differential-http.mjs` | 259/259, 0 unexpected |
| Upstream-protocol differential vs nginx 1.29.6 (FastCGI/uWSGI/SCGI/memcached/gRPC) | `node tools/run-nginx-differential-protocols.mjs` | 49/49 byte-identical (bind/keep-conn/unix + uWSGI TLS/mTLS included) |
| Stream differential vs nginx 1.29.6 (limit_conn/dry-run/ACL/return/set/map/SNI) | `node tools/run-nginx-differential-stream.mjs` | 9/9 |
| Real production config acceptance (api.birdcoder.com snapshot, 33 files) | `node tools/check-nginx-real-world.mjs` | 41 vhosts / 4 listeners / 150 resources / 88 upstreams / 0 skipped |
| Mail proxy differential vs nginx 1.29.6 (auth_http + POP3/IMAP/SMTP + STARTTLS) | `node tools/run-nginx-differential-mail.mjs` | 9/9 byte/transcript-identical |
| Virtual-host selection and named locations vs nginx 1.29.6 | `node tools/run-nginx-differential-host.mjs` | 39/39, 0 unexpected (3 documented allowances) |
| Directive catalog drift (inventory ↔ catalog ↔ implementation ↔ CLI ↔ docs) | `node tools/check-nginx-directive-matrix.mjs` | 12/12 |
| Migration CLI consumes the catalog | `node --test tools/import-nginx-conf.test.mjs` | 14/14 |
| Directive-by-directive audit ledger (744 rows) current + evidence verified | `node tools/gen-nginx-directive-audit.mjs --check --verify-evidence` | PASS |

The differential oracles are committed and pinned to nginx 1.29.6, so CI compares without a real
nginx binary; `SDKWORK_REQUIRE_NGINX_EVIDENCE=1` (set by the aggregate gate) turns any "this machine
cannot execute the evidence" skip into a failure. The directive catalog now classifies 246 directives
executable, 170 accepted-but-unenforced (each with a located diagnostic and a migration action), 334
fail-closed and 64 excluded out of the 744 nginx 1.29.6 directives; a generated 744-row audit ledger
records one decision and rationale per directive (116 enforced / 143 runtime-owned / 17 security-difference / 29 behavior-gap / 229 out-of-profile / 210 fail-closed). Remaining catalogued gaps (`tls.ocsp-stapling`,
`http.websocket.rfc8441`, `ops.reload.listener-handoff`, nested locations, chunked_transfer_encoding,
charset/autoindex families, HTTP/2 tuning, stream module gaps) are listed with commercial impact and
priorities in the assessment report and are never claimed as supported.


## 5. Browser clients

- PC: `pnpm typecheck` and `pnpm test` — 45 files / 445 tests. The host shell must not export
  `SDKWORK_ENVIRONMENT`, `SDKWORK_DEPLOYMENT_PROFILE`, or `SDKWORK_PROFILE_ID`; a production identity
  in the caller's environment is correctly rejected by the Vite topology check.
- `pnpm lint` — ESLint 10 + typescript-eslint + react-hooks over the authored PC/H5 sources, zero
  errors and zero warnings (`--max-warnings 0`), wired into `_sdkwork:check`.
- H5: `pnpm typecheck` / `pnpm test` (application shell and screen contracts).

## 6. Dependency currency

`Cargo.lock` is refreshed with `cargo update` (semver-compatible) and the full workspace plus the
capacity harness are re-run after every refresh. The 2026-10-21 refresh moved tokio to 1.54,
rustls/rustls-webpki to their current patch releases, serde_json to 1.0.154, and uuid to 1.28;
`cargo check --workspace --all-targets`, the gateway/service/tunnel suites, and both PostgreSQL
gates passed afterwards. `pnpm outdated -r` was also reviewed: the mini-program build's `esbuild`
moved to 0.28.2 (tests 32/32, build succeeded), `monaco-editor` moved to 0.57.0 (PC production build
succeeded), and react/react-dom were aligned at 19.3.1 with matching 19.3 type packages (PC 445/445,
H5 16/16, both production builds succeeded). The test toolchain moved to vitest 5, vite 8.3.4, and
jsdom 30 with the same suites green; `@testing-library/react 16.3.3` and `lucide-react 1.60` are also
current. TypeScript stays on 6.0.3 because typescript-eslint 8.72 rejects TS 7 at load time (the root
devDependency pins it for the lint peer); it is the only directly-owned frontend dependency still
behind latest. Dependencies behind latest due to semver-major constraints are listed in the audit
report §34/§42/§44/§50 and need a dedicated upgrade review.

## 7. Supply chain and workflows

- The packaging entrypoint is a thin reusable-workflow call pinned by full commit SHA; no local
  checkout/step runs in this repository, satisfying the GITHUB_WORKFLOW_SPEC thin-entry rule.
- `.github/dependabot.yml` keeps cargo, pnpm, and GitHub Actions current on a weekly schedule.
- Release artifacts are produced with the module `bin/` packagers and a sidecar `.sha256`; SBOM and
  signing remain owned by the reusable packaging workflow (verify its attestation on the release).

## 8. Known gaps

They are tracked in `docs/engineering/reviews/REVIEW-20261020-webserver-full-audit.md`; the
commercial blockers are billing/entitlement, SLO/alerting, support bundle, supply-chain signing, and
the 100k/24h capacity evidence above.
