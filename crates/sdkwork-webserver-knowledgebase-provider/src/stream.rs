use async_trait::async_trait;
use sdkwork_webserver_contract::provider::{
    WebsiteProviderContentStream, WebsiteProviderError, WebsiteProviderErrorKind,
    WebsiteProviderResult,
};

use crate::sdk::WikiContentChunkStream;

/// Idle-chunk deadline: a facade that accepts the request and then stalls
/// mid-body must fail the stream instead of holding the delivery runtime's
/// buffered-content permit until TCP-level timeouts. Client-paced downloads
/// are unaffected — the deadline applies per chunk, not to the whole body.
const CHUNK_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Forwards the SDK's bounded chunk stream while enforcing the configured
/// byte ceiling: any chunk that would exceed the ceiling fails closed with a
/// contract mismatch instead of buffering the remainder. When the facade
/// declared a content length (via [`Self::with_expected_length`]), the stream
/// additionally enforces that exact length at clean EOF — the same contract
/// the drive stream applies.
pub(crate) struct BoundedWikiContentStream {
    source: Option<Box<dyn WikiContentChunkStream>>,
    remaining: u64,
    expected: Option<u64>,
    delivered: u64,
}

impl BoundedWikiContentStream {
    pub(crate) fn new(source: Box<dyn WikiContentChunkStream>, maximum_bytes: u64) -> Self {
        Self {
            source: Some(source),
            remaining: maximum_bytes,
            expected: None,
            delivered: 0,
        }
    }

    /// Declares the exact length the facade reported; a short body then fails
    /// closed at clean EOF instead of surfacing downstream.
    pub(crate) fn with_expected_length(mut self, expected: u64) -> Self {
        self.expected = Some(expected);
        self
    }
}

#[async_trait]
impl WebsiteProviderContentStream for BoundedWikiContentStream {
    async fn next_chunk(&mut self) -> WebsiteProviderResult<Option<Vec<u8>>> {
        let Some(source) = self.source.as_mut() else {
            return Ok(None);
        };
        let chunk = tokio::time::timeout(CHUNK_IDLE_TIMEOUT, source.next_chunk())
            .await
            .map_err(|_| WebsiteProviderError::new(WebsiteProviderErrorKind::DeadlineExceeded))?
            .map_err(|_| WebsiteProviderError::new(WebsiteProviderErrorKind::ContractMismatch))?;
        match chunk {
            Some(bytes) => {
                let length = u64::try_from(bytes.len()).map_err(|_| {
                    WebsiteProviderError::new(WebsiteProviderErrorKind::ContractMismatch)
                })?;
                if length > self.remaining {
                    self.source = None;
                    return Err(WebsiteProviderError::new(
                        WebsiteProviderErrorKind::ContractMismatch,
                    ));
                }
                self.remaining -= length;
                self.delivered += length;
                Ok(Some(bytes))
            }
            None => {
                self.source = None;
                if self.expected.is_some_and(|expected| self.delivered != expected) {
                    return Err(WebsiteProviderError::new(
                        WebsiteProviderErrorKind::ContractMismatch,
                    ));
                }
                Ok(None)
            }
        }
    }
}
