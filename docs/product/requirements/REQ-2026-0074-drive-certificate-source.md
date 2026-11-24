# REQ-2026-0074 Drive Certificate Source

```yaml
id: REQ-2026-0074
title: Serve a domain from the SDKWork Drive certificate authority with a multi-level cache
owner: sdkwork-webserver
status: accepted
source: operator
problem: The data plane resolved certificates from configured files and from the control-plane assignment set only. Both are deployment-bound: a Drive-published certificate rotation could not answer a matching SNI name until a control-plane snapshot or a restart, and a host that had both a local file and a Drive-published bundle had no defined precedence.
goals:
  - Add the SDKWork Drive certificate authority as a first-class SNI source.
  - Resolve a server name from the Drive root by the bundle leaf's subjectAltName, not by directory name.
  - Prefer a usable Drive bundle over a configured file and over an assigned certificate for the same name.
  - Keep a usable lower source authoritative when the Drive bundle is unusable (expired or not yet valid).
  - Index every handshake from memory, with no per-handshake file I/O or certificate parsing.
  - Reuse parsed bundles across refreshes, and mirror every bundle into a disk cache that survives an unavailable Drive mount.
  - Refresh the Drive index on a bounded interval and on SIGHUP, swapping it atomically.
  - Enable the source automatically when the canonical shared Drive mount exists, so the preferred source is active without repeating the root in two processes.
  - Drop an unusable local certificate file at load time when a usable Drive bundle covers every name that file declared.
  - Report the winning source as drive in the served-certificate report.
non_goals:
  - A new Drive HTTP API or a cross-repository Drive certificate contract; the source reads the host-shared Drive certificate root.
  - Drive-side certificate issuance, renewal, revocation, or authorization; those remain owned by the certificate control plane.
  - Changing the resolution outcome for a name no usable Drive bundle covers; it still fails closed (the failure may move from compile time to listener load/startup when the Drive source is configured).
users:
  - Platform operators
  - Site reliability engineers
acceptance_criteria:
  - The source is enabled when SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ROOT is set or the canonical /opt/deploy/drive mount exists; SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ENABLED=false disables it in the data plane and the certificate distributor alike. The container entrypoint exports the canonical root when /opt/deploy exists and creates the shared bundle directory for the service user.
  - A Drive bundle is indexed under every exact name and leading wildcard name in its leaf subjectAltName; a name the leaf does not cover never resolves to it.
  - A usable Drive bundle wins a name a configured file and an assigned certificate also claim.
  - An expired Drive bundle yields to a usable configured file or assigned certificate and is still served when nothing else covers the name.
  - A typed configuration with an absolute missing certificate path compiles when the Drive source is configured; the data plane then drops the file if a usable Drive bundle covers every name it declared, and fails closed otherwise.
  - An unreadable, malformed or expired configured certificate file is dropped at load time when a usable Drive bundle covers every name that file declared; otherwise the original failure is returned and the listener fails closed. A certificate that also declares regex SNI names is never dropped for Drive, which cannot carry regex names.
  - The module-import bootstrap check does not fail a production-like deployment for missing local certificate material when the Drive source is configured; the handshake still fails closed for a name no usable Drive bundle covers.
  - Bundle directories are ordinary bounded path components; symlinked roots, directories, and files are refused, and oversized or key-mismatched material is skipped.
  - A malformed rotation keeps the previous parsed bundle instead of taking the name dark.
  - A bundle removed from the Drive root stops answering its names on the next refresh.
  - The certificate distributor mirrors each assigned bundle into the Drive root, writes a node-owner marker, and removes a superseded bundle it owns immediately; a bundle owned by another node or carrying no marker is left untouched.
  - A Drive mirror write or prune failure keeps the local material root and snapshot authoritative and is logged instead of failing distribution.
  - An unchanged refresh neither swaps the index nor raises its revision; a changed fingerprint set swaps it atomically.
  - The disk cache is written atomically with owner-only permissions, is restored when the Drive root is unavailable (missing or unreadable), and prunes every unreferenced entry on a successful scan; a readable but empty root clears the index instead of restoring the cache.
  - The served-certificate report attributes a Drive-resolved name to source drive, and the node daemon counts it as satisfying the assignment.
non_functional_requirements:
  security: Drive material is read only from the configured root; a symlink anywhere in the bundle path is refused, sizes are bounded, and private-key bytes never enter logs, metrics, configuration, or diagnostics. A Drive bundle can never outrank a usable lower source while outside its validity window.
  privacy: The cache metadata contains only the bundle identifier, public fingerprint, public server names, validity window, and a timestamp.
  performance: One handshake performs an indexed lookup against an atomically loaded in-memory index; an unchanged refresh costs one directory scan and no certificate parsing, and a changed bundle is parsed once and reused.
  reliability: A rejected or unavailable Drive scan retains the last-known-good index and the disk cache; an unusable local certificate file never blocks a listener that Drive can fully cover, while a name no usable Drive bundle covers still fails closed.
traceability:
  prd: PRD-https-and-certificates
  specs:
    - ../sdkwork-specs/CONFIG_SPEC.md
    - ../sdkwork-specs/SECURITY_SPEC.md
    - ../sdkwork-specs/RUST_CODE_SPEC.md
    - ../sdkwork-specs/TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core/src/drive_certificates.rs
    - crates/sdkwork-webserver-core/src/runtime_config.rs
    - crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/drive_certificates.rs
    - crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/tls.rs
    - crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/tls_resolver.rs
    - crates/sdkwork-webserver-core/src/tls_runtime/served_certificate.rs
```
