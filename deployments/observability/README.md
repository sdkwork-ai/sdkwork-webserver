# Observability (alert rules)

`prometheus-rules.yml` holds the alert-rule group for the Web Server. Every
expression is grounded in a series the shipped exporters actually publish —
the fixed data-plane operations registry (REQ-2026-0033/0034), the
certificate-expiry sampler gauges, and the audit-persistence counter — because
an alert that can never fire is a fake control (PRD §12).

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

## Exported series

- Data-plane operations registry (`sdkwork_web_data_plane_*`): scrape health,
  resource pressure, upstream failure/timeout ratios, request rejection,
  protocol errors, WebSocket drain timeouts (REQ-2026-0033/0034).
- Certificate expiry: `sdkwork_webserver_certificate_expiry_seconds_min`
  (gauge; -1 = no observation yet) and `sdkwork_webserver_certificate_expiring_soon`
  are sampled by the `data-plane` command: a supervised, shutdown-aware task
  (`SDKWORK_WEBSERVER_CERT_METRICS_INTERVAL_SECS`, default 300 s, clamped
  30..3600) reads the minimum seconds-to-expiry over active, non-revoked
  certificates plus the expiring count through the shared database pool and
  records them into the snapshot the operations `/metrics` handler
  renders. The "expiring soon" window is
  `SDKWORK_WEBSERVER_CERT_EXPIRY_WINDOW_DAYS` (days, default 30, clamped
  1..365) — retune it together with the alert thresholds in
  `prometheus-rules.yml`, which are expressed against the same window.
  Sampling is best effort by design — an edge without a
  control-plane database simply reports "no observation" (PRD-FR-015); a
  failed sample leaves the last value and logs.
- Audit persistence:
  `sdkwork_webserver_audit_persistence_failures_total` (counter) — audit rows
  that could not be persisted since process start. The audit insert is
  post-commit by design, so a nonzero increase is a permanent audit gap for
  the affected operations; the `SdkworkWebAuditPersistenceFailure` rule pages
  on it.

## Deliberately absent rules

Node-divergence and rollout-state alerts need series no exporter carries yet
(`sdkwork_web_data_plane_*` has no cluster-plane series). Shipping rules
against nonexistent series would be the fake-control failure mode this file
exists to avoid; they land together with the corresponding metric series
(tracked in TECH_ARCHITECTURE.md and the troubleshooting runbook).
