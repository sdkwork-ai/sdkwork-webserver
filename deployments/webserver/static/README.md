# Static fallback root (process-owned)

Authority: `SDKWORK_DEPLOY_SPEC.md` §8 (`static-fallback` exception for
`sdkwork-webserver` `expose.mode: api`), process `AdaptiveAppShell`.

Packaged to `/usr/share/sdkwork/webserver/web/static/` (or container
`/app/share/sdkwork/webserver/web/static/`) and selected when neither the PC
nor H5 SPA root is available. Ordinary file serving (no SPA rewrite to
`index.html`). Edge nginx does **not** mount this root.
