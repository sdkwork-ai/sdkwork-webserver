# SDKWork Web Node Daemon

The Web Node Daemon retrieves node-scoped Nginx and certificate bundles, materializes them through `sdkwork-webserver-edge-runtime`, performs a real Nginx reload, and advances its durable observed sync generation only after activation succeeds. The crate and binaries carry the canonical `sdkwork-webserver-*` application identity (`sdkwork-webserver-agent` crate, `sdkwork-webserver-node-daemon` canonical binary, `sdkwork-webserver-agent` v3 compatibility binary).

Runtime configuration is private process configuration. `SDKWORK_WEBSERVER_NODE_TOKEN`, `SDKWORK_WEBSERVER_NODE_SYNC_INTERVAL_SECS`, `SDKWORK_WEBSERVER_NODE_STATE_PATH`, and `SDKWORK_WEBSERVER_NODE_STATE_DIR` are the preferred names. The corresponding `SDKWORK_WEBSERVER_AGENT_*` keys remain deprecated aliases; conflicting preferred and legacy values fail startup. `SDKWORK_WEBSERVER_EDGE_ROOT` remains the shared durable parent fallback. The default follows the SDKWork `webserver` application data directory for the host platform; repository `.sdkwork/` and temporary directories are not runtime-state authorities.

The state file is bounded, checksummed, written atomically, and rejects corruption and symlinks. The default state file is `sdkwork-webserver-agent-state.json`; an existing legacy `sdkwork-web-agent-state.json` in the same directory is renamed automatically on first load after upgrade, so durable sync state survives the package transition without a full re-sync. A desired generation that differs from the observed generation means an earlier activation did not complete; the next synchronization request sends only the observed version so the control plane returns a complete bundle for deterministic reapplication.

Before loading state, the daemon acquires the non-blocking kernel lock `sdkwork-webserver-node-daemon.lock` in the state directory and retains it for the process lifetime. A second daemon using that directory fails startup. The retained empty file is not ownership evidence; the live operating-system lock is authoritative and is released on process exit. Production state directories must use a node-local filesystem rather than an unverified network/distributed mount.

The v3 `AgentToken` OpenAPI/generator contract does not yet expose a typed Rust credential provider. The existing transport remains a tracked security-contract gap and must not be represented as completed SDK integration until the reviewed Node Credential backend API and generated SDK changes land.

The packaged and development default is `sdkwork-webserver-node-daemon`. The
`sdkwork-webserver-agent` binary is retained only as a v3 compatibility alias and
must not be used in new deployment documentation.

```powershell
cargo run -p sdkwork-webserver-agent --bin sdkwork-webserver-node-daemon
cargo test -p sdkwork-webserver-agent
```
