# SDKWork Web Server HTTPS And Certificate Lifecycle PRD

Status: active
Owner: SDKWork maintainers
Application: sdkwork-web
Updated: 2026-07-31
Parent: [PRD.md](PRD.md)
Specs: NGINX_SPEC.md, SECURITY_SPEC.md, CONFIG_SPEC.md, ENVIRONMENT_SPEC.md, DEPLOYMENT_SPEC.md, TEST_SPEC.md

## 1. Purpose

Define production HTTPS behavior for the SDKWork Rust data plane, including TLS policy, SNI, certificate acquisition, private-key protection, renewal, atomic rotation, node-scoped distribution, failure handling, observability, and interoperability with the supported TLS client matrix (the Rust data plane itself is the reference implementation).

HTTPS is a release-critical runtime capability. A database certificate record, uploaded file, successful ACME request, or generated configuration does not by itself mean that HTTPS is active. Success requires cryptographic validation, authorized distribution, atomic runtime activation, and a served handshake that proves the intended certificate revision.

## 2. Production Security Posture

- Public production application traffic uses HTTPS.
- Plain HTTP is allowed only for deterministic HTTPS redirect, managed ACME HTTP-01 challenges, loopback development, or documented private health endpoints protected by network policy.
- TLS 1.0 and TLS 1.1 are forbidden. TLS 1.2 and TLS 1.3 are required.
- Unsafe protocol downgrade, invalid or incomplete chains, hostname mismatch, expired or not-yet-valid certificates, weak keys, and unreadable key material block activation.
- TLS 0-RTT is disabled in V1 because replay protection cannot be guaranteed for arbitrary application requests.
- Security defaults are centrally versioned. An application may select an approved policy or a stricter compatible policy, but it cannot silently weaken the platform minimum.

## 3. Listener And TLS Policy Model

Every HTTPS listener references a typed TLS policy containing:

- Minimum and maximum TLS version.
- Approved TLS 1.2 cipher suites and supported TLS 1.3 policy controls.
- Key exchange, signature algorithm, curve, and minimum key-strength policy.
- ALPN protocols, initially `h2` and `http/1.1`, with deterministic preference.
- SNI behavior and unknown-name/default-certificate policy.
- Session resumption, bounded session cache, ticket-key rotation, and ticket lifetime.
- Client authentication policy when mTLS is enabled.
- OCSP stapling policy and revocation behavior.
- Handshake, idle, header, and graceful-drain timeouts.

Listener activation fails when its TLS policy has no valid certificate candidate, references incompatible key material, conflicts with protocol settings, or violates the active security baseline.

## 4. SNI And Certificate Selection

- Hostnames are tenant assets under explicit root-domain Zones. Application routing is represented
  by `webserver_site_binding`; certificates never own or copy an application id.
- Managed certificate inventory and issuance are addressed by verified hostname identifiers. A
  `webserver_site_binding` is not an issuance prerequisite; it is required only for listener assignment.
- Certificate issue forms load eligible hostname choices through standard server pagination, keep
  selections stable across page changes, and never aggregate the tenant hostname inventory in the
  browser. Only `isVerified = true` assets are selectable; verified unbound assets remain eligible.
- A certificate covers 1..8 ordered exact or wildcard hostnames through
  `webserver_certificate_identifier`. The UI and API both enforce this limit before issuance. A hostname
  may participate in several certificate lifecycles.
- Listener selection intent is represented by `webserver_listener_certificate_binding`, not by copying a
  certificate id onto a hostname or application row.
- Certificate names support exact DNS names, standards-compliant wildcards, and SAN coverage. Regex certificate names are forbidden.
- The engine selects certificates by normalized SNI name and declared priority, then by compatible signature algorithm and client capabilities.
- Multiple certificate types, such as ECDSA and RSA, may serve the same names when deterministic selection and fallback are tested.
- One listener has at most one non-archived binding for each key algorithm. Rebinding the same
  certificate may advance its desired immutable version; binding a different certificate for an
  occupied algorithm returns a standard conflict and never leaks a database constraint error.
- A listener may define one explicit default certificate. Unknown SNI may use that certificate only when the listener policy permits it; strict listeners reject the handshake.
- HTTP `Host` routing is validated against SNI policy to prevent unintended cross-host service, while still respecting that SNI and HTTP authority are distinct protocol fields.
- Internationalized names use one declared IDNA normalization policy consistently for domain verification, issuance, SNI, and HTTP routing.

