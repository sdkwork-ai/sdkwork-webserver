use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use sdkwork_utils_rust::{
    PageInfo, PageMode, SdkWorkApiResponse, SdkWorkPageData, SdkWorkResourceData,
    SDKWORK_TRACE_ID_HEADER,
};
use sdkwork_webserver_contract::{
    ApplicationPage, AuditLogPage, CertificateDistributionPage, CertificatePage, DeploymentPage,
    DomainPage, EnvVariablePage, HealthCheckPage, ListenerCertificateBindingPage, NginxConfigPage,
    RootDomainPage, ServerPage, SourceVersionPage, PlatformTargetPage, WebServiceResult,
};
use serde::Serialize;

use crate::{correlation::resolved_trace_id, WebApiError};

fn attach_trace_header(response: &mut Response, trace_id: &str) {
    if let (Ok(name), Ok(value)) = (
        HeaderName::from_bytes(SDKWORK_TRACE_ID_HEADER.as_bytes()),
        HeaderValue::from_str(trace_id),
    ) {
        response.headers_mut().insert(name, value);
    }
}

fn envelope<T: Serialize>(status: StatusCode, data: T) -> Response {
    let trace_id = resolved_trace_id();
    let body = SdkWorkApiResponse::success(data, trace_id.clone());
    let mut response = (status, Json(body)).into_response();
    attach_trace_header(&mut response, &trace_id);
    response
}

fn offset_page_info(page: i32, page_size: i32, total: i64) -> PageInfo {
    let total_pages = if total <= 0 {
        0
    } else {
        i32::try_from((total - 1) / i64::from(page_size) + 1).unwrap_or(i32::MAX)
    };
    PageInfo {
        mode: PageMode::Offset,
        page: Some(page),
        page_size: Some(page_size),
        total_items: Some(total.to_string()),
        total_pages: Some(total_pages),
        next_cursor: None,
        has_more: Some(total > i64::from(page) * i64::from(page_size)),
    }
}

fn build_page_data<T: Serialize>(
    items: Vec<T>,
    page: i32,
    page_size: i32,
    total: i64,
) -> SdkWorkPageData<T> {
    SdkWorkPageData {
        items,
        page_info: offset_page_info(page, page_size, total),
    }
}

pub fn ok_resource<T: Serialize>(result: WebServiceResult<T>) -> Result<Response, WebApiError> {
    match result {
        Ok(item) => Ok(envelope(StatusCode::OK, SdkWorkResourceData { item })),
        Err(error) => Err(error.into()),
    }
}

pub fn created_resource<T: Serialize>(
    result: WebServiceResult<T>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(item) => Ok(envelope(StatusCode::CREATED, SdkWorkResourceData { item })),
        Err(error) => Err(error.into()),
    }
}

pub fn accepted_async<T: Serialize>(result: WebServiceResult<T>) -> Result<Response, WebApiError> {
    match result {
        Ok(operation) => Ok(envelope(StatusCode::ACCEPTED, operation)),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_application_page(
    result: WebServiceResult<ApplicationPage>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page.items, page.page, page.page_size, page.total),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_deployment_page(
    result: WebServiceResult<DeploymentPage>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => {
            // `has_more` is `Some` exactly in cursor mode, including the
            // last page; the cursor contract must never fall back to offset
            // `pageInfo` (which would report a misleading `page=0` and
            // `totalItems="0"` on the final page).
            let payload = if page.has_more.is_some() {
                SdkWorkPageData {
                    items: page.items,
                    page_info: PageInfo {
                        mode: PageMode::Cursor,
                        page: None,
                        page_size: Some(page.page_size),
                        total_items: None,
                        total_pages: None,
                        next_cursor: page.next_cursor,
                        has_more: page.has_more,
                    },
                }
            } else {
                build_page_data(page.items, page.page, page.page_size, page.total)
            };
            Ok(envelope(StatusCode::OK, payload))
        }
        Err(error) => Err(error.into()),
    }
}

pub fn ok_source_version_page(
    result: WebServiceResult<SourceVersionPage>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => {
            // Cursor mode (including the last page) is identified by
            // `has_more: Some`; see `ok_deployment_page`.
            let payload = if page.has_more.is_some() {
                SdkWorkPageData {
                    items: page.items,
                    page_info: PageInfo {
                        mode: PageMode::Cursor,
                        page: None,
                        page_size: Some(page.page_size),
                        total_items: None,
                        total_pages: None,
                        next_cursor: page.next_cursor,
                        has_more: page.has_more,
                    },
                }
            } else {
                build_page_data(page.items, page.page, page.page_size, page.total)
            };
            Ok(envelope(StatusCode::OK, payload))
        }
        Err(error) => Err(error.into()),
    }
}

pub fn ok_nginx_config_page(
    result: WebServiceResult<NginxConfigPage>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page.items, page.page, page.page_size, page.total),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_audit_log_page(result: WebServiceResult<AuditLogPage>) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => {
            // See `ok_deployment_page`: cursor mode (including the last page)
            // is identified by `has_more: Some`, never by `next_cursor`.
            let payload = if page.has_more.is_some() {
                SdkWorkPageData {
                    items: page.items,
                    page_info: PageInfo {
                        mode: PageMode::Cursor,
                        page: None,
                        page_size: Some(page.page_size),
                        total_items: None,
                        total_pages: None,
                        next_cursor: page.next_cursor,
                        has_more: page.has_more,
                    },
                }
            } else {
                build_page_data(page.items, page.page, page.page_size, page.total)
            };
            Ok(envelope(StatusCode::OK, payload))
        }
        Err(error) => Err(error.into()),
    }
}

