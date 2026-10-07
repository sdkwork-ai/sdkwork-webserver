use async_trait::async_trait;
use sdkwork_webserver_contract::provider::{
    WebsiteProviderContentStream, WebsiteProviderError, WebsiteProviderErrorKind,
    WebsiteProviderResult,
};

use crate::sdk::DriveContentChunkStream;

/// Idle-chunk deadline: a facade that accepts the request and then stalls
/// mid-body must fail the stream instead of holding the delivery runtime's
/// buffered-content permit until TCP-level timeouts. Client-paced downloads
/// are unaffected — the deadline applies per chunk, not to the whole body.
const CHUNK_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Forwards the SDK's bounded chunk stream while enforcing the expected
/// content length: the object must deliver exactly `expected_length` bytes,
/// otherwise the contract is violated (fail closed, no partial acceptance).
pub(crate) struct BoundedDriveContentStream {
    source: Option<Box<dyn DriveContentChunkStream>>,
    remaining: u64,
}

impl BoundedDriveContentStream {
    pub(crate) fn new(source: Box<dyn DriveContentChunkStream>, expected_length: u64) -> Self {
        Self {
            source: Some(source),
            remaining: expected_length,
        }
    }
}

#[async_trait]
impl WebsiteProviderContentStream for BoundedDriveContentStream {
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
                Ok(Some(bytes))
            }
            None => {
                self.source = None;
                if self.remaining != 0 {
                    return Err(WebsiteProviderError::new(
                        WebsiteProviderErrorKind::ContractMismatch,
                    ));
                }
                Ok(None)
            }
        }
    }
}
