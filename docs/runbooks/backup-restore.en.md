# Backup And Restore Runbook (sdkwork-webserver)

Standard: OPERATIONS_SPEC.md §5 (RPO: production 24 h + a pre-deploy backup;
RTO: production 4 h). Sets are created on the target at
`/opt/deploy/sdkwork-webserver/backups/` and contain: configuration (env chain), the
database (pg_dump custom format), volumes (optional), and `manifest.json`
plus a per-component `.sha256`.

## 1. Capture

```bash
bin/backup.sh create --environment demo                      # config + database + volumes
bin/backup.sh create --environment demo --no-volumes         # config + database
bin/backup.sh list  --environment demo
```

Daily production cron (02:30):
```cron
30 2 * * * cd /opt/deploy/sdkwork-webserver/bundle && bash deploy.sh --environment production --ps >/dev/null
30 2 * * * <repo>/bin/backup.sh create --environment production --host ssh://ops@db-host
```

## 2. Verify

```bash
bin/backup.sh verify --environment demo                     # newest set
bin/backup.sh verify --environment demo --set <set-name>    # named set
```

## 3. Restore (destructive, requires --yes)

```bash
bin/backup.sh restore --environment demo --set <set-name> --yes
bin/backup.sh restore --environment demo --component config --set <set-name> --yes
bin/docker-deploy.sh install --environment demo        # bring the stack back
```

Restore flow: verify checksums -> `deploy.sh --down` stops the stack ->
restore the selected components -> print the restart command. The database is
restored with `pg_restore --clean --if-exists` (the only supported path to
undo a forward-only migration).

## 4. Drill (quarterly)

1. pick a recent set: `bin/backup.sh list --environment demo`;
2. restore on an isolated target: `--host ssh://<scratch-host>`;
3. `bin/doctor.sh --environment demo` fully green;
4. record the duration and confirm RTO (production 4 h).

## 5. Retention and cleanup

| Environment | Generations kept |
|---|---|
| development / test | 3 |
| staging / demo | 7 |
| production | 30 + the last pre-release backup |

Back up before `--purge`:
```bash
bin/backup.sh create --environment demo
bin/docker-deploy.sh down --environment demo --purge --yes
```

## 6. Database connection keys

This module uses the `SDKWORK_DATABASE_*` prefix; `host.docker.internal` is mapped to
`127.0.0.1` when the pg client runs on the host.