Activation checks the full chain, hostname coverage, validity interval with clock-skew allowance, public/private trust policy, key match, algorithm strength, duplicate ownership, and required intermediates.

## 5. HTTP Redirect And HSTS

- The standard public profile provides a port 80 listener whose only application behavior is an allowlisted ACME challenge response or a permanent redirect to the canonical HTTPS origin.
- Redirect construction uses validated host and path values and cannot reflect an untrusted arbitrary `Host` value into an open redirect.
- HSTS is enabled by an explicit production policy after HTTPS readiness is verified. `includeSubDomains` and preload are separate high-impact choices that require domain ownership review.
- HSTS is never emitted by plaintext HTTP, development self-signed profiles, or a host whose certificate coverage and HTTPS availability are not verified.

## 6. Certificate Sources

Logical certificate entries reference one of these sources:

| Source | Product behavior |
| --- | --- |
| SDKWork managed | Control plane owns policy, issuance or import, encryption, renewal, distribution, and audit. |
| ACME managed | ACME account and order lifecycle use approved HTTP-01 or DNS-01 challenge providers. |
| Secret manager or KMS | Configuration stores only a versioned resource identity; authorized workers resolve key material at activation. |
| Protected standalone file | Allowed for standalone deployments through canonical access-controlled paths outside source control. |
| Development self-signed | Allowed only for explicit local development and visibly marked untrusted; never promoted to production. |

Automatic renewal is available only for ACME-managed certificates. Development self-signed
certificates may be reissued manually, but they cannot enable the automatic renewal scheduler.

PEM, PKCS#8, PKCS#12, passwords, ACME account keys, DNS provider credentials, KMS data keys, and other secrets are forbidden in app manifests, Web Server authored configuration, API responses, logs, metrics, traces, diagnostics, and generated support bundles.

## 7. Domain Authorization

Before public activation or managed public-certificate issuance, every requested domain must have current ownership evidence from an approved method:

- ACME HTTP-01 response bound to the exact token and domain authorization.
- ACME DNS-01 TXT record through an approved provider or operator workflow.
- Separate SDKWork DNS or HTTP ownership challenge for imported certificates and policy validation.

Challenge tokens are cryptographically random, time-bounded, single-purpose, stored as secrets or secure digests where possible, and compared safely. A domain is not verified by changing a database status alone. The verifier performs the external observation, records resolver/vantage evidence, handles DNS propagation and negative caching, and expires evidence according to policy.

Wildcard issuance requires DNS-01. Production issuance or activation is blocked when ownership
evidence is absent, expired, revoked, or belongs to a different tenant/domain authorization
scope. Application authorization is evaluated separately when a certificate is assigned to a
listener; it is not certificate ownership evidence.

## 8. ACME Lifecycle

The managed ACME workflow supports:

1. Create or retrieve a durable, encrypted ACME account for the selected directory and tenant scope.
2. Create an idempotent order for the exact normalized identifier set.
3. Select an approved challenge type and provision only the necessary token or DNS record.
4. Observe challenge propagation from controlled vantage points before requesting validation.
5. Poll authorization and order states with bounded exponential backoff, jitter, deadlines, and ACME rate-limit awareness.
6. Generate or resolve the private key according to policy and submit the CSR.
7. Download the full certificate chain and independently parse the leaf before persistence. The
   normalized DNS SAN set and key algorithm must equal the request, the leaf must be currently
   valid, the PKCS#8 private key must match the leaf SPKI, and all returned hashes, validity values,
   SANs, issuer, subject, and algorithm metadata must equal freshly parsed evidence.
8. Remove challenge material after success, terminal failure, or expiry.
9. Distribute and activate the new certificate revision, then verify a served handshake.

ACME HTTP-01 routing has narrow precedence only for the exact `/.well-known/acme-challenge/<token>` path on authorized hosts. It cannot expose a directory, override unrelated routes, accept arbitrary tokens, or remain indefinitely after the challenge.

DNS-01 providers use least-privilege credentials and scoped record permissions. Provider credentials never enter application configuration or worker messages.

## 9. Certificate State Machine

Certificate lifecycle states are explicit and monotonic within a revision:

