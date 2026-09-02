# SDKWork Web Node Daemon Specs

`component.spec.json` declares the Web Node synchronization process, its local runtime provider,
legacy v3 Agent API dependency, canonical and compatibility binary entrypoints, and runtime
configuration keys.

The daemon is a host process rather than an SDK family or HTTP route owner. New deployment
surfaces use `sdkwork-webserver-node-daemon`; `sdkwork-webserver-agent` remains an explicit v3
compatibility identifier. Verify from the repository root with its Cargo tests and strict
component-port validation.
