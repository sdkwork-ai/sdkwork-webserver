# Troubleshooting Runbook (sdkwork-webserver)

Rule: run `doctor.sh` first, then read logs, then act. Never restart without
evidence.

## 0. One command to locate the problem

```bash
bin/doctor.sh --environment demo          # 9 checks, non-zero exit = problem
bin/doctor.sh --environment demo --json   # for dashboards / tickets
bin/doctor.sh --environment demo --export ./incident
```

Checks: toolchain / bundle / compose / container health / HTTP probe / image
drift / configuration drift / log errors / disk.

## 1. Symptom -> action

### 1.1 `bundle FAIL`
```bash
bin/docker-deploy.sh install --environment demo
```

### 1.2 `compose FAIL` (no containers)
```bash
bin/docker-deploy.sh status --environment demo
bin/docker-deploy.sh install --environment demo
```

### 1.3 `health FAIL` (unhealthy / restart loop)
```bash
bin/docker-deploy.sh logs --environment demo --tail 300
# common: dependency unreachable -> 1.5; OOM -> raise limits; migration failed -> 1.6
```

### 1.4 `probe FAIL` (port unreachable)
```bash
bin/docker-deploy.sh status --environment demo       # port mappings
bin/config.sh show --environment demo | grep -i PORT # expected port
```
Port matrix: development=13800 ... production=18080 (instance i uses base + i - 1 or
the operator stride).

### 1.5 `config FAIL` (drift / placeholders)
```bash
bin/config.sh diff     --environment demo     # missing / undeclared / placeholder keys
bin/config.sh validate --environment demo     # module validator
bin/config.sh set SDKWORK_DATABASE_PASSWORD '<real-value>' --environment demo
bin/config.sh show    --environment demo      # re-check (secrets redacted)
```
`set` writes a `.bak.<timestamp>` on the target first and validates after;
on failure it prints the restore path. Config changes need a full `install`
(not a restart) so compose re-interpolates:
```bash
bin/docker-deploy.sh install --environment demo
```

### 1.6 `image WARN` (running image does not match the expected tag)
```bash
bin/docker-deploy.sh upgrade --environment demo --image-tag <expected-tag>
```

### 1.7 `logs WARN` (ERROR/panic lines)
```bash
bin/docker-deploy.sh logs --environment demo --tail 1000 --export ./incident
```

### 1.8 `disk WARN/FAIL`
```bash
docker system df
docker image prune -f          # only after confirming no in-use dangling image
```

## 2. Data corruption / rolling data back
```bash
bin/backup.sh list    --environment demo
bin/backup.sh verify  --environment demo --set <set-name>
bin/backup.sh restore --environment demo --set <set-name> --yes
bin/docker-deploy.sh install --environment demo
```

## 3. Returning to the previous version after a failed upgrade

`bin/docker-deploy.sh rollback` re-installs the current bundle idempotently (the webserver bundle has no release.sh). To return to an older version use `bin/docker-deploy.sh upgrade --environment <env> --image-tag <old-version>`.

## 4. Evidence to attach before escalating

1. the report from `bin/doctor.sh --environment demo --export ./incident`;
2. the capture from `bin/docker-deploy.sh logs --environment demo --tail 500 --export ./incident`;
3. `bin/config.sh show --environment demo` (secrets redacted).
