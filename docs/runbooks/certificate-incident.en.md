# Certificate Incident Runbook (sdkwork-webserver)

Standards: PRD §8.4 (certificate incident runbook), REQ-2026-0048 (bounded ACME
lifecycle). Symptoms: a certificate is near or past expiry, an issue/renew
operation is stuck in `PENDING/RUNNING`, the CA rejected a request, or node TLS
material diverged from the control plane.

## 1. Triage

```bash
# Operation states (0=PENDING,1=RUNNING,2=SUCCEEDED,3=FAILED,4=EXHAUSTED)
psql "$DATABASE_URL" -c "SELECT id, operation_type, status, attempts, lease_owner,
       lease_expires_at, fencing_token, next_attempt_at
       FROM web_certificate_operation ORDER BY id DESC LIMIT 20;"

# Certificate aggregate and versions
psql "$DATABASE_URL" -c "SELECT uuid, status, renewal_status, auto_renew
       FROM web_certificate WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 20;"

# Certificate worker liveness (systemd installs)
systemctl status sdkwork-webserver-certificate-worker
journalctl -u sdkwork-webserver-certificate-worker -n 200
```

## 2. Scenarios

### 2.1 Renewal due but unclaimed

- Verify the worker runs and `SDKWORK_WEBSERVER_CERT_OPERATION_POLL_INTERVAL_SECS`
  was not enlarged.
- The due scan interval is `SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS`
  (default 3600s); restart the worker to trigger an immediate scan.
- Manual renewal goes through the API (returns `202` + `operationId`, completes
  asynchronously): `POST /backend/v3/api/certificates/{certificateId}/renew`.

### 2.2 Operation stuck in RUNNING (worker crash leftover)

The lease mechanism self-heals: once `lease_expires_at` passes, the next claim
(`FOR UPDATE SKIP LOCKED`) re-claims with `fencing_token + 1`. Wait for lease
expiry; do **not** hand-UPDATE the status row (it would bypass the fencing
guard).

### 2.3 Retry budget exhausted (EXHAUSTED)

- Read `web_certificate.metadata.certificateOperationFailureCode` to classify
  the failure (DNS unverified / CA rejection / webroot not writable).
- Fix the root cause and issue a new operation; each operation carries its own
  bounded retry budget.

### 2.4 ACME account / directory trouble

- Accounts are encrypted under `SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT`
  (AES-256-GCM). Never hand-edit or delete account files — that recreates CA
  accounts per issuance and trips provider rate limits.
- Wildcard identifiers require DNS-01 (not implemented; config validation
  rejects explicitly). Use multi-SAN certificates instead.

### 2.5 Node TLS material divergence (edge still serving the old certificate)

- Compare `SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT` and the `tls-runtime.json`
  snapshot generation/fingerprint.
- After every successful operation the worker re-projects listener bindings and
  publishes a monotonic snapshot; the data plane hot-reloads without dropping
  connections. If the snapshot is stale, check the worker log first, then that
  `web_listener_certificate_binding.desired_version_id` matches the current
  version.

## 3. Post-recovery verification

```bash
openssl s_client -connect server.sdkwork.com:443 -servername server.sdkwork.com </dev/null \
  | openssl x509 -noout -dates -subject -ext subjectAltName
```

PRD success metric: 100% of production domains serve a valid TLS 1.2/1.3
certificate passing expiry/hostname/chain checks.
