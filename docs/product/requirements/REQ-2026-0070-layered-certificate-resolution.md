# REQ-2026-0070 Layered Certificate Resolution

```yaml
id: REQ-2026-0070
title: Prefer a usable configured certificate file per SNI name with per-name fallback to the assigned set
owner: sdkwork-webserver
status: implemented
source: operator
problem: A listener can be configured with protected certificate files and also receive node-scoped certificate assignments, but the two sources were mutually exclusive. An operator therefore had to choose between an explicitly configured certificate and the managed rotation path, and a configured file that was missing, unusable, or did not cover a host left that host unservable even though an assigned certificate covered it.
goals:
  - Allow one listener to declare tlsPolicyRef and tlsRuntime together by declaring tlsCertificateResolution policy-first.
  - Resolve every handshake per server name, so a configured file wins for the names it covers while it is inside its own validity window.
  - Fall back per server name to the assigned certificate set when no configured file covers the name or every covering file is unusable.
  - Keep exact-name precedence over single-label wildcard precedence inside each source. Two wildcards can never
    cover one name, so no ordering between them exists to define.
  - Fail closed for a server name that neither source covers.
  - Re-layer an assigned-certificate rotation beneath the configured files instead of replacing them.
  - Report an unusable configured file at listener activation without failing the listener, because the assigned set is the declared lower layer.
non_goals:
  - Reloading a configured certificate file whose content changes on disk; certificate file content stays restart-only per REQ-2026-0006.
  - An implicit or explicit default certificate for unknown or absent SNI.
  - Same-name RSA/ECDSA selection driven by client signature algorithms.
  - Cross-process certificate synchronization, Deploy-owned assignment, or distribution; those stay in REQ-2026-0061 and REQ-2026-0062 scope.
  - Root-domain Zone management and application authorization for assigned certificates.
users:
  - Platform operators
  - Site reliability engineers
acceptance_criteria:
  - JSON Schema and Serde accept tlsCertificateResolution with the values policy-only, assignment-only, and policy-first.
  - An omitted tlsCertificateResolution still resolves to policy-only for a listener that declares only tlsPolicyRef, and to assignment-only for a listener that declares only tlsRuntime.
  - Declaring both tlsPolicyRef and tlsRuntime without tlsCertificateResolution policy-first fails semantic validation.
  - Declaring tlsCertificateResolution policy-first without both sources fails semantic validation.
  - Declaring a resolution that does not serve a declared source fails semantic validation.
  - A configured file wins over an assigned certificate for a server name that both cover while the file is usable.
  - A configured file outside its validity window yields to a usable assigned certificate for the same name.
  - A configured file outside its validity window is still served when no other source covers the name, matching the existing expired-leaf tolerance.
  - An assigned-certificate rotation re-layers beneath the configured files and the configured file keeps winning.
  - Within one source an exact name outranks a single-label wildcard, and no two wildcards compete for one name because each pins the number of labels below its suffix.
  - A poll of the TLS runtime snapshot that finds the file byte-identical to the one the active generation was compiled from compiles nothing, and a file whose bytes changed but which compiles to the installed snapshot does not reload the listener.
  - A server name that neither source covers resolves to no certificate.
  - An unusable configured file is reported as a warning at listener activation and does not stop the listener from serving the assigned set.
  - The data plane publishes what it will serve for every assigned server name, together with the source that won, as a local artifact beside the TLS runtime snapshot.
  - The node daemon derives its SERVED observations from that report, counts a name a configured file legitimately won as served rather than failed, and finishes a sync on a node that runs no stock Nginx.
  - A node that configures no TLS runtime keeps the handshake probe as its authority and says so at startup.
non_functional_requirements:
  security: Every source fails closed; a certificate outside its own validity window never outranks a usable one, and a source that does not cover a name is never served for it. Private-key bytes stay out of configuration, logs, metrics, and diagnostics.
  privacy: Bundle identifiers and declared public server names are the only certificate details this runtime slice exposes.
  performance: Handshake selection stays indexed - one exact hash lookup, plus a wildcard scan bounded by the declared names of one source - with no per-handshake file I/O or certificate parsing. A poll that finds the TLS runtime snapshot unchanged costs one bounded read and one hash instead of recompiling it.
  reliability: A rejected rotation retains the last-known-good composed configuration, and a source that contributes no certificate never removes the other source coverage.
traceability:
  prd: PRD-https-and-certificates sections 3, 4, and 11
  specs:
    - ../sdkwork-specs/CONFIG_SPEC.md
    - ../sdkwork-specs/SECURITY_SPEC.md
    - ../sdkwork-specs/RUST_CODE_SPEC.md
    - ../sdkwork-specs/NGINX_SPEC.md
    - ../sdkwork-specs/TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway --no-default-features --lib data_plane::tls
  - cargo test -p sdkwork-webserver-core --test tls_runtime_snapshot
  - cargo test -p sdkwork-webserver-agent
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
```