pub fn ok_domain_page(
    result: WebServiceResult<DomainPage>,
    page: i32,
    page_size: i32,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page_data) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page_data.items, page, page_size, page_data.total),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_root_domain_page(
    result: WebServiceResult<RootDomainPage>,
    page: i32,
    page_size: i32,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page_data) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page_data.items, page, page_size, page_data.total),
        )),
        Err(error) => Err(error.into()),
    }
}

/// Bounded-by-design collections (`envVariables.list`, `healthChecks.list`) are
/// transactionally capped at 100 items (PAGINATION_SPEC §11) and are served as a
/// single page with the collection capacity as `page_size`, so `pageInfo` truthfully
/// reports one page, `hasMore=false`, and `total` matching the returned items.
const BOUNDED_COLLECTION_MAXIMUM_PAGE_SIZE: i32 = 100;

pub fn ok_env_variable_page(
    result: WebServiceResult<EnvVariablePage>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => Ok(envelope(
            StatusCode::OK,
            build_page_data(
                page.items,
                1,
                BOUNDED_COLLECTION_MAXIMUM_PAGE_SIZE,
                page.total,
            ),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_certificate_page(
    result: WebServiceResult<CertificatePage>,
    page: i32,
    page_size: i32,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page_data) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page_data.items, page, page_size, page_data.total),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_listener_certificate_binding_page(
    result: WebServiceResult<ListenerCertificateBindingPage>,
    page: i32,
    page_size: i32,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page_data) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page_data.items, page, page_size, page_data.total),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_certificate_distribution_page(
    result: WebServiceResult<CertificateDistributionPage>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page.items, page.page, page.page_size, page.total),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_platform_target_page(
    result: WebServiceResult<PlatformTargetPage>,
    page: i32,
    page_size: i32,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page_data) => Ok(envelope(
            StatusCode::OK,
            build_page_data(page_data.items, page, page_size, page_data.total),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_health_check_page(
    result: WebServiceResult<HealthCheckPage>,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page) => Ok(envelope(
            StatusCode::OK,
            build_page_data(
                page.items,
                1,
                BOUNDED_COLLECTION_MAXIMUM_PAGE_SIZE,
                page.total,
            ),
        )),
        Err(error) => Err(error.into()),
    }
}

pub fn ok_server_page(
    result: WebServiceResult<ServerPage>,
    page: i32,
    page_size: i32,
) -> Result<Response, WebApiError> {
    match result {
        Ok(page_data) => {
            // Cursor mode (including the last page) is identified by
            // `has_more: Some`, never by `next_cursor`; see
            // `ok_deployment_page`.
            let payload = if page_data.has_more.is_some() {
                SdkWorkPageData {
                    items: page_data.items,
                    page_info: PageInfo {
                        mode: PageMode::Cursor,
                        page: None,
                        page_size: Some(page_size),
                        total_items: None,
                        total_pages: None,
                        next_cursor: page_data.next_cursor,
                        has_more: page_data.has_more,
                    },
                }
            } else {
                build_page_data(page_data.items, page, page_size, page_data.total)
            };
            Ok(envelope(StatusCode::OK, payload))
        }
        Err(error) => Err(error.into()),
    }
}

pub fn no_content(result: WebServiceResult<()>) -> Result<Response, WebApiError> {
    match result {
        Ok(()) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use sdkwork_utils_rust::{SdkWorkApiResponse, SdkWorkResourceData, SDKWORK_SUCCESS_CODE};
    use sdkwork_webserver_contract::{AgentSyncResponse, CertificateOperationAcceptedResponse};

    use super::{accepted_async, ok_resource};

    #[tokio::test]
    async fn agent_sync_resource_uses_the_canonical_sdkwork_envelope() {
        let manifest = AgentSyncResponse {
            server_id: "server-1".to_string(),
            sync_version: "sv1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            unchanged: true,
            nginx_configs: Vec::new(),
            certificates: Vec::new(),
        };
        let response = ok_resource(Ok(manifest)).expect("resource response");
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("bounded response body");
        let decoded: SdkWorkApiResponse<SdkWorkResourceData<AgentSyncResponse>> =
            serde_json::from_slice(&body).expect("canonical resource envelope");

        assert_eq!(decoded.code, SDKWORK_SUCCESS_CODE);
        assert!(!decoded.trace_id.is_empty());
        assert_eq!(decoded.data.item.server_id, "server-1");
        assert!(decoded.data.item.unchanged);
    }

    #[tokio::test]
    async fn async_accept_uses_202_and_the_canonical_async_payload() {
        let response = accepted_async(Ok(CertificateOperationAcceptedResponse {
            accepted: true,
            operation_id: "operation-1".to_string(),
            status: "pending".to_string(),
        }))
        .expect("async response");
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("bounded response body");
        let decoded: SdkWorkApiResponse<CertificateOperationAcceptedResponse> =
            serde_json::from_slice(&body).expect("canonical async envelope");
        assert_eq!(decoded.code, SDKWORK_SUCCESS_CODE);
        assert!(decoded.data.accepted);
        assert_eq!(decoded.data.operation_id, "operation-1");
    }
}
