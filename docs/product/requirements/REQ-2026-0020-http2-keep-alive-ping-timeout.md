# REQ-2026-0020 HTTP/2 Keep-Alive PING Timeout

```yaml
id: REQ-2026-0020
title: Detect unresponsive HTTP/2 peers with idle PING and ACK timeout
owner: SDKWork maintainers
status: accepted
source: reliability
problem: HTTP/2 connections had finite Stream and Frame abuse budgets but no proactive liveness probe. A peer that stopped reading or responding without closing TCP could retain a connection permit and protocol state indefinitely.
goals:
  - Add finite HTTP/2 Frame-inactivity and PING-ACK timeout controls.
  - Send PING on idle connections, including when no Streams are active.
  - Send GOAWAY with NO_ERROR and close when the corresponding ACK is not received.
  - Keep responsive H2 connections usable across repeated intervals.
  - Keep the policy strictly scoped away from negotiated HTTP/1.
  - Release connection capacity after timeout so a fresh H2 connection can recover.
non_goals:
  - Closing a healthy application-idle H2 connection that continues to ACK PING.
  - Maximum connection age, maximum requests per connection, drain scheduling, or load-balancer lifetime coordination.
  - Cleartext h2c, which remains forbidden by the foundation profile.
  - Replacing Hyper/H2 PING, ACK, GOAWAY, flow-control, or connection state with an application parser.
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example config, and operator docs expose http2KeepAliveIntervalMs and http2KeepAliveTimeoutMs.
  - Interval defaults to 60 seconds and is restricted to 1 second through 1 hour.
  - ACK timeout defaults to 20 seconds, is restricted to 100 ms through 1 minute, and cannot exceed the interval.
  - A raw TLS/H2 client observes PING, withholds ACK, receives GOAWAY(NO_ERROR), and observes connection close.
  - A compliant H2 client ACKs PING and remains usable after the interval.
  - HTTP/1 on the same TLS listener remains usable beyond the H2 interval.
  - A timed-out connection releases a one-connection permit and a new H2 connection succeeds.
  - Watch changes to either field retain the active generation as Restart-only.
  - Real Nginx 1.26.2 evidence records its observed default behavior and the SDKWork difference.
  - Full repository verification passes.
non_functional_requirements:
  security: Minimum interval prevents a configuration-driven PING amplification profile; timeout uses protocol NO_ERROR shutdown.
  performance: Use Hyper 1.10.1 and H2 0.4.15 connection-owned PING/timer state; add no SDKWork task, queue, payload copy, or unbounded label.
  memory: Per-connection state remains finite; 100000-connection timer/RSS evidence remains a separate release gate.
  compatibility: Publish proactive PING as an SDKWork operational difference where Nginx does not emit one in the pinned probe window.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - NGINX_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
    - tests/nginx/http2-keepalive/nginx.conf
    - tests/nginx/http2-keepalive/probe.py
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm verify
```

## Runtime Semantics

Hyper's server records every inbound HTTP/2 Data and non-Data Frame. When no Frame has arrived for `http2KeepAliveIntervalMs`, its connection-owned timer sends one opaque PING. Any inbound Frame during the scheduled interval moves the next deadline; PINGs therefore measure transport/protocol inactivity, not application request duration.

After a PING is sent, Hyper waits `http2KeepAliveTimeoutMs` for its ACK. A timeout calls the H2 connection's `abrupt_shutdown(NO_ERROR)`, which flushes GOAWAY before closing. A responsive client may keep an otherwise application-idle connection alive by acknowledging PING; finite healthy-idle lifetime is intentionally not conflated with failure detection.

The selected Hyper server implementation always enables PING while no Streams are active once an interval is configured. HTTP/1 selected by TLS ALPN uses the separate Keep-Alive idle deadline and never enters H2 PING state. Both fields are Restart-only because they are copied into the listener's immutable Hyper connection builder.

## Nginx 1.26.2 Comparison

The pinned Windows Nginx 1.26.2 fixture negotiated TLS ALPN `h2`. During a 2,500 ms idle observation window after client Preface/SETTINGS, it emitted server SETTINGS, one connection WINDOW_UPDATE, and SETTINGS ACK, but zero proactive PING frames. The probe does not claim Nginx can never send PING under every module/build/timeout combination; it records the exact selected binary and window.

SDKWork with a 1,000 ms interval emits PING in the same idle condition. With a 300 ms ACK timeout and no client ACK, the tested server emits GOAWAY with `NO_ERROR` and closes. This is an explicit operational liveness difference, not a general Nginx Behavioral compatibility claim.

## Current Evidence

- Model, Schema, semantic range/coherence validation, example config, runtime topology, and Hyper builder wiring are implemented.
- Real TLS tests prove H1 protocol isolation, raw H2 PING, missing-ACK GOAWAY/close, one-connection recovery, and compliant-client continued use.
- Watch candidates changing interval or timeout retain the prior complete generation.
- Nginx fixture and repeatable Frame probe record the pinned comparison.
- Gateway verification passes 31 unit tests, 39 data-plane integration tests, and 4 raw HTTP/1 connection tests. Core verification passes 4 unit tests and 28 configuration integration tests.
- Strict workspace Clippy, formatting, `pnpm verify`, configuration validation, SDKWork pagination/envelope/import/documentation checks, and diff checks pass.

## Acceptance

Accepted on 2026-07-16 for proactive H2 peer-failure detection through PING/ACK timeout. PostgreSQL lifecycle execution remains ignored without `SDKWORK_DATABASE_TEST_POSTGRES_URL`; the unrelated existing `agent.sync` operation-pattern violation remains a repository-level commercial blocker. Responsive-but-idle maximum lifetime, 100,000-connection timer/RSS evidence, full H2 differential/fuzz conformance, and commercial runtime-core acceptance remain separate gates.
