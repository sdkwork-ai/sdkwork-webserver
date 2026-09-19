# SDKWork Webserver PC

Standalone Web Server management application with isolated tenant Console and backend-admin operations surfaces.

The Console renders the canonical deployment-owned application, domain, and certificate pages through `@sdkwork/deployments-app-sdk` (package bytes through `@sdkwork/drive-app-sdk`) and hosts the plugins, skills, and MCP self-service surfaces. The Admin surface manages Nginx configuration, servers, diagnostics, storage providers, and audit evidence through `@sdkwork/webserver-backend-sdk`. Machine-to-machine agent heartbeat and sync endpoints are intentionally not exposed as operator commands.

Use `pnpm --dir apps/sdkwork-webserver-pc dev` for local development and `pnpm --dir apps/sdkwork-webserver-pc check` for the application verification boundary.

