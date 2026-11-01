//! nginx `http2_chunk_size` response-body re-chunker.
//!
//! nginx caps the number of body bytes it writes into one HTTP/2 DATA frame
//! (`ngx_http_v2_module`: `http2_chunk_size`, default 8 KiB). The pinned
//! hyper 1 / h2 0.4 server path exposes **no** way to request a DATA-frame
//! boundary: `h2::SendStream::send_data` writes each body chunk as its own
//! frame (an oversized chunk may be split further by the peer's
//! `SETTINGS_MAX_FRAME_SIZE` / flow control), but it never merges distinct
//! chunks. The observable contract is therefore realized by splitting the
//! response body ourselves: a chunk at most `http2_chunk_size` bytes enters
//! hyper, so every resulting DATA frame is at most that size while the
//! reassembled entity body is byte-identical.
//!
//! The re-chunker is an outermost data-plane layer because it must run after
//! every body-producing layer, including `CompressionLayer`; the handler
//! resolves the effective per-location value and carries it on the response as
//! [`Http2ChunkSize`]. HTTP/1 requests never reach the body wrapper: nginx's
//! directive only affects `ngx_http_v2_filter_module`.

use axum::{body::Body, extract::Request, http::Version, middleware::Next, response::Response};
use bytes::Bytes;
use http_body::{Body as HttpBody, Frame, SizeHint};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// Effective nginx `http2_chunk_size` for one response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Http2ChunkSize(pub(crate) usize);

/// Outermost middleware: split an HTTP/2 response body into bounded chunks.
pub(crate) async fn apply_http2_chunking(request: Request, next: Next) -> Response<Body> {
    let http2 = request.version() == Version::HTTP_2;
    let mut response = next.run(request).await;
    if !http2 {
        return response;
    }
    let Some(chunk_size) = response
        .extensions_mut()
        .remove::<Http2ChunkSize>()
        .map(|size| size.0)
    else {
        return response;
    };
    if chunk_size == 0 {
        return response;
    }
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::new(RechunkBody {
            inner: body,
            chunk_size,
            buffered: None,
        }),
    )
}

/// A body wrapper that yields at most `chunk_size` bytes per frame.
///
/// Chunks already at or below the boundary pass through unchanged (no copy);
/// a larger frame is split with `Bytes::split_off`, so splitting is zero-copy
/// and the concatenated data (and any trailers) stay byte-identical.
struct RechunkBody {
    inner: Body,
    chunk_size: usize,
    buffered: Option<Bytes>,
}

impl HttpBody for RechunkBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        if let Some(mut buffered) = this.buffered.take() {
            if buffered.len() > this.chunk_size {
                let rest = buffered.split_off(this.chunk_size);
                this.buffered = Some(rest);
            }
            return Poll::Ready(Some(Ok(Frame::data(buffered))));
        }
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                Ok(mut data) => {
                    if data.len() > this.chunk_size {
                        let rest = data.split_off(this.chunk_size);
                        this.buffered = Some(rest);
                    }
                    Poll::Ready(Some(Ok(Frame::data(data))))
                }
                // Trailers (and any other non-data frame) pass through as-is.
                Err(frame) => Poll::Ready(Some(Ok(frame))),
            },
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.buffered.is_none() && self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    fn body_of(chunks: Vec<&'static [u8]>) -> Body {
        Body::from_stream(futures_util::stream::iter(
            chunks
                .into_iter()
                .map(|chunk| Ok::<_, std::io::Error>(Bytes::from_static(chunk))),
        ))
    }

    async fn collected(body: Body) -> Vec<Bytes> {
        let mut stream = body.into_data_stream();
        let mut frames = Vec::new();
        while let Some(chunk) = stream.next().await {
            frames.push(chunk.expect("body chunk"));
        }
        frames
    }

    #[tokio::test]
    async fn splits_oversized_chunks_and_preserves_the_body() {
        let body = Body::new(RechunkBody {
            inner: body_of(vec![b"abcdefghij", b"klm"]),
            chunk_size: 4,
            buffered: None,
        });
        let frames = collected(body).await;
        assert_eq!(
            frames,
            vec![
                Bytes::from_static(b"abcd"),
                Bytes::from_static(b"efgh"),
                Bytes::from_static(b"ij"),
                Bytes::from_static(b"klm"),
            ]
        );
        assert_eq!(
            frames.concat(),
            b"abcdefghijklm".to_vec(),
            "the reassembled body must be byte-identical"
        );
    }

    #[tokio::test]
    async fn small_chunks_pass_through_unchanged() {
        let body = Body::new(RechunkBody {
            inner: body_of(vec![b"abc", b"de"]),
            chunk_size: 4,
            buffered: None,
        });
        assert_eq!(
            collected(body).await,
            vec![Bytes::from_static(b"abc"), Bytes::from_static(b"de")]
        );
    }
}
