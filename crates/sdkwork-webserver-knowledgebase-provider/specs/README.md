# SDKWork Web Server Knowledgebase Provider Specs

`component.spec.json` owns the Knowledgebase integration surface, the Web Server resource and Wiki
provider ports, and focused verification for `sdkwork-webserver-knowledgebase-provider`.

Transport (MODULE_SPEC.md §4.2, in-process first):

- Default: `src/in_process.rs` calls Knowledgebase's own
  `KnowledgeWikiPublicProviderService` — the exact service the Knowledgebase Internal API route
  delegates to — over the shared database, with the module's embedded Drive Wiki source for page
  bytes. No Knowledgebase table is read directly here; its persistence port is
  `sqlx::AnyPool`-typed, so the host supplies the compatibility pool the database framework
  reserves for that driver.
- Explicit split deployment: `src/http_adapter.rs` keeps the generated Knowledgebase Internal SDK
  transport, selected only by `SDKWORK_WEBSERVER_KNOWLEDGEBASE_PROVIDER_MODE=http` and validated at
  startup by the gateway (see `specs/backend-module-call-boundary.allow.json`).

One provider client is valid only for the exact tenant scope configured by bootstrap; a service
credential is never reused across tenant scopes merely because a scope appears in a runtime-set. It
does not mount public HTTP routes or edit generated SDK transport. Global standards remain
authoritative under `../../../sdkwork-specs/`.
