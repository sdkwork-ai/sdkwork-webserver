# sdkwork-webserver-backend-sdk

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
npm install @sdkwork/webserver-backend-sdk
# or
yarn add @sdkwork/webserver-backend-sdk
# or
pnpm add @sdkwork/webserver-backend-sdk
```

## Quick Start

```typescript
import { SdkworkBackendClient } from '@sdkwork/webserver-backend-sdk';

const client = new SdkworkBackendClient({
  baseUrl: 'http://localhost:3800',
  timeout: 30000,
});

// Authentication
client.setAuthToken('your-auth-token');
client.setAccessToken('your-access-token');

// Use the SDK
const result = await client.nginx.status.retrieve();
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```typescript
import { SdkworkBackendClient } from '@sdkwork/webserver-backend-sdk';

const client = new SdkworkBackendClient({
  baseUrl: 'http://localhost:3800',
  timeout: 30000, // Request timeout in ms
  headers: {      // Custom headers
    'X-Custom-Header': 'value',
  },
});
```

## API Modules

- `client.application` - application API
- `client.applicationDomain` - application_domain API
- `client.certificate` - certificate API
- `client.domain` - domain API
- `client.applicationSourceVersion` - application_source_version API
- `client.applicationDeployment` - application_deployment API
- `client.certificateDistribution` - certificate_distribution API
- `client.nginx` - nginx API
- `client.server` - server API
- `client.serverFile` - server_file API
- `client.agent` - agent API
- `client.audit` - audit API

## Usage Examples

### application

```typescript
// List managed applications
const params = {
  page: 1,
  page_size: 2,
  application_type: 'WEB',
  site_type: 4,
  status: 5,
  keyword: 'keyword',
};
const result = await client.application.list(params);
```

### application_domain

```typescript
// List application domains
const applicationId = '1';
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.applicationDomain.applications.domains.list(applicationId, params);
```

### certificate

```typescript
// List canonical certificates
const params = {
  page: 1,
  page_size: 2,
  domain_id: 'domain_id',
};
const result = await client.certificate.list(params);
```

### domain

```typescript
// List tenant custom domain assets
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.domain.list(params);
```

### application_source_version

```typescript
// List immutable application source versions
const applicationId = '1';
const params = {
  page_size: 1,
  cursor: 'cursor',
};
const result = await client.applicationSourceVersion.applications.sourceVersions.list(applicationId, params);
```

### application_deployment

```typescript
// List application deployments
const applicationId = '1';
const params = {
  page_size: 1,
  cursor: 'cursor',
  status: 3,
};
const result = await client.applicationDeployment.applications.deployments.list(applicationId, params);
```

### certificate_distribution

```typescript
// List certificate manifest convergence by server
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.certificateDistribution.certificates.distribution.list(params);
```

### nginx

```typescript
// Retrieve Nginx status
const result = await client.nginx.status.retrieve();
```

### server

```typescript
// List managed servers
const params = {
  page_size: 1,
  cursor: 'cursor',
};
const result = await client.server.list(params);
```

### server_file

```typescript
// List Server Files deployment nodes
const result = await client.serverFile.nodes.list();
```

### agent

```typescript
// Retrieve the Nginx configuration and certificate bundle
const params = {
  if_sync_version: 'if_sync_version',
};
const result = await client.agent.sync.list(params);
```

### audit

```typescript
// List audit logs
const params = {
  page_size: 1,
  cursor: 'cursor',
  target_type: 'target_type',
  action: 'action',
  operator_id: 'operator_id',
  start_date: 'start_date',
  end_date: 'end_date',
};
const result = await client.audit.auditLogs.list(params);
```

## Error Handling

```typescript
import { SdkworkBackendClient, NetworkError, TimeoutError, AuthenticationError } from '@sdkwork/webserver-backend-sdk';

try {
  const result = await client.nginx.status.retrieve();
} catch (error) {
  if (error instanceof AuthenticationError) {
    console.error('Authentication failed:', error.message);
  } else if (error instanceof TimeoutError) {
    console.error('Request timed out:', error.message);
  } else if (error instanceof NetworkError) {
    console.error('Network error:', error.message);
  } else {
    throw error;
  }
}
```

## Publishing

This SDK includes cross-platform publish scripts in `bin/`:
- `bin/publish-core.mjs`
- `bin/publish.sh`
- `bin/publish.ps1`

TypeScript check and publish commands use pnpm to materialize workspace dependency versions in a temporary tarball. They reject local-only dependency protocols before npm publication and do not rewrite the source `package.json`.

### Check

```bash
./bin/publish.sh --action check
```

### Publish

```bash
./bin/publish.sh --action publish --channel release
```

```powershell
.\bin\publish.ps1 --action publish --channel test --dry-run
```

> Configure npm registry credentials before release publish.

## License

MIT

## Regeneration Contract

- HTTP/OpenAPI generator-owned files are tracked in `.sdkwork/sdkwork-generator-manifest.json`.
- HTTP/OpenAPI generation also writes `.sdkwork/sdkwork-generator-changes.json` so automation can inspect created, updated, deleted, unchanged, scaffolded, and backed-up files plus the classified impact areas, verification plan, and execution decision for the latest generation.
- HTTP/OpenAPI apply mode also writes `.sdkwork/sdkwork-generator-report.json` with the full execution report, including `schemaVersion`, `generator`, stable artifact paths, and the execution handoff commands that match CLI `--json` output.
- CLI JSON output also includes an execution handoff with concrete next commands, including reviewed apply commands for dry-run flows.
- Put HTTP/OpenAPI hand-written wrappers, adapters, and orchestration in `custom/`.
- Files scaffolded under `custom/` are created once and preserved across HTTP/OpenAPI regenerations.
- If an HTTP/OpenAPI generated-owned file was modified locally, its previous content is copied to `.sdkwork/manual-backups/` before overwrite or removal.
- RPC SDK source workspaces use convention-first evidence by default: RPC SDK family naming, language workspace naming, `rpc/*.manifest.json`, proto source references, generated client source, and native package manifests.
- Use `sdkgen inspect --protocol rpc` to verify RPC convention evidence. Request persisted generator evidence only with `--emit-control-plane` for release, CI, audit, or migration workflows; evidence paths are derived by generator convention.
