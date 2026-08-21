# REQ-2026-0053 Web Node Daemon Terminology And Compatibility Migration

```yaml
id: REQ-2026-0053
title: Replace the ambiguous edge Agent product term with Web Node Daemon terminology
owner: sdkwork-webserver
status: accepted
source: product-language-and-operational-clarity
problem: The host process that synchronizes Web Server configuration is named Agent across product text, runtime configuration, APIs, SDKs, and code. In the SDKWork product family, Agent also denotes AI agents, so operational conversations, alerts, permissions, and generated SDK methods are ambiguous.
goals:
  - Use Web Node Daemon as the canonical product and process term and Web Node as the managed resource term.
  - Use Node Credential or Node Token for node bootstrap authentication and reserve Agent for AI-agent capabilities.
  - Introduce preferred NODE runtime configuration without disconnecting deployed v3 processes.
  - Ship `sdkwork-web-node-daemon` as the canonical packaged/development entrypoint while retaining `sdkwork-web-agent` as an explicit v3 compatibility alias.
  - Define a versioned migration for crate/binary, API, SDK, security scheme, state, metadata, packaging, and operations names.
non_goals:
  - Renaming or deleting the existing v3 Agent API, generated SDK output, crate, binary, state filename, or persisted metadata keys without a reviewed compatibility window.
  - Treating a text-only rename as node acknowledgement, inventory convergence, readiness, or HA evidence.
  - Hand-editing generated SDK artifacts.
acceptance_criteria:
  - Product documentation defines one canonical dictionary and identifies every retained Agent name as a legacy compatibility identifier.
  - SDKWORK_WEBSERVER_NODE_TOKEN, SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS, SDKWORK_WEBSERVER_NODE_STATE_PATH, and SDKWORK_WEBSERVER_NODE_STATE_DIR are preferred runtime keys.
  - Corresponding SDKWORK_WEBSERVER_AGENT aliases remain readable during the v3 window; conflicting preferred and legacy values fail startup.
  - Existing default state location and filename remain unchanged so an upgrade cannot silently lose the last observed generation.
  - A future public migration uses a new API major version, additive generated SDK family output, explicit deprecation metadata, and measured legacy usage before removal.
  - AI-agent naming is not reused for Web Node Daemon concepts in newly authored product contracts.
  - Both binary names resolve to the same bounded implementation and durable state contract; the compatibility alias cannot create a second synchronization authority.
non_functional_requirements:
  security: Alias conflicts fail closed, credentials are never logged, and no tracked source profile contains a real token.
  reliability: Existing nodes retain their credential, state path, sync generation, and v3 endpoint until an explicit rollout moves them.
  compatibility: Public v3 fields and routes remain wire-compatible; breaking removal requires human review and a separately accepted migration requirement.
trace:
  specs:
    - NAMING_SPEC.md
    - CONFIG_SPEC.md
    - ENVIRONMENT_SPEC.md
    - API_SPEC.md
    - SDK_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-web-agent
    - crates/sdkwork-routes-webserver-backend-api
    - crates/sdkwork-webserver-contract
    - sdks/sdkwork-web-backend-sdk
verification:
  - cargo test -p sdkwork-web-agent
  - node --test tests/contract/agent-sync-state.contract.test.mjs
  - node ../sdkwork-specs/tools/check-source-config-standard.mjs --root .
  - pnpm verify
```

## Canonical Dictionary

| Concern | Canonical term | Legacy v3 compatibility term |
| --- | --- | --- |
| Managed host | Web Node / Edge Node | server or agent subject |
| Host process | Web Node Daemon | Edge Agent / `sdkwork-web-agent` |
| Host-side apply library | Edge Runtime | unchanged |
| Bootstrap authentication | Node Credential / Node Token | AgentToken / `agentToken` |
| Desired-state delivery | Node Sync Manifest | Agent sync response |
| Liveness publication | Node Heartbeat | Agent heartbeat |
| AI autonomous capability | Agent | reserved for AI-agent domains |

`Daemon` is the product/process role; on Windows it is installed as a service. `Edge Runtime`
remains the in-process filesystem and Nginx activation library and must not be used as the process
name. `Web Node` is preferred when the deployment is not physically at an edge location.

## Compatibility Phases

Phase 1 is implemented in the v3 process. NODE environment keys are preferred, AGENT keys are
deprecated aliases, and unequal dual definitions fail closed. The existing crate, binary, API,
header, response field, metadata keys, durable state schema, and default filename remain unchanged.

The additive binary step is implemented: `sdkwork-web-node-daemon` is the canonical entrypoint used
by development and release smoke commands, while `sdkwork-web-agent` remains an explicit
compatibility alias built from the same library and state machine. This changes no wire contract
and does not create a second daemon loop.

Phase 2 requires human review because it changes public security and generated SDK contracts. It
must add a new major-version Node API and typed Node Credential provider, generate additive Node
SDK methods from the authority source, mark v3 Agent operations deprecated, and keep both routes
backed by one service implementation. Exact paths and operation IDs are selected by API validators;
candidate names in design discussion are not authority.

Phase 3 may rename the packaged binary and migrate persisted metadata/state only after mixed-version
rollout evidence proves that old nodes are no longer active. State migration must detect both names,
reject conflicting files, preserve checksum/revision, and never reset observed generation silently.

Phase 4 may remove v3 names only after telemetry shows zero legacy use for the approved support
window, generated SDK consumers have migrated, rollback packages remain available, and release
notes identify the breaking change.

## Human Review Gate

Phase 1 is additive and accepted. Phase 2 and later require human review for the public API major
version, security-scheme ownership, generated SDK naming, credential response field, binary/package
identity, database metadata migration, deprecation window, and removal criteria.

## Implementation And Verification Evidence

- The v3 binary prefers four `SDKWORK_WEBSERVER_NODE_*` runtime keys, reads the matching
  `SDKWORK_WEBSERVER_AGENT_*` keys as deprecated aliases, and rejects unequal dual definitions.
- The durable default path and `sdkwork-web-agent-state.json` filename remain unchanged; no upgrade
  path can silently reset `observedSyncVersion` merely because terminology changed.
- The tracked environment example uses placeholder Node keys only and the source-config validator
  passes; no credential value was introduced into tracked source.
- `cargo test -p sdkwork-web-agent` passes 9/9 once through the shared daemon library, including
  value/path alias compatibility and conflict failure tests; both binary wrappers compile without
  duplicating the synchronization implementation.
- The combined terminology and state contract tests pass 4/4 and are mandatory in root `test` and
  `verify`.
- `pnpm verify` passes in 240.9 seconds after API materialization, proving phase 1 did not mutate the
  retained v3 public route authority or generated SDK contract.
