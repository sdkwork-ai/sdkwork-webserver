# Observability (first slice: alert rules)

`prometheus-rules.yml` holds the first alert-rule group for the Web Server.
Every expression is grounded in a series the fixed data-plane operations
registry (REQ-2026-0033/0034) actually exports — no rule references a series
that does not exist, because an alert that can never fire is a fake control
(PRD §12).

## Wiring

1. Scrape the host-operations listener. It is loopback-only on the host and
   `ClusterIP`-internal on Kubernetes: port `3811`, path `/metrics`
   (see `deployments/kubernetes/deployment.yaml`; the NetworkPolicy admits
   only labeled scraper roles). Recommended job name: `sdkwork-webserver-operations`
   (the `SdkworkWebOperationsScrapeDown` rule keys on it).
2. Load the rule file into your Prometheus (`rule_files`) or hand it to
   Alertmanager through whatever rule-distribution the monitoring stack uses.
3. Alert routing/severity policy lives with the monitoring stack owner; the
   `runbook_url` annotations point into `docs/runbooks/`.

## Deliberately absent rules

Certificate expiry, node divergence, and rollout-state alerts need series the
registry does not carry yet (`sdkwork_web_data_plane_*` has no certificate or
cluster-plane series). Shipping rules against nonexistent series would be the
fake-control failure mode this file exists to avoid; they land together with
the corresponding metric series (tracked in TECH_ARCHITECTURE.md and the
troubleshooting runbook). Until then, certificate-expiry detection runs
through `bin/doctor.sh` and the certificate-incident runbook's SQL checks.

## Certificate expiry series (implemented)

`sdkwork_webserver_certificate_expiry_seconds_min` (gauge; -1 = no
observation yet) and `sdkwork_webserver_certificate_expiring_soon` are
sampled by the `data-plane` command: a supervised, shutdown-aware task
(`SDKWORK_WEBSERVER_CERT_METRICS_INTERVAL_SECS`, default 300 s, clamped
30..3600) reads the minimum seconds-to-expiry over active, non-revoked
certificates plus the 30-day expiring count through the shared database
pool and records them into the snapshot the operations `/metrics` handler
renders. Sampling is best effort by design — an edge without a
control-plane database simply reports "no observation" (PRD-FR-015); a
failed sample leaves the last value and logs. The two certificate alerts
in `prometheus-rules.yml` consume these series; node-divergence and
rollout-state alerts remain absent until their series exist.
