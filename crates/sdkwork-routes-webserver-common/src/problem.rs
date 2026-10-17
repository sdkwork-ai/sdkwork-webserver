use axum::{
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use sdkwork_utils_rust::{SdkWorkProblemDetail, SdkWorkResultCode, SDKWORK_TRACE_ID_HEADER};
use sdkwork_webserver_contract::WebServiceError;

use crate::correlation::resolved_trace_id;

pub type WebApiResult<T> = Result<T, WebApiError>;

#[derive(Debug, Clone)]
pub struct WebApiError {
    code: SdkWorkResultCode,
    detail: String,
}

impl WebApiError {
    pub fn new(code: SdkWorkResultCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// Result code carried by this error (visible to tests and handlers that
    /// branch on the failure class before rendering).
    pub fn code(&self) -> SdkWorkResultCode {
        self.code
    }

    pub fn authentication_required(detail: impl Into<String>) -> Self {
        Self::new(SdkWorkResultCode::AuthenticationRequired, detail)
    }
}

impl From<WebServiceError> for WebApiError {
    fn from(error: WebServiceError) -> Self {
        use sdkwork_webserver_contract::WebServiceErrorKind;
        let kind = error.kind();
        let code = match kind {
            WebServiceErrorKind::NotFound => SdkWorkResultCode::NotFound,
            WebServiceErrorKind::Conflict => SdkWorkResultCode::Conflict,
            WebServiceErrorKind::Validation => SdkWorkResultCode::ValidationError,
            WebServiceErrorKind::Forbidden => SdkWorkResultCode::PermissionRequired,
            WebServiceErrorKind::DatabaseUnavailable => SdkWorkResultCode::ServiceUnavailable,
            WebServiceErrorKind::Unavailable => SdkWorkResultCode::ServiceUnavailable,
            WebServiceErrorKind::Internal => SdkWorkResultCode::InternalError,
        };
        let detail = match kind {
            WebServiceErrorKind::DatabaseUnavailable => {
                "database service is unavailable".to_string()
            }
            WebServiceErrorKind::Internal => "internal server error".to_string(),
            _ => error.to_string(),
        };
        Self::new(code, detail)
    }
}

impl IntoResponse for WebApiError {
    fn into_response(self) -> Response {
        let trace_id = resolved_trace_id();
        let mut problem = SdkWorkProblemDetail::platform(self.code, self.detail, trace_id.clone());
        // `API_SPEC.md` §15 allows `40001 VALIDATION_ERROR` with `400` or
        // `422`. The management OpenAPI authorities declare the
        // `ValidationError` response only under `'422'` (semantic command
        // validation), so this repository pins the `422` variant; syntactic
        // failures use `MalformedRequest`/`InvalidParameter` and stay `400`.
        if self.code == SdkWorkResultCode::ValidationError {
            problem.status = 422;
        }
        let status =
            StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response();
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(SDKWORK_TRACE_ID_HEADER.as_bytes()),
            HeaderValue::from_str(&trace_id),
        ) {
            response.headers_mut().insert(name, value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, response::IntoResponse};
    use sdkwork_utils_rust::{SdkWorkResultCode, SDKWORK_TRACE_ID_HEADER};
    use sdkwork_webserver_contract::WebServiceError;

    use super::WebApiError;

    #[test]
    fn problem_response_adds_trace_header_without_panicking() {
        let response =
            WebApiError::new(SdkWorkResultCode::ValidationError, "invalid request").into_response();

        assert!(response.headers().get(SDKWORK_TRACE_ID_HEADER).is_some());
    }

    #[test]
    fn validation_problem_pins_status_422_per_openapi_authorities() {
        // The OpenAPI authorities declare `ValidationError` only under
        // `'422'` (`API_SPEC.md` §15: `40001` with `400` or `422`).
        let response =
            WebApiError::new(SdkWorkResultCode::ValidationError, "invalid request").into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );

        let malformed = WebApiError::new(
            SdkWorkResultCode::InvalidParameter,
            "page_size must be <= 200",
        )
        .into_response();
        assert_eq!(malformed.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn internal_problem_response_does_not_expose_dependency_details() {
        let error = WebApiError::from(WebServiceError::Internal(
            "postgres password=should-not-leak".to_string(),
        ));
        assert_eq!(error.detail, "internal server error");

        let response = error.into_response();
        let body = to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("bounded problem body");
        let body = String::from_utf8(body.to_vec()).expect("UTF-8 problem body");

        assert!(!body.contains("password"));
        assert!(!body.contains("should-not-leak"));
    }
}
