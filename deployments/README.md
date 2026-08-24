# SDKWork Web Server Deployments

Validated production deployment templates for the `sdkwork-webserver` repository. Catalog identity
remains `sdkwork-web`; topology and deployment identity follow the repository name.

| Path | Purpose |
| --- | --- |
| `deploy.yaml` | Deploy v2 authority for cloud and standalone production profiles |
| `webserver/` | Declarative web server config (SDKWORK_WEBSERVER_SPEC.md layout v3) and rendered nginx sidecars |
| `docker/` | Standalone WSL multi-environment compose stacks and cloud website image contract |
| `kubernetes/` | Digest-bound cloud workload, service, migration, and disruption contracts |

## Public Hosts

Registered role host `web` (`APP_RUNTIME_TOPOLOGY_NAMING.md` §9.2; `applicationCode` remains `webserver`):

| Environment | public-ingress | app-http | backend-http |
| --- | --- | --- | --- |
| production | `server.sdkwork.com` | `server-app.sdkwork.com` | `server-admin.sdkwork.com` |
| development | `server-dev.sdkwork.com` | `server-app-dev.sdkwork.com` | `server-admin-dev.sdkwork.com` |
| test | `server-test.sdkwork.com` | `server-app-test.sdkwork.com` | `server-admin-test.sdkwork.com` |
| staging | `server-staging.sdkwork.com` | `server-app-staging.sdkwork.com` | `server-admin-staging.sdkwork.com` |

Retired nicknames: `web.sdkwork.com`, `web-*.sdkwork.com`, `testserver.sdkwork.com`.

`cloud.production` uses an immutable container image on Kubernetes and starts only the website
delivery edge runtime. The deployable cloud baseline is explicitly `single-tenant` and `dedicated`:
one opaque, non-sensitive tenant fleet name partitions the Website Service and all workload
selectors, while each Node receives its own provider-event Service and the actual tenant scope hash
and provider credentials remain only in per-Node Secrets. Every rendered
workload binds one Web Node identity and Secret to one recovery PVC; high availability is formed
from multiple independently rendered Nodes behind that tenant fleet's Website Service. Each Node
also receives a compiler-validated immutable listener ConfigMap whose trusted-proxy CIDRs are
explicit deployment inputs. Cloud management API assemblies are hosted by the platform cloud
gateway, not by this application's standalone gateway. `standalone.production` uses the signed
Linux host package with a dedicated single-tenant host service on `server.sdkwork.com`.

These files define deployable target state but do not prove publication. Application release
packages remain disabled with `releaseBuildDeferred: true`; no registry image or production rollout
is claimed until the corresponding artifact, digest, signature, SBOM, provenance, and approval
evidence exists.

Runtime values come from `etc/topology/*.env` and the deployment platform's secret manager.
`sdkwork.app.config.json` declares identity and release capability, not concrete runtime configuration.

## Validation

```bash
pnpm check:webserver-toml
pnpm check:deploy
pnpm nginx:render
pnpm check:container-deployment
```
