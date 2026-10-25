# Web Server Scripts

`webserver-dev.mjs` is the standalone gateway runner selected by
`specs/topology.spec.json`. It resolves the tracked topology profile, private PostgreSQL
development profile, IAM database mapping, and application manifest roots before starting the
Rust gateway.

IAM tenant application provisioning is automatic and fail-closed during standalone gateway
startup. The Rust gateway delegates reconciliation to
`sdkwork-iam-embedded-application-bootstrap`; scripts do not issue raw IAM bootstrap HTTP calls.

Use `pnpm check:iam-application-bootstrap` for the repository contract. The privileged
`pnpm admin:bootstrap:app` command exists for an operator to provision a remote/cloud environment
from `agent.app.config.json`; it is not invoked by browser startup or `dev:cloud`.
