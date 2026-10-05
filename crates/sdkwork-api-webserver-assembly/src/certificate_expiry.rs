//! Certificate-expiry sampling port for the standalone gateway's metrics.
//!
//! The gateway's operations scrape publishes `certificate_expiry_seconds_min`
//! / `..._expiring_soon` gauges, and something must read the aggregate from
//! the control-plane database. Constructing that reader lives here — the API
//! assembly is the crate allowed to own the repository bootstrap
//! (`RUST_CODE_SPEC` §1.1, `api-assembly` row) — while the gateway keeps the
//! scheduling and the metric recording. The service is shared by the business
//! route assembly once it is up: a data-plane-only process (or an edge with
//! no control-plane database) simply reports no reader, and the sampler stays
//! disabled instead of building a second repository bootstrap beside the
//! owner's.

use std::sync::{Arc, OnceLock};

use sdkwork_intelligence_webserver_service::WebService;
use sdkwork_webserver_contract::WebServiceResult;

static SHARED_EXPIRY_SERVICE: OnceLock<Arc<WebService>> = OnceLock::new();

/// Publishes the assembled control-plane service as the certificate-expiry
/// reader. Called once by [`assemble_business_routes`](crate::assemble_business_routes).
pub(crate) fn share_certificate_expiry_service(service: Arc<WebService>) {
    let _ = SHARED_EXPIRY_SERVICE.set(service);
}

/// The operator's certificate-expiry warning window in days: what
/// "expiring soon" means for the gauges and the alert rule. Read once per
/// sample from `SDKWORK_WEBSERVER_CERT_EXPIRY_WINDOW_DAYS`
/// (1..=365, default 30 - the window the Prometheus rule was sized to).
pub fn certificate_expiry_window_days() -> i32 {
    const DEFAULT_WINDOW_DAYS: i32 = 30;
    std::env::var("SDKWORK_WEBSERVER_CERT_EXPIRY_WINDOW_DAYS")
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .map(|days| days.clamp(1, 365))
        .unwrap_or(DEFAULT_WINDOW_DAYS)
}

/// One expiry-summary sample from the shared control-plane service; `None`
/// when no management plane has been assembled in this process.
pub async fn certificate_expiry_summary() -> Option<WebServiceResult<(i64, i64)>> {
    let service = SHARED_EXPIRY_SERVICE.get()?.clone();
    Some(
        service
            .certificate_expiry_summary(certificate_expiry_window_days())
            .await,
    )
}
