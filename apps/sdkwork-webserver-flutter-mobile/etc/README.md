# Component Deployment

This application surface shares the enclosing application deployment unit.
Deployment profiles are owned by `../../../etc/sdkwork.deployment.config.json`; runtime process topology is owned by `../../../specs/topology.spec.json`.
Surface-local build and test commands stay in this application root.

## Materialization

`materialization.format` is `dart-define-json`: the Flutter runtime reads its
environment through `String.fromEnvironment`, so the materialized artifact is a
flat JSON object of `--dart-define` pairs rather than a nested document.

Materialization is **declared but not wired in this repository yet** — see
"Blocking Prerequisites" in `../README.md`. Until
`etc/client-env.materialization.json` and a `workflow:materialize-client-env`
script exist in the repository root, the committed `env/sdkwork.<profile>.json`
matrix is the source of truth and is checked against the parent deployment index
by the surface contract test.
