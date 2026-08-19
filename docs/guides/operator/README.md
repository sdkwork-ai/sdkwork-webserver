# Operator Guide

Deployment, monitoring, and incident response entrypoints.

See `DOCUMENTATION_SPEC.md` section 2.

## Guides

| Document | Purpose |
| --- | --- |
| [WSL_DOCKER_DEPLOY.md](WSL_DOCKER_DEPLOY.md) | WSL Ubuntu Docker deployment (embedded or external PostgreSQL/Redis, multi-environment) |
| [bare-metal-install.md](bare-metal-install.md) | Linux bare-metal/VM installation: canonical config directory initialization, resource deployment, systemd unit, validation, upgrade/rollback/uninstall |

Kubernetes deployment is covered by `../../../deployments/kubernetes/README.md`; Docker by
`../../../deployments/docker/README.md`. Source runtime configuration authority is
`../../../etc/README.md`.
