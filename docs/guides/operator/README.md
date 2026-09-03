# Operator Guide

Deployment, monitoring, and incident response entrypoints.

See `DOCUMENTATION_SPEC.md` section 2.

## Guides

| Document | Purpose |
| --- | --- |
| [docker-install.md](docker-install.md) / [docker-install.en.md](docker-install.en.md) | Docker 三环境安装与部署手册（中/英）：快速上手、打新镜像包、一键部署、验证与排查 · Docker three-environment install & deployment handbook (CN/EN): quick start, new image packaging, one-command deploy, verification & troubleshooting |
| [WSL_DOCKER_DEPLOY.md](WSL_DOCKER_DEPLOY.md) | WSL Ubuntu Docker deployment (embedded or external PostgreSQL/Redis, multi-environment) |
| [bare-metal-install.md](bare-metal-install.md) | Linux bare-metal/VM installation: canonical config directory initialization, resource deployment, systemd unit, validation, upgrade/rollback/uninstall |

Kubernetes deployment is covered by `../../../deployments/kubernetes/README.md`; Docker by
`../../../deployments/docker/README.md`. Source runtime configuration authority is
`../../../etc/README.md`.
