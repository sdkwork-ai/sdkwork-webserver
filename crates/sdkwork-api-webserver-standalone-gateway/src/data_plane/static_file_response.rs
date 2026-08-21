use std::{io, ops::RangeInclusive};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use bytes::Bytes;
use futures_util::stream;
use httpdate::HttpDate;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use super::static_path::OpenedStaticFile;

const FILE_CHUNK_BYTES: usize = 64 * 1024;

pub(crate) async fn serve_opened_file(
    opened: OpenedStaticFile,
    method: &Method,
    headers: &HeaderMap,
) -> Response<Body> {
    let modified = opened.metadata.modified().ok().map(HttpDate::from);
    let etag = weak_etag(&opened.metadata);
    if !if_unmodified_since_passes(headers, modified) {
        return empty_response(StatusCode::PRECONDITION_FAILED);
    }
    if !if_none_match_passes(headers, etag.as_deref()) {
        return empty_response(StatusCode::NOT_MODIFIED);
    }
    if !if_modified_since_is_modified(headers, modified) {
        return empty_response(StatusCode::NOT_MODIFIED);
    }

    let size = opened.metadata.len();
    let ranges = parse_range(headers, size);
    let mime = mime_guess::from_path(&opened.path_hint)
        .first_raw()
        .and_then(|value| HeaderValue::from_str(value).ok())
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        // Safe default: never cache without an explicit deployment-level
        // policy. Fingerprinted immutable assets are served through the
        // CDN/edge cache layer, which owns cache invalidation.
        .header(header::CACHE_CONTROL, "public, no-cache");
    if let Some(modified) = modified {
        builder = builder.header(header::LAST_MODIFIED, modified.to_string());
    }
    if let Some(etag) = etag {
        builder = builder.header(header::ETAG, etag);
    }

    match ranges {
        Some(Ok(ranges)) if ranges.len() == 1 => {
            let range = &ranges[0];
            let range_size = range.end().saturating_sub(*range.start()) + 1;
            let body = if method == Method::HEAD {
                Body::empty()
            } else {
                let mut file = tokio::fs::File::from_std(opened.file);
                if file.seek(SeekFrom::Start(*range.start())).await.is_err() {
                    return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
                }
                file_body(file, range_size)
            };
            finish_response(
                builder
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", range.start(), range.end(), size),
                    )
                    .header(header::CONTENT_LENGTH, range_size),
                body,
            )
        }
        Some(Ok(_)) => finish_response(
            builder
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}")),
            Body::from("Cannot serve multipart range requests"),
        ),
        Some(Err(())) => finish_response(
            builder
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}")),
            Body::empty(),
        ),
        None => {
            let body = if method == Method::HEAD {
                Body::empty()
            } else {
                file_body(tokio::fs::File::from_std(opened.file), size)
            };
            finish_response(builder.header(header::CONTENT_LENGTH, size), body)
        }
    }
}

fn parse_range(headers: &HeaderMap, size: u64) -> Option<Result<Vec<RangeInclusive<u64>>, ()>> {
    let value = headers.get(header::RANGE)?.to_str().ok()?;
    Some(
        http_range_header::parse_range_header(value)
            .and_then(|range| range.validate(size))
            .map_err(|_| ()),
    )
}

fn if_unmodified_since_passes(headers: &HeaderMap, modified: Option<HttpDate>) -> bool {
    let Some(condition) = parse_http_date_header(headers, header::IF_UNMODIFIED_SINCE) else {
        return true;
    };
    modified.is_some_and(|modified| condition >= modified)
}

/// Weak entity tag derived from modification time and size; safe for
/// byte-range responses because the tag changes whenever the file changes.
fn weak_etag(metadata: &std::fs::Metadata) -> Option<String> {
    let modified = metadata.modified().ok()?;
    let seconds = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("W/\"{seconds:x}-{:x}\"", metadata.len()))
}

/// RFC 9110 `If-None-Match`: any matching entity tag means the representation
/// is unchanged. A `*` matches when the representation exists (it always does
/// at this point). When the header is absent the check passes.
fn if_none_match_passes(headers: &HeaderMap, etag: Option<&str>) -> bool {
    let Some(condition) = headers.get(header::IF_NONE_MATCH) else {
        return true;
    };
    let Ok(condition) = condition.to_str() else {
        return true;
    };
    let Some(etag) = etag else {
        return true;
    };
    if condition.trim() == "*" {
        return false;
    }
    // Tag list is comma-separated; weak comparison strips the W/ prefix.
    condition.split(',').any(|candidate| {
        candidate.trim() == etag || candidate.trim().strip_prefix("W/") == etag.strip_prefix("W/")
    })
}

fn if_modified_since_is_modified(headers: &HeaderMap, modified: Option<HttpDate>) -> bool {
    let Some(condition) = parse_http_date_header(headers, header::IF_MODIFIED_SINCE) else {
        return true;
    };
    modified.is_none_or(|modified| condition < modified)
}

fn parse_http_date_header(headers: &HeaderMap, name: header::HeaderName) -> Option<HttpDate> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .and_then(|value| httpdate::parse_http_date(value).ok())
        .map(HttpDate::from)
}

fn file_body(file: tokio::fs::File, size: u64) -> Body {
    let chunks = stream::try_unfold((file, size), |(mut file, remaining)| async move {
        if remaining == 0 {
            return Ok(None);
        }
        let length = remaining.min(FILE_CHUNK_BYTES as u64) as usize;
        let mut buffer = vec![0_u8; length];
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "static file changed while streaming",
            ));
        }
        buffer.truncate(read);
        Ok(Some((
            Bytes::from(buffer),
            (file, remaining.saturating_sub(read as u64)),
        )))
    });
    Body::from_stream(chunks)
}

fn empty_response(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn finish_response(builder: http::response::Builder, body: Body) -> Response<Body> {
    builder
        .body(body)
        .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR))
}

#[cfg(test)]
mod tests {
    use std::{io::Write, path::PathBuf};

    use axum::body::to_bytes;
    use tempfile::NamedTempFile;

    use super::*;

    #[tokio::test]
    async fn streams_head_range_and_conditional_responses_from_open_handle() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"0123456789").unwrap();
        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=2-5"));
        let response = serve_opened_file(opened_file, &Method::GET, &headers).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(to_bytes(response.into_body(), 16).await.unwrap(), "2345");

        let opened_file = opened(&temp);
        let response = serve_opened_file(opened_file, &Method::HEAD, &HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "10");
        assert!(to_bytes(response.into_body(), 1).await.unwrap().is_empty());

        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static("Fri, 31 Dec 9999 23:59:59 GMT"),
        );
        assert_eq!(
            serve_opened_file(opened_file, &Method::GET, &headers)
                .await
                .status(),
            StatusCode::NOT_MODIFIED
        );
    }

    fn opened(temp: &NamedTempFile) -> OpenedStaticFile {
        let file = temp.reopen().unwrap();
        let metadata = file.metadata().unwrap();
        OpenedStaticFile {
            file,
            metadata,
            path_hint: PathBuf::from("asset.txt"),
        }
    }
}
