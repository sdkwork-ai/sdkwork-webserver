# Failed Rollout Runbook (sdkwork-webserver)

Standards: PRD §8.2 (failed canary stops automatically, last verified revision
restorable within 30s), FR-026 (deployment status must be truthful). Symptoms:
container-channel `release.sh` health gate failure, deb upgrade failing health
probes, or application deployment records stuck in PENDING.

## 1. Container channel (bin/docker-deploy.sh)

`bin/` is the single operator surface (MODULE_BIN_SPEC.md §1): the bundle's
`deploy.sh` / `release.sh` are driven by that entrypoint. Calling them directly
bypasses the pre-change backup gate and the evidence trail.

```bash
# Release status (drives the bundle's deploy.sh --ps internally)
bin/docker-deploy.sh status --environment production
# Per-instance health gate: the old instance keeps serving until the new one
# passes wait_container_healthy
docker ps --format '{{.Names}} {{.Status}}' | grep sdkwork
# Ledger (on the deploy target: /opt/deploy/sdkwork-webserver/bundle/)
tail -n 50 /opt/deploy/sdkwork-webserver/bundle/release-ledger.log 2>/dev/null
```

- The entrypoint's `install`/`upgrade` run the **pre-change backup gate** on
  staging/demo/production (`--skip-backup` records an evidence line), and the
  bundle's `release.sh` adds digest/sha256 verification plus **auto-rollback**:
  when the new version fails its health gate it falls back to the previous image
  tag and appends the ledger.
- Manual rollback goes through the same entrypoint:
  `bin/docker-deploy.sh rollback --environment production`
  (`--to <image-tag>` to pin a revision; without it the ledger's previous tag is used).
- After rollback verify `/healthz` and a business route before re-attempting.
- Live diagnostics: `bin/docker-deploy.sh logs --environment production --tail 200`.

## 2. deb/systemd channel

- Upgrade is a bounded maintenance window ("stop, install, start";
  `TimeoutStopSec=45`). If postinst start fails its health gate:
  ```bash
  systemctl status sdkwork-webserver
  journalctl -u sdkwork-webserver -n 200
  curl -fsS http://127.0.0.1:3800/healthz
  ```
- Rollback = reinstall the previous .deb (configuration survives under
  /etc/sdkwork/webserver and /var/lib/sdkwork/webserver; database migrations
  are forward-fix per database/migrations).
- Migration failure: startup drift gating fails closed and the service refuses
  to start; fix per `docs/migrations/` and restart (0006 is irreversible —
  forward-fix only).

## 3. Application deployment records stuck in PENDING (control plane)

This is the designed truthful state: `web_deployment` is a command intent; the
deployment worker that advances `status` belongs to the Deploy control plane
(REQ-2026-0061/0062 gate; ADR-20260731 under human review). Therefore
`sites.activate` (requires a successful deployment) and `deployments.rollback`
(require a successful source version) honestly return `409`. Disposition:

- Tell tenants the release stays in "accepted" state; do not retry activation.
- Enable the full release flow once the Deploy execution authority ships.

## 4. Post-upgrade checklist

1. `/healthz`, `/readyz` pass;
2. management login and site list work;
3. spot-check one static site and one reverse-proxy route on the data plane;
4. `bin/doctor.sh` (9 read-only checks) reports no FAIL.
