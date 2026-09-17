# SDKWork Webserver H5 Core

Runtime core of the SDKWork Web Server mobile browser console
(`apps/sdkwork-webserver-h5`).

## Responsibility

One thing only: turn the materialized browser runtime configuration plus the
persisted dual-token session into the generated SDK clients the console
surfaces consume, and publish them through typed exports.

- `sdk/` — runtime-env contract, base-URL resolution (`ENVIRONMENT_SPEC.md`
  §6.3 protocol adaptation), generated client construction, list-page mapping.
- `session/` — the single owner of the persisted dual-token session and the
  `AuthTokenManager` the generated clients accept.
- `composition/` — the SDK inventory / module / host registries required by
  `APP_COMPOSITION_SPEC.md` §3.

Feature packages MUST NOT construct SDK clients or read credentials; they
receive an injected client from this package (`APP_SDK_INTEGRATION_SPEC.md` §2).

## Verification

```bash
pnpm --filter @sdkwork/webserver-h5 typecheck
pnpm --filter @sdkwork/webserver-h5 test
```
