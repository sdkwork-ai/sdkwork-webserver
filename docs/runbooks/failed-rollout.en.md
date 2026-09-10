# Failed Rollout Runbook (sdkwork-webserver)

Standards: PRD §8.2 (failed canary stops automatically, last verified revision
restorable within 30s), FR-026 (deployment status must be truthful). Symptoms:
container-channel `release.sh` health gate failure, deb upgrade failing health
probes, or application deployment records stuck in PENDING.

## 1. Container channel (deploy.sh / release.sh)

```bash
cd /opt/deploy/sdkwork-webserver/bundle
bash deploy.sh --environment production --ps
tail -n 50 release-ledger.log 2>/dev/null || ls var/
docker ps --format '{{.Names}} {{.Status}}' | grep sdkwork
```

- `release.sh` performs digest/sha256 bundle verification and **auto-rollback**:
  when the new version fails its health gate, it falls back to the previous
  image tag and appends the ledger. Manual rollback: re-run `deploy.sh` with
  the previous `--image-tag` recorded in the ledger.
- After rollback verify `/healthz` and a business route before re-attempting.

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
