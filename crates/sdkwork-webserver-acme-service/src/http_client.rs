use std::error::Error as StdError;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::{Buf, Bytes, BytesMut};
use http::header::CONTENT_LENGTH;
use http::{Request, Response};
use http_body::Body;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use instant_acme::{BodyWrapper, BytesBody, BytesResponse, Error as InstantAcmeError, HttpClient};
use rustls_pki_types::CertificateDer;

use crate::{AcmeServiceError, AcmeServiceResult};

pub(crate) const MAX_ACME_RESPONSE_BODY_BYTES: usize = 2 * 1024 * 1024;

type AcmeHyperClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, BodyWrapper<Bytes>>;

/// Builds the bounded ACME HTTP client used for CA directory requests.
///
/// The default factory uses the platform verifier; a custom factory can add
/// private CA trust roots (for example a local Pebble test server or an
/// internal ACME directory).
pub trait AcmeHttpClientFactory: Send + Sync {
    fn build(&self) -> AcmeServiceResult<Box<dyn HttpClient>>;
}

/// Default factory: platform trust store (Let's Encrypt and public CAs).
#[derive(Default)]
pub struct PlatformVerifierClientFactory;

impl AcmeHttpClientFactory for PlatformVerifierClientFactory {
    fn build(&self) -> AcmeServiceResult<Box<dyn HttpClient>> {
        Ok(Box::new(BoundedAcmeHttpClient::new()?))
    }
}

/// Factory that trusts the platform roots plus explicit extra roots.
pub struct ExtraRootsClientFactory {
    extra_roots: Vec<CertificateDer<'static>>,
}

impl ExtraRootsClientFactory {
    pub fn new(extra_roots: Vec<CertificateDer<'static>>) -> Self {
        Self { extra_roots }
    }
}

impl AcmeHttpClientFactory for ExtraRootsClientFactory {
    fn build(&self) -> AcmeServiceResult<Box<dyn HttpClient>> {
        Ok(Box::new(BoundedAcmeHttpClient::new_with_roots(
            self.extra_roots.clone(),
        )?))
    }
}

pub(crate) struct BoundedAcmeHttpClient {
    client: AcmeHyperClient,
}

impl BoundedAcmeHttpClient {
    pub(crate) fn new() -> AcmeServiceResult<Self> {
        let connector = HttpsConnectorBuilder::new()
            .try_with_platform_verifier()
            .map_err(|error| {
                AcmeServiceError::provider(format!("initialize ACME TLS verifier: {error}"))
            })?
            .https_only()
            .enable_http1()
            .enable_http2()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        Ok(Self { client })
    }

    /// ACME client that trusts the platform roots plus `extra_roots`.
    ///
    /// This is the integration point for private ACME CAs (for example a
    /// local Pebble test server or an internal CA directory) whose chain is
    /// not part of the platform trust store. The caller provides the complete
    /// set of trust anchors the CA chain is verified against. Production
    /// Let's Encrypt issuance keeps using [`Self::new`].
    pub(crate) fn new_with_roots(
        extra_roots: Vec<CertificateDer<'static>>,
    ) -> AcmeServiceResult<Self> {
        if extra_roots.is_empty() {
            return Err(AcmeServiceError::config(
                "at least one extra ACME trust root is required",
            ));
        }
        let mut roots = rustls::RootCertStore::empty();
        for root in extra_roots {
            roots
                .add(root)
                .map_err(|error| AcmeServiceError::Internal(format!("add extra root: {error}")))?;
        }
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            provider.clone(),
        )
        .build()
        .map_err(|error| {
            AcmeServiceError::provider(format!("initialize ACME TLS verifier: {error}"))
        })?;
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|error| {
                AcmeServiceError::provider(format!(
                    "initialize ACME TLS protocol versions: {error}"
                ))
            })?
            .with_webpki_verifier(verifier)
            .with_no_client_auth();
        let connector = HttpsConnectorBuilder::new()
            .with_tls_config(config)
            .https_only()
            .enable_http1()
            .enable_http2()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        Ok(Self { client })
    }
}

impl HttpClient for BoundedAcmeHttpClient {
    fn request(
        &self,
        request: Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, InstantAcmeError>> + Send>> {
        let client = self.client.clone();
        Box::pin(async move {
            let response = client
                .request(request)
                .await
                .map_err(|error| InstantAcmeError::Other(Box::new(error)))?;
            bounded_response(response)
        })
    }
}

fn bounded_response(response: Response<Incoming>) -> Result<BytesResponse, InstantAcmeError> {
    let (parts, body) = response.into_parts();
    if let Some(value) = parts.headers.get(CONTENT_LENGTH) {
        let length = value
            .to_str()
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| acme_http_error("ACME response has invalid Content-Length"))?;
        if length > MAX_ACME_RESPONSE_BODY_BYTES as u64 {
            return Err(acme_http_error("ACME response body exceeds 2 MiB"));
        }
    }
    Ok(BytesResponse {
        parts,
        body: Box::new(BoundedResponseBody::new(body, MAX_ACME_RESPONSE_BODY_BYTES)),
    })
}

struct BoundedResponseBody<B> {
    body: Option<B>,
    max_bytes: usize,
}

impl<B> BoundedResponseBody<B> {
    fn new(body: B, max_bytes: usize) -> Self {
        Self {
            body: Some(body),
            max_bytes,
        }
    }
}

#[async_trait]
impl<B> BytesBody for BoundedResponseBody<B>
where
    B: Body + Send + Unpin + 'static,
    B::Data: Buf + Send,
    B::Error: Into<Box<dyn StdError + Send + Sync + 'static>>,
{
    async fn into_bytes(&mut self) -> Result<Bytes, Box<dyn StdError + Send + Sync + 'static>> {
        let Some(mut body) = self.body.take() else {
            return Ok(Bytes::new());
        };
        if body.size_hint().lower() > self.max_bytes as u64 {
            return Err(bounded_body_error());
        }

        let initial_capacity = body
            .size_hint()
            .upper()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0)
            .min(self.max_bytes);
        let mut collected = BytesMut::with_capacity(initial_capacity);
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(Into::into)?;
            let Ok(mut data) = frame.into_data() else {
                continue;
            };
            let next_len = collected
                .len()
                .checked_add(data.remaining())
                .ok_or_else(bounded_body_error)?;
            if next_len > self.max_bytes {
                return Err(bounded_body_error());
            }
            while data.has_remaining() {
                let chunk = data.chunk();
                collected.extend_from_slice(chunk);
                let chunk_len = chunk.len();
                data.advance(chunk_len);
            }
        }
        Ok(collected.freeze())
    }
}

fn acme_http_error(message: &'static str) -> InstantAcmeError {
    InstantAcmeError::Other(Box::new(io::Error::new(
        io::ErrorKind::InvalidData,
        message,
    )))
}

fn bounded_body_error() -> Box<dyn StdError + Send + Sync + 'static> {
    Box::new(io::Error::new(
        io::ErrorKind::InvalidData,
        "ACME response body exceeds configured maximum",
    ))
}

#[cfg(test)]
mod tests {
    use http_body_util::Full;

    use super::*;

    #[tokio::test]
    async fn bounded_body_accepts_limit_and_rejects_excess() {
        let mut accepted = BoundedResponseBody::new(Full::new(Bytes::from(vec![1_u8; 16])), 16);
        assert_eq!(accepted.into_bytes().await.expect("accepted").len(), 16);

        let mut rejected = BoundedResponseBody::new(Full::new(Bytes::from(vec![1_u8; 17])), 16);
        assert!(rejected.into_bytes().await.is_err());
    }
}