Product authority: [PRD-https-and-certificates.md](../prd/PRD-https-and-certificates.md).

## 1. Configuration

A listener selects its certificate sources with three mutually describing fields:

| Field | Meaning |
| --- | --- |
| `tlsPolicyRef` | References the TLS policy whose `certificateRefs` are protected certificate files. |
| `tlsRuntime` | Declares that the listener consumes the node-scoped certificate assignment runtime. |
| `tlsCertificateResolution` | Selects which sources the listener serves: `policy-only`, `assignment-only`, or `policy-first`. |

`tlsCertificateResolution` is optional. When it is omitted the resolution is derived from the declared
sources, which keeps every existing configuration valid: `tlsPolicyRef` alone stays `policy-only`,
`tlsRuntime` alone stays `assignment-only`. Declaring both sources has no implied value and requires an
explicit `policy-first`, so an operator can never combine them by accident.

## 2. Resolution Order

Resolution happens once per handshake, keyed by the normalized SNI name:

1. The most specific *usable* configured certificate. Inside this source an exact name is checked before
   any wildcard, and a stale exact entry does not shadow a usable wildcard.
2. The most specific *usable* assigned certificate, with the same exact-before-wildcard order.
3. The most specific configured certificate that merely covers the name, ignoring validity.
4. The most specific assigned certificate that merely covers the name, ignoring validity.

Steps 3 and 4 exist because a certificate outside its own validity window must still be served when
nothing else covers the name. That matches the tolerance `load_certified_key` already applies to an
expired leaf at listener activation, so a host stays reachable while its replacement certificate
propagates. A name that neither source covers gets no certificate and the handshake fails.

Wildcard matching covers exactly one additional DNS label, mirroring RFC 6125 and what TLS clients
themselves enforce; this is the accepted behavior of REQ-2026-0006 and is unchanged here.

Because a wildcard match pins the label count, `*.a.test` and `*.test` cover disjoint sets of names and
at most one wildcard can ever match a queried name. Selection is therefore a lookup rather than a
ranking, and a source index needs no ordering between its wildcards - which is what removes the
ordering step, the sorted vector, and the build-order contract that went with them.

## 3. Interaction With Assigned-Certificate Rotation

A `policy-first` listener keeps the assigned certificates as the lower layer of one resolver. When the
TLS runtime activates a new snapshot it composes the configured files above the candidate assigned set
rather than adopting the candidate configuration directly, so the precedence survives every rotation.
Protocol parameters (minimum and maximum TLS version, ALPN, and client authentication) come from the
operator `tlsPolicyRef`, because that is what declared them. A candidate whose ALPN is incompatible
with the listener is still rejected before activation.

## 4. Activation Behavior

An unusable configured file, whether missing, malformed, or outside its validity window, is reported as
a warning and dropped from the upper layer. The listener still opens and serves the assigned set for
that name. A listener that declares only `tlsPolicyRef` keeps the original fail-closed behavior, because
with no lower layer a dropped file would leave the listener serving nothing rather than falling back.

## 5. Interaction With The Node Observation Plane

Introducing a configured layer above the assigned layer invalidates an assumption in the node
certificate observation plane. This requirement records that interaction rather than silently
accepting a broken rollout signal.

`verify_served_certificate` in `sdkwork-webserver-edge-runtime` opens a real TLS connection to the
local listener and asserts that the served leaf equals the *assigned* certificate fingerprint for
every hostname in the node manifest. On a `policy-first` listener whose configured file also covers
such a hostname, the served leaf is the configured certificate, so the probe reports
`TLS_SNI_PROBE_FAILED`. That code routes through `record_deployment_failure` into a `FAILED` row in
`webserver_certificate_node_state`, and `promote_converged_listener_certificate_bindings` converts any
`FAILED` row into a `FAILED` listener certificate binding. A listener that is serving correctly
would be marked failed purely because the operator's own certificate took precedence.

The same plane is also the only writer of `webserver_certificate_node_state`. `agents.rs` records
observations from the node daemon heartbeat, and that daemon drives the stock-Nginx activation path
that this repository retires for public domains. In a standalone deployment, where Rust
request-path serving owns the listener, nothing reports what the data plane actually served, so
assigned-certificate convergence has no observable signal at all.

