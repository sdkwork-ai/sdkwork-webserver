# REQ-2026-0062 Owner-Scoped Console Release And TLS Workflow

```yaml
id: REQ-2026-0062
title: Publish owner-scoped, Drive-backed application source versions with domains and certificates
owner: sdkwork-webserver
status: in-progress
source: user
problem: Authenticated app users can enter the Web Server Console, but a page-level IAM denial blocks normal workflows and the existing deployment form does not upload an immutable package. Certificate workflows must expose verified owned domains through a bounded selector without hiding later pages or leaking another user's resources. Administrators and normal users need separate product surfaces without hiding the shared Console shell or sign-out command.
goals:
  - Keep the complete Console shell and sign-out command visible for every authenticated account, with resource-level unauthorized states for unavailable capabilities.
  - List and mutate only sites owned by the authenticated app user, including configuration, domains, deployments, and certificates.
  - Persist every ZIP, local-directory, and Git source as an immutable application source version in SDKWork Drive before creating a release record.
  - Import public HTTPS Git repositories on the server through the Drive uploader service with the same bounded application-source policy as browser-selected sources.
  - Keep source versions and release records as separate resources so one stored source version can be released, audited, or rolled back independently.
  - Retain the newest five ready source artifacts by default, allow a bounded per-application override, and preserve release audit records when old artifacts are pruned.
  - Discover and report `sdkwork.app.config.json` and `etc/sdkwork.deployment.config.json` from each stored source version.
  - Apply one bounded application-source policy to browser directory packages and uploaded ZIP archives before Drive extraction.
  - Keep source preparation, upload, archive inspection, and extraction cancellable or fail-closed without allowing duplicate UI submission.
  - Persist a stable Drive resource URI, package size, lowercase SHA-256 digest, source type, original source reference, environment, version metadata, and idempotency key without storing signed URLs, object keys, or provider credentials.
  - Select 1..8 verified certificate domains from the currently selected owned application through server pagination, preserve selections across pages, and filter certificate queries by that application at the repository boundary.
  - Show deployment history and asynchronous status truthfully, and permit rollback only from a successful deployment while preserving artifact identity.
  - Route Web Server administrators to the isolated `/admin` surface and normal app users to `/console`.
non_goals:
  - Copying Drive upload APIs into the Web App SDK or replacing generated SDK calls with raw HTTP.
  - Treating package upload or deployment-command acceptance as proof that a version is serving traffic.
  - Storing signed delivery URLs, storage-provider object keys, access tokens, certificate private keys, or ambient tenant/user identity in deployment requests.
  - Using deployment records as the source-version catalog or deleting release history when an artifact exceeds retention.
  - Replacing Drive archive validation, malware scanning, retention, quota, or access-control authority with browser-only checks.
users:
  - application owners publishing Web and API applications
  - application owners configuring custom domains and TLS certificates
  - tenant Web Server administrators
acceptance_criteria:
  - An app user sees only their own applications; a second user in the same tenant cannot list or mutate the first user's applications, domains, deployments, or certificates.
  - Certificate list accepts an optional siteId, verifies owner access, and performs owner plus site filtering in SQL.
  - Certificate issuance rejects a domain owned by another app user in the same tenant.
  - The certificate domain selector requests one bounded page at a time through the generated App SDK, excludes unverified domains, preserves selected ids and labels across page changes, and exposes loading, empty, failure, and retry states.
  - For ZIP and directory sources, the Console release action requires a non-empty archive, reports upload progress, computes SHA-256, uploads through Drive, and submits the stable Drive URI and artifact metadata through the generated Web App SDK.
  - Application creation and redeployment offer ZIP archive, local directory, and Git repository as mutually exclusive source modes; all three modes create a ready source-version record with a stable Drive URI before release creation.
  - Git repository input accepts only an absolute HTTPS URL with a non-root repository path, rejects embedded credentials, query parameters, fragments, HTTP URLs, and values longer than 500 characters, and reports source-specific validation errors without invalidating a populated version.
  - Git source import performs a non-interactive bounded shallow clone, excludes VCS metadata, produces a deterministic ZIP, writes it through the Drive uploader service, and never exposes repository credentials in URLs, logs, configuration, or frontend state.
  - Directory packaging applies root and nested `.gitignore` files with Git-compatible ordering, negation, anchoring, escaping, and ignored-parent semantics.
  - Directory and ZIP source packages exclude VCS metadata directories and reject unsafe, duplicate, excessively deep, excessively long, or control-character paths before extraction.
  - Application source extraction is bounded to at most 500 files, 16 MiB per file, and 64 MiB total uncompressed content, matching the active Drive extraction profile.
  - Uploaded ZIP archives are inspected through the generated Drive App SDK and only validated file entry paths are submitted to Drive extraction.
  - The client rejects incomplete archive listings and mismatched extraction counts before creating a deployment command.
  - Source-version responses expose source type, business version, original source reference, commit hash, stable Drive identity, size, digest, configuration discovery state, lifecycle status, and retention state.
  - A release request references `sourceVersionId`; the service copies the immutable source facts into the deployment row for audit and backward-compatible execution.
  - Applications default `sourceVersionRetentionLimit` to 5 in typed deployment configuration; values outside 1 through 50 are rejected.
  - Retention marks older source versions pruned and requests Drive lifecycle deletion without deleting deployment records; rollback is rejected when the selected release no longer has a retained artifact.
  - The creation workflow separates application basics, store media, source version, deployment configuration, and review; a populated version remains valid when source-specific validation reports an error.
  - A release dialog cannot submit twice or close while a command is running; unmounting aborts an active multipart upload.
  - When application creation succeeds but source storage or initial deployment creation fails, the UI reports the recoverable draft state without exposing stack traces or provider details.
  - Deployment responses expose environment, version and source metadata, artifact identity, status, start/completion timestamps, and duration.
  - Rollback records inherit the selected deployment's version, source, and artifact fields and are unavailable in the Console unless the selected deployment succeeded.
  - The Console shell and sign-out command remain visible without Web permissions; unavailable resources show an IAM access state.
  - Admin permission scope lands on `/admin`; non-admin access to `/admin/*` redirects to `/console`.
  - No Console module constructs authentication headers, parses credentials, or calls remote APIs outside injected generated/composed SDK clients.
non_functional_requirements:
  security: Owner scope is enforced by service and repository boundaries, not by frontend filtering. Drive and Web SDK clients share the IAM bootstrap TokenManager.
  privacy: Cross-owner data is not returned even when users share a tenant.
  performance: Application, domain, deployment, certificate, and certificate-option queries remain store-paginated without browser-side all-page aggregation; browser uploads use the Drive multipart uploader; raw directory selection and archive path depth are bounded; repeated ancestor ignore checks are cached.
  reliability: Deployment state remains pending until an execution authority advances it; retries use an idempotency key and content fingerprint, incomplete extraction fails closed, source versions are immutable, and rollbacks preserve immutable artifact provenance.
affected_surfaces:
  - api
  - sdk
  - backend
  - pc
  - iam
  - deployment
trace:
  specs:
    - API_SPEC.md
    - PAGINATION_SPEC.md
    - SDK_SPEC.md
    - APP_SDK_INTEGRATION_SPEC.md
    - IAM_SPEC.md
    - SECURITY_SPEC.md
    - SUPPLY_CHAIN_SECURITY_SPEC.md
    - DEPLOYMENT_SPEC.md
    - FRONTEND_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-routes-webserver-app-api
    - crates/sdkwork-intelligence-webserver-service
    - crates/sdkwork-intelligence-webserver-repository-sqlx
    - crates/sdkwork-webserver-source-provider
    - sdks/sdkwork-web-app-sdk
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-core
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-commons
verification:
  - cargo test -p sdkwork-intelligence-webserver-service
  - cargo test -p sdkwork-routes-webserver-app-api
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test repository_parity postgres_repository_transactions_tenants_idempotency_and_pagination_are_bounded -- --ignored --exact
  - pnpm sdk:generate:check
  - pnpm --dir apps/sdkwork-webserver-pc check
  - node ../sdkwork-specs/tools/check-app-sdk-consumer-imports.mjs --workspace .
  - node ../sdkwork-specs/tools/check-permission-composition.mjs --workspace .
  - pnpm --dir apps/sdkwork-webserver-pc test
  - pnpm --dir apps/sdkwork-webserver-pc typecheck
  - pnpm --dir apps/sdkwork-webserver-pc build
  - pnpm check:repository-docs
```

## Decision

The Web Server remains the authority for applications, source-version metadata, domains, certificates, typed deployment configuration, and release records. Drive owns source bytes, archive safety, object lifecycle, ACLs, scanning, and stable resource identity. Browser-selected ZIP and directory sources use the generated Drive App SDK; server-imported Git sources use the Drive uploader service directly. A deployment references one ready source version and preserves an immutable artifact snapshot for execution, audit, and rollback. A separate deployment execution authority is still required to advance accepted records from pending to running, successful, or failed; until that authority reports evidence, the Console must show the command as pending rather than published.