```text
pending -> authorizing -> issuing -> valid -> renewing -> rotating -> valid
    |           |           |          |          |           |
    +-----------+-----------+----------+----------+-----------+-> failed
                                         |
                                         +-> revoked
                                         +-> expired
```

Each transition records the operation, actor or worker identity, policy revision, certificate fingerprint, reason, timestamps, lease/fencing token, and redacted evidence. Retriable failure is distinguished from terminal failure. A stale worker cannot overwrite a newer state or certificate revision.

Automatic ACME renewal begins at a policy-defined window with jitter to avoid cluster-wide
synchronization. Manual reissuance remains available for supported certificate types independently
of that policy. The system retains and serves the last valid certificate while renewal is retried,
alerts before the safety threshold, and never replaces a valid certificate with an invalid candidate.

## 10. Private-Key Security

- Private keys are generated in an approved cryptographic provider or imported through a protected channel.
- At rest, keys use envelope encryption with a KMS-managed key or platform-equivalent protected key. Data-key and wrapping-key versions are recorded without exposing plaintext.
- In transit, certificate bundles use mutually authenticated encrypted channels and are additionally bound to the intended node, application, revision, and expiry where the distribution design supports envelope encryption.
- Nodes receive only keys required by their current or imminent assignments. Tenant-wide certificate bundles are forbidden.
- Decrypted key material exists only in the serving process or approved local secret provider, is never swapped or written to logs/crash dumps, and is zeroized when the selected Rust TLS/key types permit reliable zeroization.
- APIs return metadata, public chains, fingerprints, and references only. Private-key export is disabled by default and, if ever offered, requires a separate reviewed requirement, step-up authorization, and audit.
- Key access, rotation, failed decrypt, distribution, and use are auditable without recording secret bytes.

## 11. Atomic Activation And Zero-Downtime Rotation

Certificate activation follows the same immutable revision lifecycle as Web Server configuration:

1. Resolve the assigned encrypted resource locally.
2. Decrypt into protected memory and parse the key and chain.
3. Validate key match, chain, names, policy, time, and revision bindings.
4. Build a complete immutable TLS context away from the accept path.
5. Atomically swap the listener's certificate map only after all required hosts are ready.
6. Preserve existing connections on their original TLS context and use the new context for new handshakes.
7. Probe the served endpoint from the required vantage points and compare fingerprint, SNI, ALPN, chain, and policy.
8. Mark activation successful only after node convergence and probe evidence; otherwise restore the last verified context.

Partial maps are not exposed. Reload concurrency is serialized or version-fenced per listener, and superseded work is cancelled. Retained TLS contexts, certificate chains, session ticket keys, and rollback revisions are bounded by count and time so repeated rotations cannot leak memory.

## 12. Cluster Distribution And High Availability

- The control plane publishes signed, checksummed, immutable certificate metadata and node-scoped encrypted payload references.
- Nodes authenticate, authorize, checksum, decrypt, validate, stage, activate, and acknowledge a specific revision with a fencing token.
- Distribution is idempotent and resumable under duplicate, delayed, reordered, or replayed messages.
- Offline nodes do not block the defined quorum indefinitely. They must reconcile to an allowed revision before becoming ready for traffic.
- A node with missing, expired, unauthorized, or divergent certificate state fails readiness for affected public listeners and is removed from traffic.
- Multi-region deployments define issuer reachability, DNS challenge ownership, KMS locality, propagation deadline, and disaster-recovery behavior.
- Control-plane outage does not interrupt already active valid certificates, but expiry risk and inability to renew are alerted according to runbook thresholds.

## 13. OCSP, Revocation, And Client Authentication

- OCSP responses are fetched from validated responder URLs, signature-checked, cached with bounded lifetime, refreshed before expiry, and never fetched synchronously on the client handshake path.
- Default stapling failure policy is explicit. Certificates carrying a must-staple requirement use fail-closed behavior; other profiles document whether a temporarily unavailable response is soft-fail or hard-fail.
- Revocation requests require strong authorization, confirmation, reason recording, issuer result verification, immediate distribution impact analysis, and replacement planning.
- Optional mTLS policies define trust bundle references, verification depth, accepted client identities, revocation policy, mapping to application identity, and failure status. Trust bundles use the same immutable, node-scoped rotation model.
- Client certificate contents and identity attributes are treated as sensitive and are logged only through approved redacted fields.

