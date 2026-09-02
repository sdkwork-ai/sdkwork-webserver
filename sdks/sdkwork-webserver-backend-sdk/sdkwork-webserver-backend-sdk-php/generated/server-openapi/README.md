# sdkwork-webserver-backend-sdk (PHP)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
composer require sdkwork/web-backend-sdk
```

## Quick Start

```php
<?php

use SDKWork\Web\BackendSdk\SdkworkBackendClient;
use SDKWork\Web\BackendSdk\SdkConfig;


$config = new SdkConfig(baseUrl: 'http://localhost:3800');
$client = new SdkworkBackendClient($config);
$$result = $client->nginx->statusRetrieve();


var_dump($result);
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```php
<?php

use SDKWork\Web\BackendSdk\SdkworkBackendClient;
use SDKWork\Web\BackendSdk\SdkConfig;

$config = new SdkConfig(baseUrl: 'http://localhost:3800');
$client = new SdkworkBackendClient($config);

// Set custom headers
$client->setHeader('X-Custom-Header', 'value');
```

## API Modules

- `$client->application` - application API
- `$client->applicationDomain` - application_domain API
- `$client->certificate` - certificate API
- `$client->domain` - domain API
- `$client->applicationSourceVersion` - application_source_version API
- `$client->applicationDeployment` - application_deployment API
- `$client->certificateDistribution` - certificate_distribution API
- `$client->nginx` - nginx API
- `$client->server` - server API
- `$client->serverFile` - server_file API
- `$client->agent` - agent API
- `$client->audit` - audit API

## Usage Examples

### application

```php
<?php

// List managed applications
$params = ['page' => 1, 'page_size' => 2, 'application_type' => 'WEB', 'site_type' => 4, 'status' => 5, 'keyword' => 'keyword'];
$result = $client->application->applicationsList($params);
var_dump($result);
```

### application_domain

```php
<?php

// List application domains
$applicationId = '1';
$params = ['page' => 1, 'page_size' => 2];
$result = $client->applicationDomain->applicationsDomainsList($applicationId, $params);
var_dump($result);
```

### certificate

```php
<?php

// List canonical certificates
$params = ['page' => 1, 'page_size' => 2, 'domain_id' => '00000000-0000-0000-0000-000000000001'];
$result = $client->certificate->certificatesList($params);
var_dump($result);
```

### domain

```php
<?php

// List tenant custom domain assets
$params = ['page' => 1, 'page_size' => 2];
$result = $client->domain->domainsList($params);
var_dump($result);
```

### application_source_version

```php
<?php

// List immutable application source versions
$applicationId = '1';
$params = ['page_size' => 1, 'cursor' => 'cursor'];
$result = $client->applicationSourceVersion->applicationsSourceVersionsList($applicationId, $params);
var_dump($result);
```

### application_deployment

```php
<?php

// List application deployments
$applicationId = '1';
$params = ['page_size' => 1, 'cursor' => 'cursor', 'status' => 3];
$result = $client->applicationDeployment->applicationsDeploymentsList($applicationId, $params);
var_dump($result);
```

### certificate_distribution

```php
<?php

// List certificate manifest convergence by server
$params = ['page' => 1, 'page_size' => 2];
$result = $client->certificateDistribution->certificatesDistributionList($params);
var_dump($result);
```

### nginx

```php
<?php

// Retrieve Nginx status
$result = $client->nginx->statusRetrieve();
var_dump($result);
```

### server

```php
<?php

// List managed servers
$params = ['page_size' => 1, 'cursor' => 'cursor'];
$result = $client->server->serversList($params);
var_dump($result);
```

### server_file

```php
<?php

// List Server Files deployment nodes
$result = $client->serverFile->serverFilesNodesList();
var_dump($result);
```

### agent

```php
<?php

// Retrieve the Nginx configuration and certificate bundle
$params = ['if_sync_version' => 'if-sync-version'];
$result = $client->agent->retrieve($params);
var_dump($result);
```

### audit

```php
<?php

// List audit logs
$params = ['page_size' => 1, 'cursor' => 'cursor', 'target_type' => 'target-type', 'action' => 'action', 'operator_id' => '1', 'start_date' => '2026-04-10T00:00:00Z', 'end_date' => '2026-04-10T00:00:00Z'];
$result = $client->audit->logsList($params);
var_dump($result);
```

## Error Handling

```php
<?php

use SDKWork\Web\BackendSdk\SdkworkBackendClient;
use SDKWork\Web\BackendSdk\SdkConfig;


$config = new SdkConfig(baseUrl: 'http://localhost:3800');
$client = new SdkworkBackendClient($config);

try {
    $client->nginx->statusRetrieve();
} catch (\Throwable $e) {
    echo "Error: {$e->getMessage()}\n";
}
```

## Publishing

This SDK includes cross-platform publish scripts in `bin/`:
- `bin/publish-core.mjs`
- `bin/publish.sh`
- `bin/publish.ps1`

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

> Set `PHP_RELEASE_TAG` (or `SDKWORK_RELEASE_TAG`) for Composer/Packagist tag-based release.

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
