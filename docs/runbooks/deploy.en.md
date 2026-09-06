# Deploy Runbook (sdkwork-webserver)

Environments: `development|test|staging|demo|production`. Commands default to the local WSL target; add
`--host ssh://[user@]host[:port]` for a remote host.

## 1. Build and package

```bash
bin/docker-image.sh build                       # tag from sdkwork.app.config.json
bin/docker-image.sh save -o dist/image.tar.gz   # emits .tar.gz + .sha256
bin/apps-package.sh pc production               # browser artifacts
bin/apps-package.sh h5 production
```

## 2. Install / upgrade

```bash
# production requires --yes
bin/docker-deploy.sh install --environment development
bin/docker-deploy.sh install --environment demo --replicas 2
bin/docker-deploy.sh install --environment production --yes
bin/docker-deploy.sh upgrade  --environment demo --image-tag 0.1.1
```

`install` syncs the bundle to `/opt/deploy/sdkwork-webserver/bundle`, loads the image, starts
instances 1..N and waits for the health gate.

## 3. Verify

```bash
bin/docker-deploy.sh status --environment demo
bin/doctor.sh --environment demo          # aggregated diagnostics (9 checks)
```

## 4. Version rollback

`bin/docker-deploy.sh rollback` uses the bundle-owned `release.sh` to return to the previous successful version recorded in the append-only release ledger (OPERATIONS_SPEC.md §1.2), gated by `/healthz` probes on the management ports; a failed gate triggers automatic rollback. Pin an explicit version with `--to <old-version>`. Inspect history with `bash release.sh history --environment <env>` on the target bundle.

```bash
bin/docker-deploy.sh rollback --environment demo                     # back to the previous successful version
bin/docker-deploy.sh rollback --environment demo --to 0.1.0          # back to an explicit version
bin/docker-deploy.sh upgrade  --environment demo --image-tag 0.2.0   # roll forward
```

## 5. Retire

```bash
bin/docker-deploy.sh down --environment demo
bin/docker-deploy.sh stop    --environment demo                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment demo                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment demo                  # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down --environment demo --purge --yes
bin/docker-deploy.sh stop    --environment demo                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment demo                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment demo                  # restart app instances only (deps and gateway stay up)
```
