# SDKWork Webserver PC

Standalone Web Server management application with isolated tenant Console and backend-admin operations surfaces.

The Console manages sites, configuration, domains, certificates, deployments, and health checks through `@sdkwork/webserver-app-sdk`. The Admin surface manages Nginx configuration, servers, diagnostics, and audit evidence through `@sdkwork/webserver-backend-sdk`. Machine-to-machine agent heartbeat and sync endpoints are intentionally not exposed as operator commands.

Use `pnpm --dir apps/sdkwork-webserver-pc dev` for local development and `pnpm --dir apps/sdkwork-webserver-pc check` for the application verification boundary.

