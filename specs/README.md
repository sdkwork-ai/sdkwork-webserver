# Web Server component specs

Local contracts for `sdkwork-webserver`.

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
