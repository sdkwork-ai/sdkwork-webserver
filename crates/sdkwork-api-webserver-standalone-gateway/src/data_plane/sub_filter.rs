//! nginx `sub_filter` response body substitution middleware.
//!
//! The handler attaches the selected route's `SubFilterConfig` to the
//! response via `SubFilterExtension`; this layer (registered inside the
//! compression layer, so substitution runs before gzip like nginx's filter
//! order) buffers eligible bodies, applies the ordered rules, recomputes
//! `Content-Length`, and drops `Last-Modified` unless `sub_filter_last_modified
//! on`. Ineligible responses (no extension, non-matching content type, or an
//! already-encoded body) stream through untouched.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use futures_util::StreamExt;

use sdkwork_webserver_core::{
    apply_sub_filters, sub_filter_content_type_matches, SubFilterConfig, MAX_SUB_FILTER_BODY_BYTES,
};

/// Route-level substitution configuration carried on the response.
#[derive(Clone)]
pub(crate) struct SubFilterExtension(pub SubFilterConfig);

/// Process-wide cap on concurrently buffered sub_filter response bytes.
///
/// The per-response `MAX_SUB_FILTER_BODY_BYTES` alone bounds one buffer, so
/// the theoretical aggregate is `max_concurrent_requests × 16 MiB`. This
/// budget reserves the worst-case share before buffering and releases it on
/// drop; when the budget is exhausted the response streams through
/// unsubstituted (availability over transformation).
const MAXIMUM_AGGREGATE_BUFFERED_BYTES: usize = 256 * 1024 * 1024;

static BUFFERED_BYTES: AtomicUsize = AtomicUsize::new(0);

/// RAII reservation against the aggregate buffer budget.
struct BufferBudgetGuard(usize);

impl Drop for BufferBudgetGuard {
    fn drop(&mut self) {
        BUFFERED_BYTES.fetch_sub(self.0, Ordering::Relaxed);
    }
}

fn reserve_buffer_budget() -> Option<BufferBudgetGuard> {
    let share = MAX_SUB_FILTER_BODY_BYTES;
    let mut current = BUFFERED_BYTES.load(Ordering::Relaxed);
    loop {
        if current + share > MAXIMUM_AGGREGATE_BUFFERED_BYTES {
            return None;
        }
        match BUFFERED_BYTES.compare_exchange_weak(
            current,
            current + share,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(BufferBudgetGuard(share)),
            Err(observed) => current = observed,
        }
    }
}

pub(crate) async fn apply_sub_filters_middleware(
    request: axum::extract::Request,
    next: Next,
) -> Response<Body> {
    let response = next.run(request).await;
    apply_sub_filters_to_response(response).await
}

async fn apply_sub_filters_to_response(mut response: Response<Body>) -> Response<Body> {
    let Some(SubFilterExtension(config)) = response.extensions_mut().remove::<SubFilterExtension>()
    else {
        return response;
    };
    if config.rules.is_empty() {
        return response;
    }
    let Some(content_type) = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return response;
    };
    if !sub_filter_content_type_matches(content_type, &config.types) {
        return response;
    }
    if response.headers().contains_key(header::CONTENT_ENCODING) {
        return response;
    }

    // Reserve the aggregate buffer budget before buffering; without budget
    // headroom the body streams through unsubstituted.
    let Some(_budget) = reserve_buffer_budget() else {
        return response;
    };

    let body = std::mem::take(response.body_mut());
    let bytes = match collect_body_limited(body, MAX_SUB_FILTER_BODY_BYTES as u64).await {
        Ok(bytes) => bytes,
        Err(()) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("sub_filter buffer failed"))
                .unwrap_or_else(|_| Response::new(Body::empty()));
        }
    };
    let replaced = apply_sub_filters(&bytes, &config);
    if replaced == bytes.as_ref() {
        // Nothing substituted: restore the buffered body (it was taken from
        // the response above), keeping the original headers intact.
        let (parts, _) = response.into_parts();
        return Response::from_parts(parts, Body::from(bytes));
    }

    let (mut parts, _) = response.into_parts();
    parts.headers.remove(header::CONTENT_LENGTH);
    if !config.last_modified {
        parts.headers.remove(header::LAST_MODIFIED);
    }
    parts
        .headers
        .insert(header::CONTENT_LENGTH, HeaderValue::from(replaced.len()));
    Response::from_parts(parts, Body::from(replaced))
}

/// Buffer a response body with a hard bound; substitution is skipped when
/// the body exceeds the bound (availability over transformation).
async fn collect_body_limited(body: Body, maximum_bytes: u64) -> Result<bytes::Bytes, ()> {
    let mut stream = body.into_data_stream();
    let mut collected = Vec::new();
    while let Some(frame) = stream.next().await {
        let chunk = frame.map_err(|_| ())?;
        if collected.len() as u64 + chunk.len() as u64 > maximum_bytes {
            return Err(());
        }
        collected.extend_from_slice(&chunk);
    }
    Ok(bytes::Bytes::from(collected))
}