## 14. Upstream HTTPS

Reverse-proxy targets using HTTPS must support:

- Trusted CA bundle reference and system/private trust selection.
- SNI and expected hostname independent of the resolved IP address.
- Full chain and hostname verification enabled by default.
- Optional client certificate and key reference for upstream mTLS.
- TLS version and cipher policy compatible with the platform security minimum.
- Bounded connection pools and session reuse partitioned by security identity.

Disabling upstream verification is forbidden in production profiles. Development exceptions are visibly marked, time-bounded where possible, and cannot be promoted without failing validation.

## 15. Performance And Memory Requirements

- TLS handshakes, certificate parsing, OCSP refresh, ACME orders, and KMS operations run through bounded concurrency and queues with deadlines.
- Expensive key generation and cryptographic parsing do not block asynchronous request executors.
- Certificate and TLS-context caches are bounded by entry count and estimated bytes; eviction never removes the only active valid context.
- SNI lookup is indexed and does not linearly scan all tenant certificates per handshake.
- Certificate synchronization uses deltas and streaming; memory is O(assigned delta size), not O(all tenants or all certificates).
- Handshake rate limits and per-source controls protect CPU while preserving configured trusted proxy behavior.
- Load and soak tests include handshake storms, session resumption, many SNI names, slow clients, repeated failed rotations, OCSP failure, ACME retries, and node reconnects with stable memory and no executor starvation.

## 16. Observability And Alerts

Metrics and bounded events cover:

- Handshake totals, failures by low-cardinality reason, duration, negotiated protocol, ALPN, and approved cipher group.
- Active certificate revision and public fingerprint by listener/host without private material.
- Days to expiry, renewal window, renewal attempt/result, ACME order/challenge state, OCSP freshness, and trust-bundle revision.
- Distribution lag, decrypt/validation failure, node convergence, activation/rollback duration, and served-probe mismatch.
- Session resumption effectiveness, TLS context/cache size, handshake saturation, and crypto-worker queue depth.

Alerts include configurable 30-day, 14-day, 7-day, 72-hour, and 24-hour expiry thresholds; renewal terminal failure; invalid served chain; fingerprint divergence; stale OCSP for must-staple; KMS or ACME outage; and certificate state divergence across ready nodes. Alert routing, ownership, suppression, and escalation are part of the production runbook.

## 17. APIs, SDKs, Persistence, And Transactions

- Certificate, domain, challenge, issuance, renewal, rotation, revocation, status, event, and audit APIs follow SDKWork response envelopes, problem details, IAM, idempotency, optimistic concurrency, and asynchronous operation semantics.
- Certificate issue and manual renewal return HTTP `202` with `SdkWorkAsyncData`; they never return
  a certificate resource as proof that external issuance completed. App and backend generated SDKs
  retrieve the tenant/owner-scoped operation by `operationId` until a terminal state is observed.
- `webserver_certificate_operation` persists `ISSUE` and `RENEW` intent independently of browser or API
  process lifetime. Claims use `FOR UPDATE SKIP LOCKED`, expiring leases, monotonically increasing
  fencing tokens, bounded attempts, and retry timestamps. Stale workers cannot finalize newer work,
  and exhausted leases become terminal failures with a stable non-secret `failureCode`.
- PC clients poll at a bounded cadence with a ten-minute client deadline. Closing the dialog aborts
  only browser HTTP polling, refreshes the certificate inventory to expose pending state, and never
  deletes or cancels the durable server operation.
- Growing certificate, domain, operation, event, node-status, and audit collections use store-level cursor/keyset pagination with bounded `page_size` and standard `pageInfo`.
- PostgreSQL is the only cloud and standalone authoritative server database and uses transactional row claims, leases, fencing, unique constraints, and outbox/event delivery for state transitions that cross process boundaries.
- Database commits never claim external ACME, DNS, KMS, node activation, or public probe success. External effects use durable operations and verified state transitions.
- PostgreSQL runs certificate lifecycle, migration, rollback policy, uniqueness, transaction,
  backup/restore, failover, and crash-recovery scenarios.

## 18. Verification Matrix

Release verification includes:

