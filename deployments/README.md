# SDKWork Web Server Deployments

Validated production deployment templates for the `sdkwork-web-server` repository. Catalog identity
remains `sdkwork-web`; topology and deployment identity follow the repository name.

| Path | Purpose |
| --- | --- |
| `deploy.yaml` | Deploy v2 authority for cloud and standalone production profiles |
| `docker/` | Standalone WSL multi-environment compose stacks and cloud website image contract |
| `kubernetes/` | Digest-bound cloud workload, service, migration, and disruption contracts |

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
Linux host package with a dedicated single-tenant host service.

These files define deployable target state but do not prove publication. Application release
packages remain disabled with `releaseBuildDeferred: true`; no registry image or production rollout
is claimed until the corresponding artifact, digest, signature, SBOM, provenance, and approval
evidence exists.

Runtime values come from `etc/topology/*.env` and the deployment platform's secret manager. `sdkwork.app.config.json` declares identity and release capability, not concrete runtime configuration.
