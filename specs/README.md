# Web Server component specs

Local contracts for `sdkwork-webserver`.

Composition closure for the seven same-origin building blocks (webserver backend/internal, IAM,
Drive, Skills, MCP, Sandbox, Deployments) is verified by `pnpm check:composable-architecture`,
which runs the component-port, permission, Rust/frontend composition, resolved-graph, and app SDK
consumer-import validators. `component.spec.json` declares the hosting component, its ports, SDK
dependencies, dependency API surfaces, and the commands that prove the contract.

- `component.spec.json`: component identity and verification entrypoints
- `topology.spec.json`: runtime topology profile vocabulary and env bindings
- `sdkwork.webserver.config.schema.json`: authored application Web Server configuration authority
- `sdkwork.website-runtime.descriptor.schema.json`: strict v1 consumer schema for immutable
  Deploy-compiled Site routing snapshots
- `sdkwork.website-runtime-set.snapshot.schema.json`: strict v1 node-scoped envelope for bounded,
  atomically activated sets of complete Site descriptors
- `sdkwork.tls-runtime.snapshot.schema.json`: strict v1 consumer schema for node-scoped certificate
  assignments and SNI policy
- `sdkwork.website-provider-event-ingress.schema.json`: strict loopback ingress, subscription,
  tenant binding, secret-file reference, replay window, concurrency, and durable checkpoint config
  for Drive WebsiteRoot and Knowledgebase Wiki owner events
- `nginx-gap.catalog.json`: living capability status for the declared `http-core-v1` nginx
  compatibility profile (graded capability level, not a directive-level contract)
- `nginx-directive-inventory.json`: canonical snapshot of the 744 nginx 1.29.6 OSS directives
  (contexts, argument grammar, source module, declared reference build), reproduced from the nginx
  source tree by `tools/gen-nginx-directive-inventory.mjs`
- `nginx-directive-support.catalog.json`: per-directive support classification for `http-core-v1`
  (`executable` / `accepted-unenforced` / `unsupported-fail-closed` / `excluded`), runtime field,
  importer mapping, risk, migration impact/action, known differences and executable evidence; kept
  in sync with the implementation by `tools/check-nginx-directive-matrix.mjs` and rendered as
  `docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`
- `nginx-behavioral-corpus.manifest.json`: index of the executed nginx behavioural/differential
  slices under `tests/nginx/` (case floors, reference binary, orchestrator per slice)