| Area | Required evidence |
| --- | --- |
| Interoperability | Current supported browsers, SDKs, `openssl`, Rust data plane reference, HTTP/1.1, HTTP/2, RSA/ECDSA, SNI, wildcard/SAN, and ALPN matrices. |
| Negative security | Weak protocol/cipher, bad chain, hostname mismatch, expired/not-yet-valid cert, wrong key, unknown SNI, malformed ClientHello, invalid OCSP, traversal, secret redaction, replay, and unauthorized node tests. |
| Lifecycle | Import, issuance, HTTP-01, DNS-01, propagation delay, renewal, key rotation, revocation, challenge cleanup, clock skew, rate limit, and issuer outage tests. |
| Runtime | Atomic reload, concurrent reload fencing, existing-connection survival, served fingerprint probe, rollback, node restart, offline reconciliation, and rolling upgrade tests. |
| Scale | Handshake throughput/latency, many SNI names, node-scoped delta sync, bounded caches/queues, 24-hour soak, and no-OOM fault injection. |
| Persistence | PostgreSQL transaction rollback, idempotency replay/conflict, duplicate worker, lease reclaim/exhaustion, stale fencing token, migration, backup/restore, failover, and crash recovery. |

## 19. Acceptance Criteria

- Every production public host serves HTTPS with TLS 1.2/1.3, a valid hostname-covering chain, and the intended active fingerprint.
- HTTP is limited to approved redirect, challenge, development, or private health behavior.
- SNI, RSA/ECDSA selection, ALPN, redirect, HSTS, mTLS where configured, and upstream verification pass interoperability and negative tests.
- ACME HTTP-01 completes real external validation through the data plane's narrow-precedence challenge endpoint, cleans up challenges, respects retries/rate limits, and renews before the safety threshold (CA ARI window preferred).
- Private keys never appear in authored configuration, API output, logs, metrics, traces, database plaintext, generated Web Server configuration, or support bundles.
- Rotation is atomic, existing healthy connections remain available, all ready nodes converge, and a failed candidate restores the last verified context.
- Certificate operations, queues, caches, TLS contexts, worker concurrency, and node synchronization remain bounded under load and soak tests without OOM.
- PostgreSQL lifecycle and recovery suites pass and no database-only state transition is presented as proof of an external effect.
- Expiry, renewal, served-certificate divergence, KMS/ACME failure, and node-distribution alerts are exercised with current runbooks before commercial release.

## 20. Current Verified Delivery Boundary

[REQ-2026-0006](../requirements/REQ-2026-0006-multi-certificate-sni.md) delivers the bounded standalone static-map portion of sections 3 and 4: one HTTPS listener can load multiple protected-file certificates, validate leaf time/SAN/key consistency before opening, and select Exact or single-label Wildcard SNI through immutable indexed maps. Exact wins over Wildcard. Unknown or absent DNS SNI fails closed; there is no implicit first-certificate fallback.

The node-scoped `sdkwork.tls-runtime.v1` consumer now extends that boundary with protected
`file:<opaque-version-id>` material resolution, node/hash/fingerprint/declared-time validation,
listener ALPN and TLS-version enforcement, complete SNI-map construction, atomic Rustls live
rotation, failed-candidate last-known-good retention, and staging/production A/B restart recovery.
Existing connections retain the Rustls context captured at acceptance; new handshakes use the
atomically replaced context.

[REQ-2026-0070](../requirements/REQ-2026-0070-layered-certificate-resolution.md) lets one listener
serve both sources at once. A listener that declares `tlsCertificateResolution: policy-first` keeps
its configured certificate files as the upper layer and the assigned certificate set as the lower
layer of one resolver, so a configured file wins for each server name it covers and is usable for,
and the assigned set covers every other name. A configured file that is missing, malformed, or
outside its validity window degrades that server name to the assigned set instead of failing the
listener, and an assigned-certificate rotation is re-layered beneath the configured files so the
precedence survives every rotation. Certificate file content remains restart-only, as REQ-2026-0006
accepted.

This still does not complete this PRD. Explicit default-certificate policy, same-name RSA/ECDSA
negotiation, full public trust-chain and revocation validation, Deploy-owned certificate-version
assignment/distribution/observation, KMS/Vault/CSI authorization and decryption, public
served-fingerprint probes, cluster convergence, handshake abuse controls, certificate telemetry,
load/soak, and incident runbooks remain commercial release blockers. The current authored
Kubernetes profile explicitly selects external ingress termination until those control-plane and
operations gates close.
