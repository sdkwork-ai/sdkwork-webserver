# SDKWork Web Server Drive Provider Specs

Machine-readable integration authority: [component.spec.json](component.spec.json).

The crate adapts Drive to the Web Server website resource and static-content provider ports.

Transport (MODULE_SPEC.md §4.2, in-process first):

- Default: `src/in_process.rs` calls Drive's own services on the shared PostgreSQL pool
  (`SqlDriveWebsiteRootService`, `SqlDriveResourceResolutionService`,
  `DriveObjectStoreRuntime`). Tenant scope comes from the deployment identity bound to the Web
  Node's tenant scope; the ACL and eligibility predicate are Drive's, never re-implemented here.
- Explicit split deployment: `src/http_adapter.rs` keeps the generated Drive Internal SDK
  transport, selected only by `SDKWORK_WEBSERVER_DRIVE_PROVIDER_MODE=http` and validated at
  startup by the gateway (see `specs/backend-module-call-boundary.allow.json`).

The injected client resolver is bound to one exact tenant scope; it never infers credential
authority from runtime-set membership. The in-process transport reads Drive through Drive's public
ports, so this crate still never writes Drive SQL or derives object-store coordinates itself.