Two resolutions were viable and mutually exclusive. This requirement takes the second:

- Widen the probe expectation, keeping the external handshake and teaching it which fingerprints
  are legitimate for a hostname.
- Move `SERVED` authority to the data plane, because the component that resolves SNI names is the
  only one that knows which certificate a name actually receives.

The change has two halves:

- The Rust data plane publishes the certificate it will actually serve for each server name,
  together with the source (`config` or `assignment`) that won, as a local artifact beside the TLS
  runtime snapshot.
- The node daemon derives its `SERVED` observations from that report instead of asserting the
  assigned fingerprint, and completes a sync on a node that runs no stock Nginx.

Both halves are implemented. The Nginx half removes the false failure: the entry points that
only assert something - `validate_nginx_config`, `reload_nginx`, `verify_served_config`, and
`validate_active_nginx_config` - are explicit no-ops when `SDKWORK_WEBSERVER_NGINX_ENABLED` is
false, and each logs the work it skipped, so a node daemon no longer aborts a whole sync on a step
that has nothing to validate. `stage_nginx_config` still fails loudly, because it has to hand back
a real staged file and has no safe empty result. `verify_served_config` notes in its own contract
that it then reports success without producing served-revision evidence, which the data-plane
report supersedes.

The data-plane half supplies that evidence. Its shape is defined once, in
`sdkwork-webserver-core::tls_runtime::served_certificate`, together with the file name
(`tls-served-certificates.json`), the byte ceiling, and the path derived from the snapshot file, so
the process that writes it and the process that reads it cannot disagree about the handoff.
`TLS_RUNTIME_SNAPSHOT_FILE_ENV` moved to `sdkwork-webserver-core::runtime_env` for the same reason:
the data plane locates its snapshot with it, and the node daemon derives the report's path from
that same file.

The data plane publishes after every activation - the initial load and each rotation - and it
builds the report from the exact resolver layers it has just installed rather than from an
equivalent set rebuilt for the report, so the report cannot describe a layer set the listener is
not serving. Each assigned server name yields one entry carrying the winning source and the
fingerprint that name will receive, which is what lets a reader tell "the operator's file won this
name on purpose" apart from "the assignment did not take". Publication stages beside the target
and renames into place, so a reader sees the previous complete report or the next one and never a
partial file. A starting controller first discards any report left by a previous process, because a
report describes a configuration only the process that published it has installed.

`sdkwork-webserver-agent` prefers the report and keeps the handshake probe only for a node that
configures no TLS runtime, logging the authority it chose at startup so an operator can see why a
node is or is not reporting served fingerprints. Under the report authority the verdict follows the
source: a name a configured file won is served as intended, because that certificate legitimately
outranks the assignment; an assigned name whose reported fingerprint differs from the manifest's is
`TLS_SERVED_FINGERPRINT_MISMATCH`; a name no source covers is `TLS_SERVER_NAME_UNCOVERED`; and a
manifest hostname with no report entry at all is `TLS_SERVER_NAME_NOT_ASSIGNED`. A failure is
recorded only once the report was resolved after the observation already on record, so a rotation
the data plane has not adopted yet leaves the previous observation in place instead of manufacturing
a failure.

## Acceptance Evidence

Implemented on 2026-09-17 for the bounded standalone layered-resolution slice.

- Unit tests in `data_plane::tls_resolver` prove configured-file precedence, stale-file fallback to the
  assigned set, stale-file retention when nothing else covers the name, exact-before-wildcard precedence
  inside one source, wildcard disjointness across suffix lengths, wildcard precedence being independent
  of insertion order, cross-source wildcard/exact interaction, duplicate claim rejection within one
  source, shared ownership across the two sources, and fail-closed resolution for an uncovered name.
- Semantic validation proves that the resolution must match its declared sources, that both sources
  require an explicit `policy-first`, and that `policy-first` requires both sources.
- The embedded JSON Schema accepts `tlsCertificateResolution` on a listener and still rejects unknown
  listener properties.
- `data_plane::tls_served_report` builds the report from the installed layer set, attributes each
  name to the source that won, keeps an exact name ahead of a wildcard inside one source, replaces
  the previous revision without leaving a staging file behind, and refuses an oversized report
  without touching the target.
- `served_certificates` in the node daemon reads that report, refuses one whose schema version it
  does not know, treats a configured-file win as served, distinguishes the uncovered,
  not-assigned, and fingerprint-mismatch verdicts, leaves a verdict it cannot establish yet
  unchanged, and rewrites no state file whose verdicts already match.
