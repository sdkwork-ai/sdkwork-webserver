# SDKWork Webserver Mini Program Core

Runtime core of the SDKWork Web Server WeChat mini program
(`apps/sdkwork-webserver-mini-program`).

## Responsibility

One thing only: turn the selected mini program runtime profile plus the
persisted dual-token session into the generated SDK clients the capability
packages consume, and publish them through typed exports.

- `sdk/` — runtime-env contract (`SDKWORK_*` profile keys), generated client
  construction, and list-page mapping.
- `session/` — the single owner of the persisted dual-token session, the
  platform storage bridge it writes through, the `AuthTokenManager` the
  generated clients accept, and the sensitive-state clearing registry.
- `composition/` — the SDK inventory / module / host registries required by
  `APP_COMPOSITION_SPEC.md` §3.

Capability packages MUST NOT construct SDK clients, read credentials, or touch
platform storage; they receive an injected client from this package
(`APP_SDK_INTEGRATION_SPEC.md` §2, `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §6).

## Verification

```bash
pnpm --dir apps/sdkwork-webserver-mini-program typecheck
pnpm --dir apps/sdkwork-webserver-mini-program test
```
