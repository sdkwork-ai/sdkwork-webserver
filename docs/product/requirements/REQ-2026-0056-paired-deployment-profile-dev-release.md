# REQ-2026-0056 Paired Deployment-Profile Development And Release Contracts

```yaml
id: REQ-2026-0056
title: Pair standalone and cloud development and immutable server package commands
owner: sdkwork-webserver
status: accepted
source: sdkwork-profile-command-and-release-alignment
problem: The root development command did not expose explicit standalone and cloud profiles, while the release workflow declared a tar.gz target without a profile-paired archive producer. A cloud command could therefore accidentally bootstrap local infrastructure, and a release could publish a raw binary glob or use a version different from the workflow matrix.
goals:
  - Make bare dev delegate directly to standalone development and expose an explicit cloud development command.
  - Resolve cloud development from the topology profile with explicit remote HTTPS surfaces and start only the local Web Node Daemon.
  - Provide paired standalone and cloud immutable Linux x64/arm64 server archive commands and workflow targets.
  - Bind archive identity to the workflow package version and fail closed on conflicting compatibility version inputs.
  - Bound, checksum, and deterministically order package content without tracking credentials.
non_goals:
  - Claiming an actual Linux archive was executed or installed from Windows dry-run evidence.
  - Claiming an enabled container registry publication, production install, upgrade, rollout, or rollback without external release evidence.
  - Starting a local database, application API process, or gateway for cloud development.
acceptance_criteria:
  - dev delegates directly to dev:standalone; dev:standalone resolves standalone.development and dev:cloud resolves cloud.development.
  - The cloud profile contains one remote HTTPS backend API origin, contains no token, and the runner starts only the canonical Web Node Daemon binary.
  - Tracked Node Daemon environment examples keep SDKWORK_WEBSERVER_NODE_TOKEN blank.
  - release:package:standalone and release:package:cloud produce distinct canonical artifact names from SDKWORK_PACKAGE_VERSION.
  - The workflow planner exposes Linux x64 and arm64 server tar.gz targets for each supported deployment profile with archive, checksum, signature, and SBOM output globs.
  - Packaging is restricted to a matching Linux architecture and uses a confined temporary stage, deterministic tar metadata, a per-file SHA-256 manifest, atomic archive rename, a SHA-256 sidecar, and a 512 MiB archive ceiling.
non_functional_requirements:
  security: No tracked profile or packaged example contains a Node Token; paths remain confined and release version conflicts fail closed.
  reliability: Profile selection and artifact identity are explicit and immutable; cloud development cannot silently fall back to local infrastructure.
  reproducibility: Archive member order, timestamp, owner, group, manifest order, file hashes, and artifact name are deterministic inputs.
trace:
  specs:
    - CONFIG_SPEC.md
    - ENVIRONMENT_SPEC.md
    - DEPLOYMENT_SPEC.md
    - PNPM_SCRIPT_SPEC.md
    - GITHUB_WORKFLOW_SPEC.md
    - TEST_SPEC.md
  components:
    - scripts/webserver-release.mjs
    - etc/sdkwork.deployment.config.json
    - specs/topology.spec.json
    - sdkwork.workflow.json
verification:
  - node --test tests/contract/deployment-profile-commands.contract.test.mjs tests/contract/dev-runner.contract.test.mjs
  - node ../sdkwork-github-workflow/scripts/sdkwork-workflow.mjs validate --config sdkwork.workflow.json
  - node ../sdkwork-github-workflow/scripts/sdkwork-workflow.mjs matrix --config sdkwork.workflow.json --json
  - node ../sdkwork-specs/tools/check-source-config-standard.mjs --root .
  - node ../sdkwork-specs/tools/check-pnpm-script-standard.mjs --root . --product-prefix web
  - pnpm verify
```

## Runtime Profile Boundary

`sdkwork.app.config.json` declares that the application supports `standalone` and `cloud`; it does
not own concrete origins, ports, credentials, or process plans. `etc/sdkwork.deployment.config.json`
selects the typed profile. Bare `pnpm dev` enters `standalone.development`. `pnpm dev:cloud` resolves
`etc/topology/cloud.development.env`, resolves the explicit remote application/backend/platform
surfaces, and launches only the local Web Node Daemon against the deployed control plane.

The cloud runner reads the Node Token only from the local process environment. Dry-run reports
whether a token is configured but never emits its value. An actual cloud run fails before spawning
the daemon when neither the preferred Node Token nor the v3 compatibility alias is present.

## Immutable Package Boundary

The reusable workflow injects `SDKWORK_DEPLOYMENT_PROFILE` and `SDKWORK_PACKAGE_VERSION` for every
matrix target. The release script uses those exact values, rejects disagreement with the legacy
`SDKWORK_RELEASE_VERSION`, builds release binaries, writes a sorted content manifest, and atomically
publishes the bounded archive plus checksum. The packaged environment example uses the canonical
`etc/node-daemon/` path while the repository retains `etc/agent/` as the v3 compatibility alias.

The archive producer intentionally fails when the host OS/architecture does not match the target.
REQ-2026-0058 and REQ-2026-0059 own Linux x64/arm64 archive creation, checksum validation,
extraction, startup, HTTP/HTTPS readiness/traffic, shutdown, and cleanup evidence.

REQ-2026-0057 owns the stronger frozen-workspace and bounded archive trust-boundary follow-up. It
does not expand this requirement's deployment-profile scope or convert fixture validation into
Linux runtime evidence.

## Verification Evidence

- The deployment-profile and development-runner contract tests validate SDKWork app lifecycle
  commands, four topology profiles, and cloud Node Daemon-only process ownership.
- The SDKWork workflow validator accepts cloud/standalone Linux x64/arm64 server targets.
- Source-config and pnpm-script validators pass; the tracked Node Token value is blank.
- Both profile package dry-runs resolve workflow version `9.8.7` to distinct canonical artifact
  names, and conflicting current/legacy version inputs fail closed.
- On Windows, a non-dry-run package request fails at the matching Linux host gate before source recovery
  or build execution.
- Current verification counts and duration are intentionally not embedded here; CI output is the
  evidence authority.
- No production publication, installation, or rollout is claimed while application release packages
  remain disabled and deferred.