- Hardening of the distribution chain this requirement depends on, found while making the report a
  sibling of the snapshot: `tls_material_distribution` refuses a version uuid that would name a path
  outside the material root and applies that rule before it writes, rather than only while building
  the snapshot it has already written; and its directory sync is skipped on a platform that cannot
  open a directory as a file handle, which previously failed the whole publication on Windows.

Verification for the observation-plane revision: `cargo test -p sdkwork-webserver-agent` reports
19 passed and 0 failed, `cargo test -p
sdkwork-api-webserver-standalone-gateway --no-default-features --lib data_plane::tls` reports 35
passed and 0 failed, and `cargo test -p sdkwork-intelligence-webserver-service` reports 35 passed
and 0 failed. The two distribution defects were reproduced against the pre-fix code before being
fixed: the traversal test failed with the escaped directory created, and the publication test
failed with `open TLS material directory ...: Access is denied. (os error 5)`.

### Steady-state cost of the resolver revision

The watcher asks the filesystem whether the snapshot changed once per `pollIntervalMs`, and the answer
is almost always that it did not. It now answers that by hashing the file it has already read and
comparing the digest with the one the active generation was compiled from, and by returning before any
further work. Measured on this machine with a 256-assignment, 165 KiB snapshot: one compile - a JSON
parse, a schema check, a canonical re-hash, and a name index over every assigned server name - costs
about 4.0 ms, of which about 0.5 ms was the recompilation of the embedded JSON Schema; reading the same
file and hashing it costs about 70 us. At the default 2000 ms interval the idle cost per listener falls
from about 0.2% of one core to under 0.004%, and at the 250 ms minimum from about 1.6% to about 0.03%.
The behavioural proof is `a_content_identical_snapshot_is_absorbed_without_reloading_the_listener`:
its final assertion can only hold if the second read is answered without compiling, because compiling
would return a candidate.

Two pieces of dead weight came out with it. Every embedded JSON Schema is now compiled once per process
through one shared helper, instead of being rebuilt for each document by four separate loaders that
each carried their own copy of the diagnostic-truncation helper. And `sdkwork-webserver-core` no longer
builds an index of assigned server names for resolution at all: its only consumer was a test, it
normalized names with a different function than the data plane, and it could have diverged from the
resolution order the data plane actually enforces while serving nothing. The component port
`tlsSniAssignmentIndex` is withdrawn with it. SNI resolution has one implementation, in the data plane,
where the loaded certificates and their validity windows are.

Repository-level compilation was re-checked for this revision and is clean. `cargo check --workspace
--tests` finishes with zero errors, as do `cargo check -p sdkwork-webserver-core` and `cargo check -p
sdkwork-api-webserver-standalone-gateway` (default features, which include `management`), and
plain `cargo clippy` over the same two crates also exits zero, emitting only warnings.

Clippy under `-D warnings` is not clean, and this revision neither caused nor fixed that. The core
library reports 146 errors, dominated by `clippy::result_large_err` on every function returning
`Result<_, WebServerConfigError>`: that enum's `Toml` variant carries a `toml::de::Error` and so
exceeds the 128-byte threshold. The rest are `manual_memcpy`, `identity_op`, `derivable_impls` and
similar in the `nginx` and `config` modules. The gateway, which cannot be linted until core compiles,
adds its own set across `data_plane` and its integration tests. Every one of those sites sits in a
file this revision leaves untouched - the error enum was not modified at all, and the named files are
at their committed state - while no file this revision added or edited produces a single Clippy
finding. An earlier revision of this document claimed `cargo clippy` over the two crates exits zero
under `-D warnings`; that claim was unverified, is false, and has been withdrawn.

The earlier statement here that this crate set was blocked by an unrelated in-progress change in
`sdkwork-deploy-service-host` no longer holds and has been withdrawn.

Still not claimed, with reasons: the full `pnpm verify` chain. Its `cargo test --workspace` step
carries three failures that predate this requirement and fall outside its changed files - a fixture
drift in a sibling repository that `server_toml` reads, a Windows-only path-separator assertion in
`module_imports`, and a self-described temporary diagnostic named `dump_import_materialization`. The
repository also defines no Clippy gate in any script, so Clippy is reported here as observational
rather than gating.

This acceptance does not include zero-downtime rotation of a statically configured certificate file,
or commercial HTTPS readiness. Cross-process certificate synchronization stays where it already
lives - the `tls_material_distribution` snapshot and the `certificate_activation` convergence chain
that REQ-2026-0061 and REQ-2026-0062 own. What this requirement adds is the missing signal on that
chain: a node serving through the Rust data plane can now report which certificate each name
actually received.

