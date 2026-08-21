# Adaptive Web helpers (module-local entry)

Canonical plan-folding for modules that emit Adaptive Web on stock nginx:

`../../sdkwork-specs/tools/webserver/adaptive-web.mjs`

Reference nginx snippets (other modules, `expose.mode` `web` / `web+api`):

`../../sdkwork-specs/examples/webserver/adaptive-snippets/`

The `sdkwork-webserver` product edge is reverse-proxy only (`expose.mode: api`);
console Adaptive Web is process-owned (`AdaptiveAppShell`). Do not reintroduce
Adaptive Web nginx snippets under `deployments/webserver/` (W23).

Authority: `SDKWORK_DEPLOY_SPEC.md` §7 / §8, `NGINX_SPEC.md` §7,
`SDKWORK_WEBSERVER_SPEC.md` §11.3.
