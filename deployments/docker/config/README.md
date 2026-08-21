# Standalone container data-plane seed

Operations-only seed for image/local smoke (`serverNames: ["*"]`, development
default listen `:3800`, `/healthz` only). It does **not** host Adaptive Web.

Console Adaptive Web is owned by the standalone gateway `AdaptiveAppShell`:

| Mechanism | Keys / paths |
| --- | --- |
| Runtime TOML `[app_roots]` | `pc_static_root`, `h5_static_root`, `static_fallback_root`, `tablet_surface` (from `entrypoint-standalone.sh`) |
| Env overrides | `SDKWORK_WEBSERVER_{PC,H5,STATIC_FALLBACK}_STATIC_ROOT`, `SDKWORK_WEBSERVER_TABLET_SURFACE` |
| Image install roots | `/app/share/sdkwork/webserver/web/{pc,h5,static}` |

Public edge authority: `deployments/webserver/` (`expose.mode: api`, proxy-only).
Binds/URLs: `deployments/docker/env/<environment>.env`, `etc/topology/*.env`.