### Certificate loading paths agree on what a name is

Auditing the load path turned up three cases where a configuration or an assignment validated
cleanly and then could not be served. Each is closed now, and each by removing a duplicate definition
rather than by patching a call site.

`policy-first` was refused by its own validation. A listener combining `tlsPolicyRef` with
`tlsRuntime` is the layered strategy - configured files win for the names they cover and the assigned
set covers every other name - so a virtual host whose name no configured certificate covers is
exactly what such a listener is for. Validation nevertheless demanded that a certificate in the
policy cover every listener server name, and rejected the configuration with `TLS policy ... has no
certificate covering server name ...`. The rule now applies only to a listener that serves no
assignments, because only a listener with no lower layer has to be complete; without this the mode
could not be configured at all. `policy_only_requires_a_configured_certificate_for_every_server_name`
pins the retained half, and `policy_first_listener_leaves_uncovered_server_names_to_the_assigned_set`
pins the fix.

An IP address was accepted as a certificate server name. `normalize_server_name` returns one, because
an HTTP virtual host may legitimately be addressed by an IP, but a certificate is chosen through SNI,
which RFC 6066 confines to DNS names, and the loader matches declared names against SAN DNS names
alone. Such a certificate could never load, and under `policy-first` it left the upper layer with no
more than a warning. That rule is now stated once, as `normalize_tls_server_name`, and called by
configuration validation, assignment snapshot validation and the certificate index alike.

The assignment snapshot validator used a different rule from the loader. It normalized server names
with the website hostname normalizer, whose IDNA conversion is genuinely needed - it is what names
the canonical form a control plane must send for a Unicode name - but which also admits names the
certificate index cannot hold, an underscore label among them. A snapshot carrying one passed
validation and then failed to install. The validator keeps the IDNA diagnostic and adds the
indexability rule beside it, so a name that validates is a name that can be indexed.

The same revision removes a third copy of the wildcard rule. Certificate SAN coverage, SNI selection
and website host selection each carried their own `*.suffix` predicate. They agree today, and the
audit established that only by reading all three; they now call one function, whose rule has one test
next to it.

A stream listener's TLS acceptor substituted `localhost` for a certificate that declared no server
name, indexing it under a name it need not cover. The schema already requires at least one name, so
the substitution was unreachable, and it is now an outright refusal rather than an invented name.

One further case was audited and found already correct, and is recorded here so it is not
re-investigated: a wildcard certificate name against a wildcard declared name. `server_name_covers`
answers "does this certificate name cover that server name", and the second argument may itself be a
wildcard, which makes the two a question about patterns rather than about a name. It is right: a
distinct wildcard never covers another, because the label-count rule rejects the haystack - the
`*` is followed by a dot, so the remaining prefix still contains one. Only an identical wildcard
matches, through the equality check. `a_wildcard_certificate_does_not_cover_a_different_wildcard_server_name`
pins all four directions of that.

The order in which a certificate is chosen for a name is likewise stated once. The control plane
publishes the assigned certificates as a set - `TlsAssignmentSnapshot.assignments` carries a
`serverNames` list per certificate - and the data plane indexes that set, so "exact name first, then
a wildcard" lives only in `CertificateSourceIndex`. No control-plane component resolves a
certificate for a domain on its own, which is what keeps the served certificate and the certificate
the control plane believes it published from drifting apart.

Verified for this revision, all with the changed crates as the only build input:

| Command | Result |
| --- | --- |
| `cargo test -p sdkwork-webserver-core --test webserver_config` | 85 passed, 0 failed |
| `cargo test -p sdkwork-webserver-core --test tls_runtime_snapshot` | 7 passed, 0 failed |
| `cargo test -p sdkwork-api-webserver-standalone-gateway --no-default-features` | 380 passed, 0 failed |
| `cargo test -p sdkwork-webserver-agent` | 19 passed, 0 failed |
| `cargo check --workspace --tests` | exit 0, no new warnings |

The gateway run includes `selects_exact_and_wildcard_sni_certificates_and_fails_closed`, which is the
only test that exercises the whole chain rather than one layer of it: it writes a TOML configuration,
starts the data plane, and completes real TLS handshakes over a real socket, asserting that an exact
name receives its own certificate, that a name below a wildcard receives the wildcard certificate,
that an exact name still wins over a wildcard that also covers it, and that an unknown server name and
a connection carrying no DNS SNI both fail the handshake. That test passing is what makes "the
capability landed" a claim about the running server rather than about its parts.

The three workspace test failures that predate this requirement are unchanged, as is the Clippy
position recorded above.
