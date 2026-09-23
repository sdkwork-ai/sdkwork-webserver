# ADR-20260922 TLS Version Window Is An Operator Policy On Both Configuration Surfaces

Status: accepted
Requirement: REQ-2026-0024, REQ-2026-0003
Owner: SDKWork maintainers
Date: 2026-09-22
Specs: SECURITY_SPEC.md, CONFIG_SPEC.md, NGINX_SPEC.md, IAM_SPEC.md, RUST_CODE_SPEC.md

## Context

Two authored configuration surfaces declare the same TLS listener control and both
were dropping it:

- nginx surface: `ssl_protocols` was listed in `ACCEPTED_IGNORED`
  (`crates/sdkwork-webserver-core/src/nginx/mapping.rs`) and in an explicit
  server-level no-op arm.
- typed TOML surface: `[http.server.tls] protocols` is listed in `SERVER_TLS_KEYS`
  and constrained by the config schema to `["TLSv1.2", "TLSv1.3"]`, but
  `Materializer::ensure_certificate` and `Materializer::materialize_server` both
  wrote the literal pair `"tls1.2"` / `"tls1.3"` into every TLS policy. The key was
  accepted and never read.

Downstream of both, `SemanticValidator` rejected every window except exactly
`TLS 1.2 .. TLS 1.3`, so even a consumed declaration could not have taken effect.

The cost was not cosmetic. `protocols = ["TLSv1.3"]` is the deployment form of an
industry-standard TLS 1.3-only hardening policy, and an operator writing it got a
silent TLS 1.2 fallback: the artifact looked hardened and was not. This is the same
defect class as a directive accepted-but-ignored anywhere else, and it is worse on
the TOML surface because the key is advertised as supported.

The capability was already implemented underneath. `tls_material::tls_protocol_versions`
maps all three legal windows (`1.2..1.2`, `1.3..1.3`, `1.2..1.3`) onto rustls
`SupportedProtocolVersion` sets, and reserves `unreachable!` for the inverted range.
Upstream (`[http.upstream.tls]`) validation already accepted any window with
`minimum <= maximum`. Only the downstream listener policy carried the extra lock.

## Decision

- The TLS version window of a listener policy is an operator-controlled policy with
  exactly two invariants: the minimum is not below the platform floor (TLS 1.2) and
  the minimum is not above the maximum. Any window inside `[TLS 1.2, TLS 1.3]` is
  legal, including the singletons `TLS 1.2..TLS 1.2` and `TLS 1.3..TLS 1.3`.
  `PRD-https-and-certificates.md` §2 grants exactly this ("an application may select
  an approved policy or a stricter compatible policy, but it cannot silently weaken
  the platform minimum"), and §3 requires every policy to carry a minimum and a
  maximum version. The removed rule forbade the stricter case the PRD grants.
- TLS 1.0 and 1.1 remain forbidden. `TlsVersion` cannot express them, and the
  validator keeps a guard that fires if a future variant widens the enum past
  `TlsVersion::PLATFORM_FLOOR` / `PLATFORM_CEILING` without updating them.
- The platform default window stays `TLS 1.2 .. TLS 1.3`
  (`SDKWORK_WEBSERVER_SPEC.md` §10 `protocols` default, `NGINX_SPEC.md` §3).
  Narrowing is opt-in; nothing narrows implicitly.
- Both surfaces resolve to the same policy field, so they cannot diverge:
  - nginx `ssl_protocols` is parsed as a token set and intersected with the
    supported window. Legacy tokens (`TLSv1`, `TLSv1.1`) stay legal because the
    intersection can only *narrow* what a client may negotiate, which is never
    weaker than the declaration; a set that keeps neither 1.2 nor 1.3 is refused
    rather than silently reverted to the broad default.
  - TOML `protocols` accepts only the schema's two tokens and fails closed on
    anything else.
- One TLS policy is materialized per certificate, so every server naming a
  certificate must agree on the window. A second server that narrows the same
  certificate differently is refused with a diagnostic instead of resolved
  last-wins, matching the existing `clientCertificate` conflict rule.
- `TlsVersion::wire()` is the single serializer for both surfaces, so a new variant
  cannot be added without the compiler forcing a decision here.

## Alternatives

- Delete the whole window from the listener policy and hardcode TLS 1.2/1.3:
  rejected because it discards an implemented runtime capability and denies an
  operator a documented hardening control.
- Keep the lock and document the divergence: rejected because the PRD explicitly
  grants stricter policies, so the lock contradicted the authority it cited, and a
  silently ignored hardening key is a security-report-grade defect rather than a
  documented limitation.
- Remove the validator check entirely without a replacement guard: rejected because
  the floor and ceiling would then be unenforced against future `TlsVersion`
  variants.
- Reject legacy tokens in nginx `ssl_protocols` outright: rejected because it turns
  a loaded config into a startup failure for a declaration that is already
  impossible to honor more strictly, and clamping is the safer migration behavior.

## Consequences

- `protocols = ["TLSv1.3"]` and `ssl_protocols TLSv1.3;` now produce a TLS 1.3-only
  policy on both surfaces, and the listener negotiates exactly that set.
- Existing artifacts are unaffected: every generated sidecar declares
  `TLSv1.2 TLSv1.3`, which is the default window. The full-surface nginx behavioral
  corpus stayed at 66/66 after the change.
- Operators can now narrow below the default, so the platform minimum is the only
  floor. Reviewing a deployment for TLS 1.3-only compliance means reading the
  rendered policy, not assuming the platform default.
- The nginx stream surface (`ssl_protocols` inside `stream {}`) is still accepted
  and ignored; `StreamTlsMode::Terminate` has no window field and
  `stream_proxy.rs` hardcodes `TlsVersion::Tls12, TlsVersion::Tls13`. Closing that
  requires a field on the stream TLS model and is tracked separately.
