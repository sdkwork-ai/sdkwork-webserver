# SDKWork Web Server Certificate Worker Specs

`component.spec.json` declares the durable certificate operation worker and its service/repository
runtime dependencies.

The worker periodically schedules due managed renewals and executes bounded `ISSUE` and `RENEW`
operations from `webserver_certificate_operation`. Repository claims use expiring leases, fencing tokens,
bounded attempts, and retry timestamps so concurrent workers cannot finalize stale work. The
service owns issuance orchestration and aggregate transitions; ACME, protected material, and edge
activation remain in their declared providers.

`SDKWORK_WEBSERVER_CERT_OPERATION_POLL_INTERVAL_SECS` controls operation polling,
`SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS` controls renewal scheduling, and
`SDKWORK_WEBSERVER_CERT_WORKER_ID` identifies the lease owner. Each worker instance requires a distinct,
stable ID. Shutdown stops before another polling delay and lets the active bounded cycle finish.

Verify from the repository root with the worker Cargo tests, repository operation tests, and strict
component-port validation.
