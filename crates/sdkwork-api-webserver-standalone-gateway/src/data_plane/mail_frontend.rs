//! nginx `mail {}` frontend listener plane (Wave 9B).
//!
//! The plane owns:
//!
//! - TCP listeners with optional inbound PROXY protocol v1, bounded client
//!   command framing and idle timeouts;
//! - the nginx POP3/IMAP/SMTP greeting and authorization dialog driven by the
//!   transport-free [`sdkwork_webserver_mail_protocol`] state machines;
//! - the `auth_http` authentication exchange (nginx's bounded
//!   `GET <uri> HTTP/1.0` request and `Auth-*` response grammar);
//! - frontend TLS: implicit TLS (`listen … ssl`) and STARTTLS (`starttls
//!   on|only`) with the configured certificate;
//! - the backend relay: connect to the `Auth-Server:Auth-Port` target, optional
//!   outbound PROXY protocol v1, optional SMTP `XCLIENT`, backend client
//!   authentication, and a bounded bidirectional relay with half-close;
//! - the optional typed-plane backend TLS extension (`proxy.tls`).
//!
//! Everything the runtime cannot honour fails closed at configuration
//! materialization (see `sdkwork_webserver_core::nginx::mapping`), never by
//! silently ignoring an operator declaration.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use sdkwork_webserver_core::{
    MailAuthMechanism, MailProxyConfig, MailProxyTlsConfig, MailServerConfig, MailServerProtocol,
    MailStarttlsMode, MailTlsProtocol,
};
use sdkwork_webserver_mail_protocol::{
    auth::{decode_login_response, decode_plain},
    frontend::{
        FrontendAction, FrontendSession, FrontendState as FrontendSessionState, MailProtocol,
    },
    line::LineReader,
    proxy_protocol::ProxyProtocolV1,
    xclient::{XclientAttribute, XclientRequest},
    Limits,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{lookup_host, TcpListener, TcpStream},
    sync::watch,
    time::{sleep, timeout},
};

use super::{accept, DataPlaneError, DataPlaneRuntime};

/// Maximum client commands one session processes before closing.
const MAX_SESSION_COMMANDS: usize = 64;
/// Read chunk for one client command batch.
const MAIL_READ_CHUNK_BYTES: usize = 4096;
/// Base64 prompts nginx sends for LOGIN/`AUTH LOGIN` continuations.
const LOGIN_USER_PROMPT: &str = "VXNlcm5hbWU6";
const LOGIN_PASSWORD_PROMPT: &str = "UGFzc3dvcmQ6";
/// nginx mail `auth_http` retries after an `Auth-Status: WAIT` response.
const MAX_AUTH_HTTP_WAIT_RETRIES: u32 = 5;
/// Upper bound for one auth_http request/response exchange.
const MAX_AUTH_HTTP_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_AUTH_HTTP_HEADERS: usize = 64;
/// Upper bound for one backend protocol line (nginx `proxy_buffer` scales the
/// real bound; this is the hard ceiling).
const MAX_BACKEND_LINE_BYTES: usize = 1024 * 1024;
/// How long a frontend STARTTLS/TLS handshake may take.
const MAIL_TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// One prepared mail listener.
pub(crate) struct PreparedMailListener {
    pub(crate) id: String,
    socket: TcpListener,
    profile: Arc<MailConnectionProfile>,
}

/// Resolved per-connection configuration shared by all accepted sessions.
struct MailConnectionProfile {
    id: String,
    protocol: MailProtocol,
    greeting_name: String,
    timeout: Duration,
    so_keepalive: bool,
    inbound_proxy_protocol: bool,
    max_line_bytes: usize,
    greeting_delay: Duration,
    mechanisms: Vec<MailAuthMechanism>,
    max_errors: u32,
    auth_http: Option<AuthHttpProfile>,
    tls: Option<MailTlsProfile>,
    proxy: MailProxyProfile,
}

/// Resolved `auth_http` service profile.
struct AuthHttpProfile {
    address: SocketAddr,
    host_header: String,
    path: String,
    headers: Vec<String>,
    timeout: Duration,
    pass_client_cert: bool,
}

/// Resolved frontend TLS profile.
struct MailTlsProfile {
    acceptor: tokio_rustls::TlsAcceptor,
    starttls: MailStarttlsMode,
    implicit: bool,
}

/// Resolved backend relay profile.
struct MailProxyProfile {
    xclient: bool,
    smtp_auth: bool,
    proxy_protocol: bool,
    pass_error_message: bool,
    buffer_bytes: usize,
    timeout: Duration,
    tls: Option<BackendTlsProfile>,
}

struct BackendTlsProfile {
    config: Arc<ClientConfig>,
    server_name: Option<String>,
}

impl MailProxyProfile {
    fn from_config(config: &MailProxyConfig) -> Result<Self, DataPlaneError> {
        let tls = match &config.tls {
            None => None,
            Some(tls) => Some(build_backend_tls_profile(tls)?),
        };
        Ok(Self {
            xclient: config.xclient,
            smtp_auth: config.smtp_auth,
            proxy_protocol: config.proxy_protocol,
            pass_error_message: config.pass_error_message,
            buffer_bytes: config.buffer_bytes.max(512),
            timeout: Duration::from_millis(config.timeout_ms.max(1)),
            tls,
        })
    }
}

impl PreparedMailListener {
    /// Bind the listener socket and resolve the bounded session profile.
    pub(crate) async fn prepare(config: &MailServerConfig) -> Result<Self, DataPlaneError> {
        // The mail plane may be the only plane in the process (a mail-only
        // configuration), so it installs the rustls crypto provider itself
        // before any TLS material is built.
        super::tls_material::install_crypto_provider()?;
        let address: SocketAddr =
            format!("{}:{}", config.bind, config.port)
                .parse()
                .map_err(|source| DataPlaneError::InvalidBind {
                    bind: config.bind.clone(),
                    source,
                })?;
        let socket = super::server::bind_listener(address)
            .await
            .map_err(|source| DataPlaneError::Listener {
                listener_id: config.id.clone(),
                source,
            })?;
        let protocol = match config.protocol {
            MailServerProtocol::Pop3 => MailProtocol::Pop3,
            MailServerProtocol::Imap => MailProtocol::Imap,
            MailServerProtocol::Smtp => MailProtocol::Smtp,
        };
        let greeting_delay = match config.protocol {
            MailServerProtocol::Smtp => Duration::from_millis(config.smtp_greeting_delay_ms),
            MailServerProtocol::Pop3 | MailServerProtocol::Imap => Duration::ZERO,
        };
        let auth_http = match &config.auth_http {
            None => None,
            Some(auth) => Some(resolve_auth_http_profile(auth).await?),
        };
        let tls = match &config.tls {
            None => None,
            Some(tls) => Some(build_mail_tls_profile(&config.id, tls)?),
        };
        if auth_http.is_some() && tls.is_none() && config.tls.is_some() {
            return Err(DataPlaneError::MailConfiguration {
                mail_server_id: config.id.clone(),
                detail: "mail TLS material is declared but unused; enable `listen … ssl` or `starttls on|only`".to_owned(),
            });
        }
        let profile = MailConnectionProfile {
            id: config.id.clone(),
            protocol,
            greeting_name: config.effective_server_name(),
            timeout: Duration::from_millis(config.timeout_ms),
            so_keepalive: config.so_keepalive,
            inbound_proxy_protocol: config.proxy_protocol,
            max_line_bytes: config.client_buffer_bytes(),
            greeting_delay,
            mechanisms: config.auth_mechanisms().to_vec(),
            max_errors: config.max_errors,
            auth_http,
            tls,
            proxy: MailProxyProfile::from_config(&config.proxy)?,
        };
        tracing::info!(
            mail_server_id = %config.id,
            protocol = config.protocol.as_str(),
            address = %address,
            proxy_protocol = config.proxy_protocol,
            auth_http = profile.auth_http.is_some(),
            tls = profile.tls.is_some(),
            "mail listener prepared"
        );
        Ok(Self {
            id: config.id.clone(),
            socket,
            profile: Arc::new(profile),
        })
    }
}

/// Resolve the `auth_http` endpoint once at listener preparation.
async fn resolve_auth_http_profile(
    config: &sdkwork_webserver_core::MailAuthHttpConfig,
) -> Result<AuthHttpProfile, DataPlaneError> {
    let raw = config
        .uri
        .strip_prefix("http://")
        .unwrap_or(config.uri.as_str())
        .trim();
    let (authority, path) = match raw.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (raw, "/".to_owned()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => {
            let port = port
                .parse::<u16>()
                .map_err(|source| DataPlaneError::MailConfiguration {
                    mail_server_id: "auth_http".to_owned(),
                    detail: format!("invalid auth_http port in `{raw}`: {source}"),
                })?;
            (host.trim_matches(['[', ']']), port)
        }
        _ => (authority.trim_matches(['[', ']']), 80),
    };
    if host.is_empty() {
        return Err(DataPlaneError::MailConfiguration {
            mail_server_id: "auth_http".to_owned(),
            detail: format!("auth_http `{raw}` has no host"),
        });
    }
    let mut resolved =
        lookup_host((host, port))
            .await
            .map_err(|source| DataPlaneError::MailConfiguration {
                mail_server_id: "auth_http".to_owned(),
                detail: format!("cannot resolve auth_http host `{host}`: {source}"),
            })?;
    let address = resolved
        .next()
        .ok_or_else(|| DataPlaneError::MailConfiguration {
            mail_server_id: "auth_http".to_owned(),
            detail: format!("auth_http host `{host}` resolved to no address"),
        })?;
    Ok(AuthHttpProfile {
        address,
        host_header: config
            .host_header
            .clone()
            .unwrap_or_else(|| host.to_owned()),
        path,
        headers: config.headers.clone(),
        timeout: Duration::from_millis(config.timeout_ms.max(1)),
        pass_client_cert: config.pass_client_cert,
    })
}

/// Build the frontend rustls server configuration from the mail TLS material.
fn build_mail_tls_profile(
    server_id: &str,
    tls: &sdkwork_webserver_core::MailTlsConfig,
) -> Result<MailTlsProfile, DataPlaneError> {
    let certificate_pem = read_mail_tls_material(server_id, &tls.certificate_file)?;
    let private_key_pem = read_mail_tls_material(server_id, &tls.private_key_file)?;
    let chain = parse_mail_certificate_chain(server_id, &certificate_pem)?;
    let private_key = parse_mail_private_key(server_id, &private_key_pem)?;
    let versions: Vec<&'static rustls::SupportedProtocolVersion> = tls
        .protocols
        .iter()
        .map(|protocol| match protocol {
            MailTlsProtocol::Tls12 => &rustls::version::TLS12,
            MailTlsProtocol::Tls13 => &rustls::version::TLS13,
        })
        .collect();
    let config = ServerConfig::builder_with_protocol_versions(&versions)
        .with_no_client_auth()
        .with_single_cert(chain, private_key)
        .map_err(|source| DataPlaneError::MailTlsMaterial {
            mail_server_id: server_id.to_owned(),
            detail: format!("invalid mail certificate/key: {source}"),
        })?;
    Ok(MailTlsProfile {
        acceptor: tokio_rustls::TlsAcceptor::from(Arc::new(config)),
        starttls: tls.starttls,
        implicit: tls.implicit,
    })
}

fn read_mail_tls_material(server_id: &str, path: &str) -> Result<Vec<u8>, DataPlaneError> {
    std::fs::read(path).map_err(|source| DataPlaneError::MailTlsMaterial {
        mail_server_id: server_id.to_owned(),
        detail: format!("cannot read `{path}`: {source}"),
    })
}

fn parse_mail_certificate_chain(
    server_id: &str,
    pem: &[u8],
) -> Result<Vec<CertificateDer<'static>>, DataPlaneError> {
    let chain = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| DataPlaneError::MailTlsMaterial {
            mail_server_id: server_id.to_owned(),
            detail: format!("invalid certificate PEM: {source}"),
        })?;
    if chain.is_empty() {
        return Err(DataPlaneError::MailTlsMaterial {
            mail_server_id: server_id.to_owned(),
            detail: "certificate file contains no PEM certificates".to_owned(),
        });
    }
    Ok(chain)
}

fn parse_mail_private_key(
    server_id: &str,
    pem: &[u8],
) -> Result<PrivateKeyDer<'static>, DataPlaneError> {
    PrivateKeyDer::from_pem_slice(pem).map_err(|source| DataPlaneError::MailTlsMaterial {
        mail_server_id: server_id.to_owned(),
        detail: format!("invalid private key PEM: {source}"),
    })
}

/// Build the typed-plane backend TLS client configuration.
fn build_backend_tls_profile(
    tls: &MailProxyTlsConfig,
) -> Result<BackendTlsProfile, DataPlaneError> {
    let mut roots = RootCertStore::empty();
    match &tls.trusted_certificate_file {
        Some(path) => {
            let pem = std::fs::read(path).map_err(|source| DataPlaneError::MailTlsMaterial {
                mail_server_id: "proxy.tls".to_owned(),
                detail: format!("cannot read backend trust anchor `{path}`: {source}"),
            })?;
            let mut loaded = 0usize;
            for certificate in CertificateDer::pem_slice_iter(&pem) {
                let certificate =
                    certificate.map_err(|source| DataPlaneError::MailTlsMaterial {
                        mail_server_id: "proxy.tls".to_owned(),
                        detail: format!("invalid backend trust anchor in `{path}`: {source}"),
                    })?;
                roots
                    .add(certificate)
                    .map_err(|source| DataPlaneError::MailTlsMaterial {
                        mail_server_id: "proxy.tls".to_owned(),
                        detail: format!("invalid backend trust anchor in `{path}`: {source}"),
                    })?;
                loaded = loaded.saturating_add(1);
            }
            if loaded == 0 {
                return Err(DataPlaneError::MailTlsMaterial {
                    mail_server_id: "proxy.tls".to_owned(),
                    detail: format!("backend trust anchor `{path}` contains no certificates"),
                });
            }
        }
        None => {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
    }
    let builder = if tls.verify {
        ClientConfig::builder().with_root_certificates(roots)
    } else {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerification::new()?))
    };
    let config = match (&tls.certificate_file, &tls.certificate_key_file) {
        (Some(certificate), Some(private_key)) => {
            let certificate_pem =
                std::fs::read(certificate).map_err(|source| DataPlaneError::MailTlsMaterial {
                    mail_server_id: "proxy.tls".to_owned(),
                    detail: format!(
                        "cannot read backend client certificate `{certificate}`: {source}"
                    ),
                })?;
            let key_pem =
                std::fs::read(private_key).map_err(|source| DataPlaneError::MailTlsMaterial {
                    mail_server_id: "proxy.tls".to_owned(),
                    detail: format!("cannot read backend client key `{private_key}`: {source}"),
                })?;
            let chain = parse_mail_certificate_chain("proxy.tls", &certificate_pem)?;
            let key = parse_mail_private_key("proxy.tls", &key_pem)?;
            builder
                .with_client_auth_cert(chain, key)
                .map_err(|source| DataPlaneError::MailTlsMaterial {
                    mail_server_id: "proxy.tls".to_owned(),
                    detail: format!("invalid backend client identity: {source}"),
                })?
        }
        (None, None) => builder.with_no_client_auth(),
        _ => {
            return Err(DataPlaneError::MailTlsMaterial {
                mail_server_id: "proxy.tls".to_owned(),
                detail:
                    "backend client identity requires both certificateFile and certificateKeyFile"
                        .to_owned(),
            })
        }
    };
    Ok(BackendTlsProfile {
        config: Arc::new(config),
        server_name: tls.server_name.clone(),
    })
}

/// Certificate verifier for the explicit `verify: false` typed-plane opt-out.
#[derive(Debug)]
struct NoCertificateVerification {
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl NoCertificateVerification {
    fn new() -> Result<Self, DataPlaneError> {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .ok_or(DataPlaneError::TlsCryptoProvider)?;
        Ok(Self {
            algorithms: provider.signature_verification_algorithms,
        })
    }
}

impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, certificate, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, certificate, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// Accept loop for one mail listener.
pub(crate) async fn serve_mail_listener(
    runtime: Arc<DataPlaneRuntime>,
    listener: PreparedMailListener,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), DataPlaneError> {
    let mut tasks = tokio::task::JoinSet::new();
    let mut accept_policy = accept::AcceptRetryPolicy::new();
    let profile = Arc::clone(&listener.profile);
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            joined = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(error)) = joined {
                    tracing::warn!(mail_server_id = %listener.id, error = %error, "mail connection task failed");
                }
            }
            accepted = listener.socket.accept() => {
                let (downstream, peer) = match accepted {
                    Ok(accepted) => {
                        accept_policy.on_success();
                        accepted
                    }
                    Err(error) => {
                        if accept_policy.on_error(&error) {
                            tokio::time::sleep(accept::ACCEPT_RETRY_PAUSE).await;
                            continue;
                        }
                        tracing::error!(mail_server_id = %listener.id, error = %error, "mail listener accept failed terminally");
                        return Err(DataPlaneError::Listener {
                            listener_id: listener.id.clone(),
                            source: error,
                        });
                    }
                };
                let permit = match Arc::clone(&runtime.connection_permits).try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        tracing::warn!(
                            mail_server_id = %listener.id,
                            peer = %peer,
                            "mail connection rejected by admission budget"
                        );
                        drop(downstream);
                        continue;
                    }
                };
                let profile = Arc::clone(&profile);
                tasks.spawn(async move {
                    let result = serve_mail_connection(&profile, downstream, peer).await;
                    drop(permit);
                    result
                });
            }
        }
    }
    drop(listener.socket);
    // Bounded drain: mail sessions end through their idle timeout, so a
    // shutdown cannot wait for them indefinitely.
    let drain_timeout =
        Duration::from_millis(runtime.current().app.config().limits.drain_timeout_ms);
    let drain = async {
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                tracing::warn!(mail_server_id = %listener.id, error = %error, "mail connection task failed");
            }
        }
    };
    if drain_timeout.is_zero() {
        drain.await;
    } else {
        tokio::select! {
            () = drain => {}
            () = sleep(drain_timeout) => {
                let remaining = tasks.len();
                tasks.abort_all();
                tracing::info!(
                    mail_server_id = %listener.id,
                    remaining,
                    drain_timeout_ms = drain_timeout.as_millis() as u64,
                    "mail listener drained with remaining sessions aborted"
                );
            }
        }
    }
    Ok(())
}

/// The client-facing byte stream: plaintext or TLS (implicit or STARTTLS).
enum MailStream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::server::TlsStream<TcpStream>>),
}

impl MailStream {
    async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer).await,
            Self::Tls(stream) => stream.read(buffer).await,
        }
    }

    async fn shutdown(&mut self) {
        let _ = match self {
            Self::Plain(stream) => stream.shutdown().await,
            Self::Tls(stream) => stream.shutdown().await,
        };
    }

    fn into_plain(self) -> Option<TcpStream> {
        match self {
            Self::Plain(stream) => Some(stream),
            Self::Tls(_) => None,
        }
    }

    /// `(protocol, cipher-suite)` of the established frontend TLS session.
    fn tls_facts(&self) -> Option<(String, String)> {
        let Self::Tls(stream) = self else {
            return None;
        };
        let connection = stream.get_ref().1;
        let protocol = match connection.protocol_version()? {
            rustls::ProtocolVersion::TLSv1_2 => "TLSv1.2".to_owned(),
            rustls::ProtocolVersion::TLSv1_3 => "TLSv1.3".to_owned(),
            other => format!("{other:?}"),
        };
        let cipher = connection
            .negotiated_cipher_suite()
            .map(|suite| format!("{:?}", suite.suite()))
            .unwrap_or_default();
        Some((protocol, cipher))
    }
}

/// The backend byte stream: plaintext or TLS.
enum BackendStream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AsyncRead for BackendStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for BackendStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
        }
    }
}

impl AsyncRead for MailStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MailStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            Self::Tls(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
        }
    }
}

/// Facts captured from the frontend dialog that feed the auth exchange.
struct AuthAttempt {
    user: String,
    password: Option<String>,
    /// nginx `Auth-Method` token (`plain` for the supported dialogs).
    method: &'static str,
    /// IMAP tag of the authorization command (reused for the backend login).
    imap_tag: Option<String>,
}

/// What the frontend dialog decided to do next.
enum DialogOutcome {
    Close,
    Authenticate(AuthAttempt),
    StartTls,
}

/// Drive one mail session end to end.
async fn serve_mail_connection(
    profile: &MailConnectionProfile,
    mut raw: TcpStream,
    peer: SocketAddr,
) -> Result<(), ()> {
    if profile.so_keepalive {
        if let Err(error) = socket2::SockRef::from(&raw).set_keepalive(true) {
            tracing::warn!(
                mail_server_id = %profile.id,
                peer = %peer,
                error = %error,
                "mail listener could not enable SO_KEEPALIVE"
            );
        }
    }
    let limits = session_limits(profile.max_line_bytes);
    let mut pending: Vec<u8> = Vec::new();
    let mut proxy_header: Option<ProxyProtocolV1> = None;
    let mut client_ip = peer.ip();
    if profile.inbound_proxy_protocol {
        match read_proxy_v1(&mut raw, &limits, profile.timeout).await {
            Ok((header, leftover)) => {
                pending = leftover;
                if let Some(address) = header.source() {
                    client_ip = address.ip();
                }
                proxy_header = Some(header);
            }
            Err(reason) => {
                tracing::warn!(
                    mail_server_id = %profile.id,
                    peer = %peer,
                    reason,
                    "mail PROXY protocol header rejected; closing connection"
                );
                return Err(());
            }
        }
    }
    let local_addr = raw.local_addr().ok();
    let implicit_tls = profile.tls.as_ref().is_some_and(|tls| tls.implicit);
    let mut stream = if implicit_tls {
        let tls = profile.tls.as_ref().expect("checked implicit tls");
        match timeout(MAIL_TLS_HANDSHAKE_TIMEOUT, tls.acceptor.accept(raw)).await {
            Ok(Ok(stream)) => MailStream::Tls(Box::new(stream)),
            Ok(Err(error)) => {
                tracing::info!(mail_server_id = %profile.id, peer = %peer, error = %error, "mail implicit TLS handshake failed");
                return Err(());
            }
            Err(_) => {
                tracing::info!(mail_server_id = %profile.id, peer = %peer, "mail implicit TLS handshake timed out");
                return Err(());
            }
        }
    } else {
        MailStream::Plain(raw)
    };
    if !profile.greeting_delay.is_zero() {
        sleep(profile.greeting_delay).await;
    }
    let mut state = MailSessionState::new(profile, peer, client_ip, implicit_tls, &limits);
    state.proxy_local_addr = local_addr;
    let greeting = state.greeting();
    if write_mail(&mut stream, &greeting, profile.timeout)
        .await
        .is_err()
    {
        return Err(());
    }
    let mut slot = Some(stream);
    loop {
        let outcome = run_dialog(
            profile,
            &mut state,
            slot.as_mut().expect("mail stream present"),
            &mut pending,
            &limits,
        )
        .await;
        match outcome {
            DialogOutcome::Close => break,
            DialogOutcome::StartTls => {
                let stream = slot.take().expect("mail stream present");
                let Some(plain) = stream.into_plain() else {
                    break;
                };
                let Some(tls) = profile.tls.as_ref() else {
                    break;
                };
                match timeout(MAIL_TLS_HANDSHAKE_TIMEOUT, tls.acceptor.accept(plain)).await {
                    Ok(Ok(tls_stream)) => {
                        // nginx restarts the protocol parser after STARTTLS but
                        // does NOT resend the greeting (verified against nginx
                        // 1.29.6 by the mail differential slice).
                        slot = Some(MailStream::Tls(Box::new(tls_stream)));
                        state.tls_established(profile, &limits);
                        pending.clear();
                        continue;
                    }
                    Ok(Err(error)) => {
                        tracing::info!(mail_server_id = %profile.id, peer = %peer, error = %error, "mail STARTTLS handshake failed");
                        break;
                    }
                    Err(_) => {
                        tracing::info!(mail_server_id = %profile.id, peer = %peer, "mail STARTTLS handshake timed out");
                        break;
                    }
                }
            }
            DialogOutcome::Authenticate(attempt) => {
                let stream = slot.take().expect("mail stream present");
                authenticate_and_relay(
                    profile,
                    &mut state,
                    stream,
                    &attempt,
                    proxy_header.as_ref(),
                    &limits,
                )
                .await;
                return Ok(());
            }
        }
    }
    if let Some(mut stream) = slot {
        stream.shutdown().await;
    }
    Ok(())
}

/// Read and dispatch client commands until the session closes, authenticates
/// or starts STARTTLS.
async fn run_dialog(
    profile: &MailConnectionProfile,
    state: &mut MailSessionState,
    stream: &mut MailStream,
    pending: &mut Vec<u8>,
    limits: &Limits,
) -> DialogOutcome {
    let mut reader = LineReader::new(state.session.protocol().as_str(), limits);
    let mut chunk = [0_u8; MAIL_READ_CHUNK_BYTES];
    loop {
        if pending.is_empty() {
            match timeout(profile.timeout, stream.read(&mut chunk)).await {
                Ok(Ok(0)) => return DialogOutcome::Close,
                Ok(Ok(read)) => pending.extend_from_slice(&chunk[..read]),
                Ok(Err(error)) => {
                    tracing::debug!(mail_server_id = %profile.id, peer = %state.peer, error = %error, "mail connection read failed");
                    return DialogOutcome::Close;
                }
                Err(_) => {
                    tracing::debug!(mail_server_id = %profile.id, peer = %state.peer, "mail session idle timeout");
                    return DialogOutcome::Close;
                }
            }
        }
        let lines = match reader.feed(pending) {
            Ok(lines) => {
                pending.clear();
                lines
            }
            Err(error) => {
                tracing::warn!(
                    mail_server_id = %profile.id,
                    peer = %state.peer,
                    error = %error,
                    "mail client command framing rejected; closing connection"
                );
                return DialogOutcome::Close;
            }
        };
        for line in lines {
            state.commands = state.commands.saturating_add(1);
            if state.commands > MAX_SESSION_COMMANDS {
                tracing::debug!(
                    mail_server_id = %profile.id,
                    peer = %state.peer,
                    "mail session command budget exhausted"
                );
                return DialogOutcome::Close;
            }
            let step = state.on_command(&line.text);
            let reply = state.take_reply();
            if !reply.is_empty() && write_mail(stream, &reply, profile.timeout).await.is_err() {
                return DialogOutcome::Close;
            }
            match step {
                DialogStep::Continue => {}
                DialogStep::Close => return DialogOutcome::Close,
                DialogStep::StartTls => return DialogOutcome::StartTls,
                DialogStep::Authenticate(attempt) => return DialogOutcome::Authenticate(attempt),
            }
        }
    }
}

/// Bounded write with the session timeout.
async fn write_mail<S>(stream: &mut S, bytes: &[u8], deadline: Duration) -> Result<(), ()>
where
    S: AsyncWrite + Unpin,
{
    if bytes.is_empty() {
        return Ok(());
    }
    match timeout(deadline, stream.write_all(bytes)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) => Err(()),
    }
}

/// Session limits derived from the configured client buffer.
fn session_limits(max_line_bytes: usize) -> Limits {
    let mut limits = Limits::defaults();
    limits.max_line_bytes = max_line_bytes.max(512);
    limits.sanitized()
}

/// Read and parse one PROXY protocol v1 header; returns the parsed header plus
/// any bytes already received after it (client data that arrived together with
/// the header).
async fn read_proxy_v1(
    stream: &mut TcpStream,
    limits: &Limits,
    deadline: Duration,
) -> Result<(ProxyProtocolV1, Vec<u8>), &'static str> {
    let mut buffer = Vec::with_capacity(128);
    let mut chunk = [0_u8; 128];
    loop {
        if let Some(terminator) = buffer.windows(2).position(|window| window == b"\r\n") {
            let consumed = terminator + 2;
            let (header, _) = ProxyProtocolV1::parse(&buffer, limits)
                .map_err(|_| "PROXY protocol header is malformed")?;
            return Ok((header, buffer[consumed..].to_vec()));
        }
        if buffer.len() > limits.max_relay_header_bytes {
            return Err("PROXY protocol header exceeds the configured bound");
        }
        match timeout(deadline, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => return Err("connection closed before the PROXY protocol header"),
            Ok(Ok(read)) => buffer.extend_from_slice(&chunk[..read]),
            Ok(Err(_)) => return Err("PROXY protocol header read failed"),
            Err(_) => return Err("PROXY protocol header read timed out"),
        }
    }
}

/// One step of the protocol dialog.
enum DialogStep {
    /// Reply written, session continues.
    Continue,
    /// Reply written, session closes.
    Close,
    /// Reply written, the caller must upgrade the connection with STARTTLS.
    StartTls,
    /// The client completed an authorization exchange; the caller runs the
    /// auth_http lookup and the backend relay.
    Authenticate(AuthAttempt),
}

/// LOGIN/PLAIN continuation state.
#[derive(Default, Clone, Copy)]
enum PendingAuth {
    #[default]
    None,
    LoginUser,
    LoginPassword,
    PlainContinuation,
}

/// Per-session dialog state shared by POP3/IMAP/SMTP.
struct MailSessionState {
    id: String,
    peer: SocketAddr,
    client_ip: std::net::IpAddr,
    session: FrontendSession,
    mechanisms: Vec<MailAuthMechanism>,
    greeting_name: String,
    limits: Limits,
    commands: usize,
    errors: u32,
    max_errors: u32,
    reply: Vec<u8>,
    pop3_user: Option<String>,
    imap_tag: Option<String>,
    smtp_helo: Option<String>,
    smtp_esmtp: bool,
    pending: PendingAuth,
    login_user: Option<String>,
    starttls_available: bool,
    tls_active: bool,
    starttls_only: bool,
    /// Local address of the client-facing connection (outbound PROXY header).
    proxy_local_addr: Option<SocketAddr>,
}

impl MailSessionState {
    fn new(
        profile: &MailConnectionProfile,
        peer: SocketAddr,
        client_ip: std::net::IpAddr,
        implicit_tls: bool,
        limits: &Limits,
    ) -> Self {
        let tls_active = implicit_tls;
        let starttls_only = !implicit_tls
            && profile
                .tls
                .as_ref()
                .is_some_and(|tls| matches!(tls.starttls, MailStarttlsMode::Only));
        let starttls_available = profile
            .tls
            .as_ref()
            .is_some_and(|tls| !tls_active && !matches!(tls.starttls, MailStarttlsMode::Off));
        Self {
            id: profile.id.clone(),
            peer,
            client_ip,
            session: FrontendSession::new(profile.protocol, implicit_tls),
            mechanisms: profile.mechanisms.clone(),
            greeting_name: profile.greeting_name.clone(),
            limits: *limits,
            commands: 0,
            errors: 0,
            max_errors: profile.max_errors,
            reply: Vec::new(),
            pop3_user: None,
            imap_tag: None,
            smtp_helo: None,
            smtp_esmtp: false,
            pending: PendingAuth::None,
            login_user: None,
            starttls_available,
            tls_active,
            starttls_only,
            proxy_local_addr: None,
        }
    }

    /// STARTTLS completed: the dialog restarts with a TLS-active session.
    fn tls_established(&mut self, profile: &MailConnectionProfile, limits: &Limits) {
        self.session = FrontendSession::new(profile.protocol, true);
        self.session
            .apply(FrontendAction::TlsEstablished, None, limits)
            .ok();
        self.session = FrontendSession::new(profile.protocol, true);
        self.tls_active = true;
        self.starttls_available = false;
        self.starttls_only = false;
        self.pop3_user = None;
        self.imap_tag = None;
        self.smtp_helo = None;
        self.smtp_esmtp = false;
        self.pending = PendingAuth::None;
        self.login_user = None;
        self.errors = 0;
        self.commands = 0;
        self.reply.clear();
    }

    fn greeting(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        match self.session.protocol() {
            MailProtocol::Pop3 => out.extend_from_slice(b"+OK POP3 ready\r\n"),
            MailProtocol::Imap => out.extend_from_slice(b"* OK IMAP4 ready\r\n"),
            MailProtocol::Smtp => {
                out.extend_from_slice(
                    format!("220 {} ESMTP ready\r\n", self.greeting_name).as_bytes(),
                );
            }
        }
        out
    }

    fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.reply)
    }

    fn push_reply(&mut self, text: &str) {
        self.reply.extend_from_slice(text.as_bytes());
    }

    fn push_reply_line(&mut self, text: &str) {
        self.reply.extend_from_slice(text.as_bytes());
        self.reply.extend_from_slice(b"\r\n");
    }

    fn mechanism_tokens(&self) -> Vec<&'static str> {
        self.mechanisms
            .iter()
            .map(|mechanism| mechanism.as_token())
            .collect()
    }

    /// Count one protocol error; nginx closes the session once the configured
    /// `max_errors` budget is exhausted.
    fn note_protocol_error(&mut self) -> bool {
        self.errors = self.errors.saturating_add(1);
        self.errors >= self.max_errors
    }

    fn auth_attempt(&mut self, user: String, password: String) -> DialogStep {
        if self.session.state() == FrontendSessionState::Initial {
            if let Err(error) = self
                .session
                .apply(FrontendAction::BeginAuth, None, &self.limits)
            {
                tracing::debug!(
                    mail_server_id = %self.id,
                    peer = %self.peer,
                    error = %error,
                    "mail session rejected an out-of-state auth transition"
                );
                return DialogStep::Close;
            }
        }
        let _ = self
            .session
            .apply(FrontendAction::CompleteAuth, Some(&user), &self.limits);
        self.pending = PendingAuth::None;
        DialogStep::Authenticate(AuthAttempt {
            user,
            password: Some(password),
            method: "plain",
            imap_tag: self.imap_tag.clone(),
        })
    }

    /// nginx's protocol-specific reply for a failed authorization attempt.
    fn auth_failure(&mut self, message: &str, error_code: Option<&str>) -> DialogStep {
        let message = sanitize_reply_text(message);
        match self.session.protocol() {
            MailProtocol::Pop3 => self.push_reply_line(&format!("-ERR {message}")),
            MailProtocol::Imap => {
                let tag = self.imap_tag.clone().unwrap_or_else(|| "*".to_owned());
                let tag = if tag == "*" { "*".to_owned() } else { tag };
                self.push_reply_line(&format!("{tag} NO {message}"));
            }
            MailProtocol::Smtp => {
                let code = error_code
                    .map(sanitize_reply_text)
                    .unwrap_or_else(|| "535 5.7.0".to_owned());
                self.push_reply_line(&format!("{code} {message}"));
            }
        }
        DialogStep::Close
    }

    /// nginx's protocol-specific internal-server-error reply.
    fn internal_server_error(&mut self) -> DialogStep {
        match self.session.protocol() {
            MailProtocol::Pop3 => self.push_reply_line("-ERR internal server error"),
            MailProtocol::Imap => self.push_reply_line("* BAD internal server error"),
            MailProtocol::Smtp => self.push_reply_line("451 4.3.2 Internal server error"),
        }
        DialogStep::Close
    }

    fn on_command(&mut self, line: &str) -> DialogStep {
        if !matches!(self.pending, PendingAuth::None) {
            return self.on_auth_continuation(line);
        }
        match self.session.protocol() {
            MailProtocol::Pop3 => self.on_pop3(line),
            MailProtocol::Imap => self.on_imap(line),
            MailProtocol::Smtp => self.on_smtp(line),
        }
    }

    fn on_auth_continuation(&mut self, line: &str) -> DialogStep {
        let pending = std::mem::take(&mut self.pending);
        match pending {
            PendingAuth::LoginUser => {
                match decode_login_response(line.trim(), &self.limits) {
                    Ok(user) => self.login_user = Some(user),
                    Err(_) => {
                        self.protocol_bad_command();
                        return DialogStep::Continue;
                    }
                }
                self.pending = PendingAuth::LoginPassword;
                match self.session.protocol() {
                    MailProtocol::Pop3 | MailProtocol::Imap => {
                        self.push_reply_line(&format!("+ {LOGIN_PASSWORD_PROMPT}"))
                    }
                    MailProtocol::Smtp => {
                        self.push_reply_line(&format!("334 {LOGIN_PASSWORD_PROMPT}"))
                    }
                }
                self.session
                    .apply(FrontendAction::BeginAuth, None, &self.limits)
                    .ok();
                DialogStep::Continue
            }
            PendingAuth::LoginPassword => {
                let password = match decode_login_response(line.trim(), &self.limits) {
                    Ok(password) => password,
                    Err(_) => {
                        self.protocol_bad_command();
                        return DialogStep::Continue;
                    }
                };
                let Some(user) = self.login_user.take() else {
                    self.protocol_bad_command();
                    return DialogStep::Continue;
                };
                self.auth_attempt(user, password)
            }
            PendingAuth::PlainContinuation => {
                let decoded = line.trim();
                let decoded = if decoded == "=" { "" } else { decoded };
                match decode_plain(decoded, &self.limits) {
                    Ok(credentials) => {
                        self.auth_attempt(credentials.authentication_id, credentials.password)
                    }
                    Err(_) => {
                        self.protocol_bad_command();
                        DialogStep::Continue
                    }
                }
            }
            PendingAuth::None => {
                self.unknown_command();
                DialogStep::Continue
            }
        }
    }

    fn protocol_bad_command(&mut self) -> DialogStep {
        let exhausted = self.note_protocol_error();
        match self.session.protocol() {
            MailProtocol::Pop3 => self.push_reply_line("-ERR invalid command"),
            MailProtocol::Imap => match self.imap_tag.clone() {
                Some(tag) => self.push_reply_line(&format!("{tag} BAD invalid command")),
                None => self.push_reply_line("* BAD invalid command"),
            },
            MailProtocol::Smtp => self.push_reply_line("501 5.5.2 Cannot decode response"),
        }
        if exhausted {
            DialogStep::Close
        } else {
            DialogStep::Continue
        }
    }

    fn unknown_command(&mut self) {
        let exhausted = self.note_protocol_error();
        match self.session.protocol() {
            MailProtocol::Pop3 => self.push_reply_line("-ERR invalid command"),
            MailProtocol::Imap => match self.imap_tag.clone() {
                Some(tag) => self.push_reply_line(&format!("{tag} BAD invalid command")),
                None => self.push_reply_line("* BAD invalid command"),
            },
            MailProtocol::Smtp => self.push_reply_line("500 5.5.1 Invalid command"),
        }
        if exhausted {
            // The caller writes the reply and then closes; `Close` conveys that.
        }
    }

    // ---- POP3 ---------------------------------------------------------

    fn on_pop3(&mut self, line: &str) -> DialogStep {
        let (verb, argument) = split_mail_command(line);
        match verb.as_str() {
            "CAPA" => {
                let capabilities = self.pop3_capabilities();
                self.reply.extend_from_slice(&capabilities);
                DialogStep::Continue
            }
            "NOOP" => {
                self.push_reply_line("+OK");
                DialogStep::Continue
            }
            "QUIT" => {
                self.push_reply_line("+OK");
                DialogStep::Close
            }
            "USER" => {
                let Some(user) = non_empty(argument) else {
                    self.push_reply_line("-ERR invalid command");
                    return DialogStep::Continue;
                };
                if user.len() > self.limits.max_credential_bytes {
                    self.push_reply_line("-ERR invalid command");
                    return DialogStep::Continue;
                }
                self.pop3_user = Some(user.to_owned());
                self.push_reply_line("+OK");
                DialogStep::Continue
            }
            "PASS" => {
                let Some(password) = non_empty(argument) else {
                    self.push_reply_line("-ERR invalid command");
                    return DialogStep::Continue;
                };
                let Some(user) = self.pop3_user.clone() else {
                    self.push_reply_line("-ERR invalid command");
                    return DialogStep::Continue;
                };
                self.auth_attempt(user, password.to_owned())
            }
            "APOP" => {
                // APOP needs a digest-capable auth backend; the bounded
                // vocabulary advertises PLAIN/LOGIN only.
                if self.note_protocol_error() {
                    self.push_reply_line("-ERR invalid command");
                    return DialogStep::Close;
                }
                self.push_reply_line("-ERR invalid command");
                DialogStep::Continue
            }
            "AUTH" => self.on_pop3_auth(argument),
            "STLS" => {
                if self.tls_active || !self.starttls_available {
                    self.push_reply_line("-ERR invalid command");
                    return DialogStep::Continue;
                }
                self.push_reply_line("+OK");
                DialogStep::StartTls
            }
            _ => {
                let exhausted = self.note_protocol_error();
                self.push_reply_line("-ERR invalid command");
                if exhausted {
                    DialogStep::Close
                } else {
                    DialogStep::Continue
                }
            }
        }
    }

    fn pop3_capabilities(&self) -> Vec<u8> {
        let mut out = String::from("+OK Capability list follows\r\n");
        if self.starttls_only {
            out.push_str("TOP\r\nUIDL\r\nSTLS\r\n.\r\n");
            return out.into_bytes();
        }
        out.push_str("TOP\r\nUSER\r\nUIDL\r\nSASL");
        for mechanism in self.mechanism_tokens() {
            out.push(' ');
            out.push_str(mechanism);
        }
        out.push_str("\r\n");
        if self.starttls_available {
            out.push_str("STLS\r\n");
        }
        out.push_str(".\r\n");
        out.into_bytes()
    }

    fn on_pop3_auth(&mut self, argument: Option<&str>) -> DialogStep {
        if self.starttls_only {
            return self.pop3_invalid();
        }
        let (mechanism, initial) = split_mechanism(argument);
        match mechanism.to_ascii_uppercase().as_str() {
            "PLAIN" => {
                let Some(initial) = initial.filter(|value| !value.is_empty()) else {
                    self.pending = PendingAuth::PlainContinuation;
                    self.push_reply_line("+ ");
                    return DialogStep::Continue;
                };
                let decoded = if initial == "=" { "" } else { initial };
                match decode_plain(decoded, &self.limits) {
                    Ok(credentials) => {
                        self.auth_attempt(credentials.authentication_id, credentials.password)
                    }
                    Err(_) => self.pop3_invalid(),
                }
            }
            "LOGIN" => {
                self.pending = PendingAuth::LoginUser;
                self.push_reply_line(&format!("+ {LOGIN_USER_PROMPT}"));
                DialogStep::Continue
            }
            _ => self.pop3_invalid(),
        }
    }

    fn pop3_invalid(&mut self) -> DialogStep {
        let exhausted = self.note_protocol_error();
        self.push_reply_line("-ERR invalid command");
        if exhausted {
            DialogStep::Close
        } else {
            DialogStep::Continue
        }
    }

    // ---- IMAP ---------------------------------------------------------

    fn on_imap(&mut self, line: &str) -> DialogStep {
        if self.starttls_only && !self.tls_active && !is_imap_starttls_command(line) {
            let Some((tag, _)) = split_imap_tag(line) else {
                self.imap_tag = None;
                self.push_reply_line("* BAD invalid command");
                return DialogStep::Continue;
            };
            self.imap_tag = Some(tag.clone());
            self.push_reply_line(&format!("{tag} BAD LOGINDISABLED"));
            return DialogStep::Continue;
        }
        let Some((tag, rest)) = split_imap_tag(line) else {
            self.imap_tag = None;
            self.push_reply_line("* BAD invalid command");
            return DialogStep::Continue;
        };
        self.imap_tag = Some(tag.clone());
        let (verb, argument) = split_mail_command(rest);
        match verb.as_str() {
            "CAPABILITY" => {
                let mut text = String::from("* CAPABILITY IMAP4 IMAP4rev1 UIDPLUS");
                if self.starttls_only {
                    text.push_str(" STARTTLS LOGINDISABLED");
                } else {
                    for mechanism in self.mechanism_tokens() {
                        text.push_str(" AUTH=");
                        text.push_str(mechanism);
                    }
                    if self.starttls_available {
                        text.push_str(" STARTTLS");
                    }
                }
                self.push_reply(&format!("{text}\r\n{tag} OK completed\r\n"));
                DialogStep::Continue
            }
            "NOOP" => {
                self.push_reply_line(&format!("{tag} OK completed"));
                DialogStep::Continue
            }
            "LOGOUT" => {
                self.push_reply(&format!("* BYE\r\n{tag} OK completed\r\n"));
                DialogStep::Close
            }
            "LOGIN" => {
                let Some(credentials) = argument else {
                    self.push_reply_line(&format!("{tag} BAD invalid command"));
                    return DialogStep::Continue;
                };
                let parts = credentials.split_whitespace().collect::<Vec<_>>();
                if parts.len() != 2
                    || parts.iter().any(|part| {
                        part.len() > self.limits.max_credential_bytes || part.is_empty()
                    })
                {
                    self.push_reply_line(&format!("{tag} BAD invalid command"));
                    return DialogStep::Continue;
                }
                self.auth_attempt(parts[0].to_owned(), parts[1].to_owned())
            }
            "AUTHENTICATE" => self.on_imap_authenticate(argument),
            "STARTTLS" => {
                if self.tls_active || !self.starttls_available {
                    self.push_reply_line(&format!("{tag} BAD invalid command"));
                    return DialogStep::Continue;
                }
                self.push_reply_line(&format!("{tag} OK completed"));
                DialogStep::StartTls
            }
            _ => {
                let exhausted = self.note_protocol_error();
                self.push_reply_line(&format!("{tag} BAD invalid command"));
                if exhausted {
                    DialogStep::Close
                } else {
                    DialogStep::Continue
                }
            }
        }
    }

    fn on_imap_authenticate(&mut self, argument: Option<&str>) -> DialogStep {
        let tag = self.imap_tag.clone().unwrap_or_else(|| "*".to_owned());
        let (mechanism, initial) = split_mechanism(argument);
        match mechanism.to_ascii_uppercase().as_str() {
            "PLAIN" => {
                let Some(initial) = initial.filter(|value| !value.is_empty()) else {
                    self.pending = PendingAuth::PlainContinuation;
                    self.push_reply_line("+ ");
                    return DialogStep::Continue;
                };
                let decoded = if initial == "=" { "" } else { initial };
                match decode_plain(decoded, &self.limits) {
                    Ok(credentials) => {
                        self.auth_attempt(credentials.authentication_id, credentials.password)
                    }
                    Err(_) => {
                        self.push_reply_line(&format!("{tag} BAD invalid command"));
                        DialogStep::Continue
                    }
                }
            }
            "LOGIN" => {
                self.pending = PendingAuth::LoginUser;
                self.push_reply_line(&format!("+ {LOGIN_USER_PROMPT}"));
                DialogStep::Continue
            }
            _ => {
                self.push_reply_line(&format!("{tag} BAD invalid command"));
                DialogStep::Continue
            }
        }
    }

    // ---- SMTP ---------------------------------------------------------

    fn on_smtp(&mut self, line: &str) -> DialogStep {
        let (verb, argument) = split_mail_command(line);
        match verb.to_ascii_uppercase().as_str() {
            "EHLO" => {
                let Some(domain) = non_empty(argument) else {
                    self.push_reply_line("501 5.5.4 Invalid arguments");
                    return DialogStep::Continue;
                };
                if domain.len() > self.limits.max_line_bytes {
                    self.push_reply_line("501 5.5.4 Invalid arguments");
                    return DialogStep::Continue;
                }
                self.smtp_helo = Some(domain.to_owned());
                self.smtp_esmtp = true;
                let capabilities = self.smtp_capabilities();
                self.reply.extend_from_slice(&capabilities);
                DialogStep::Continue
            }
            "HELO" => {
                let Some(domain) = non_empty(argument) else {
                    self.push_reply_line("501 5.5.4 Invalid arguments");
                    return DialogStep::Continue;
                };
                if domain.len() > self.limits.max_line_bytes {
                    self.push_reply_line("501 5.5.4 Invalid arguments");
                    return DialogStep::Continue;
                }
                self.smtp_helo = Some(domain.to_owned());
                self.smtp_esmtp = false;
                self.push_reply_line(&format!("250 {}", self.greeting_name));
                DialogStep::Continue
            }
            "NOOP" | "RSET" => {
                self.push_reply_line("250 2.0.0 OK");
                DialogStep::Continue
            }
            "QUIT" => {
                self.push_reply_line("221 2.0.0 Bye");
                DialogStep::Close
            }
            "MAIL" => {
                self.push_reply_line("530 5.7.1 Authentication required");
                DialogStep::Continue
            }
            "RCPT" => {
                self.push_reply_line("503 5.5.1 Bad sequence of commands");
                DialogStep::Continue
            }
            "DATA" => {
                self.push_reply_line("500 5.5.1 Invalid command");
                DialogStep::Continue
            }
            "AUTH" => self.on_smtp_auth(argument),
            "STARTTLS" => {
                if self.tls_active || !self.starttls_available {
                    self.push_reply_line("500 5.5.1 Invalid command");
                    return DialogStep::Continue;
                }
                self.push_reply_line("220 2.0.0 Start TLS");
                DialogStep::StartTls
            }
            _ => {
                let exhausted = self.note_protocol_error();
                self.push_reply_line("500 5.5.1 Invalid command");
                if exhausted {
                    DialogStep::Close
                } else {
                    DialogStep::Continue
                }
            }
        }
    }

    fn smtp_capabilities(&self) -> Vec<u8> {
        let mut out = String::new();
        if self.starttls_only {
            out.push_str(&format!("250-{}\r\n", self.greeting_name));
            out.push_str("250 STARTTLS\r\n");
            return out.into_bytes();
        }
        out.push_str(&format!("250-{}\r\n", self.greeting_name));
        if self.starttls_available {
            out.push_str("250-AUTH");
        } else {
            out.push_str("250 AUTH");
        }
        for mechanism in self.mechanism_tokens() {
            out.push(' ');
            out.push_str(mechanism);
        }
        out.push_str("\r\n");
        if self.starttls_available {
            out.push_str("250 STARTTLS\r\n");
        }
        out.into_bytes()
    }

    fn on_smtp_auth(&mut self, argument: Option<&str>) -> DialogStep {
        let (mechanism, initial) = split_mechanism(argument);
        match mechanism.to_ascii_uppercase().as_str() {
            "PLAIN" => {
                let Some(initial) = initial.filter(|value| !value.is_empty()) else {
                    self.pending = PendingAuth::PlainContinuation;
                    self.push_reply_line("334 ");
                    return DialogStep::Continue;
                };
                let decoded = if initial == "=" { "" } else { initial };
                match decode_plain(decoded, &self.limits) {
                    Ok(credentials) => {
                        self.auth_attempt(credentials.authentication_id, credentials.password)
                    }
                    Err(_) => {
                        self.push_reply_line("501 5.5.2 Cannot decode response");
                        DialogStep::Continue
                    }
                }
            }
            "LOGIN" => {
                self.pending = PendingAuth::LoginUser;
                self.push_reply_line(&format!("334 {LOGIN_USER_PROMPT}"));
                DialogStep::Continue
            }
            _ => {
                self.push_reply_line("500 5.5.1 Invalid command");
                DialogStep::Continue
            }
        }
    }
}

/// IMAP line that selects the STARTTLS command regardless of LOGINDISABLED.
fn is_imap_starttls_command(line: &str) -> bool {
    split_imap_tag(line)
        .map(|(_, rest)| {
            rest.split_whitespace()
                .next()
                .is_some_and(|verb| verb.eq_ignore_ascii_case("STARTTLS"))
        })
        .unwrap_or(false)
}

/// Strip CR/LF/control characters from an auth-service supplied reply text so a
/// malicious auth backend cannot inject extra protocol lines.
fn sanitize_reply_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect()
}

/// Split one mail command into its verb (uppercased) and the remaining
/// arguments (trimmed), preserving empty arguments as `None`.
fn split_mail_command(line: &str) -> (String, Option<&str>) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }
    match trimmed.split_once(char::is_whitespace) {
        Some((verb, rest)) => {
            let rest = rest.trim();
            (
                verb.to_ascii_uppercase(),
                (!rest.is_empty()).then_some(rest),
            )
        }
        None => (trimmed.to_ascii_uppercase(), None),
    }
}

/// Split a SASL `MECHANISM [initial-response]` argument pair.
fn split_mechanism(argument: Option<&str>) -> (&str, Option<&str>) {
    match argument {
        None => ("", None),
        Some(argument) => match argument.split_once(char::is_whitespace) {
            Some((mechanism, initial)) => (mechanism, Some(initial.trim())),
            None => (argument, None),
        },
    }
}

/// Split an IMAP tagged command line into `(<tag>, <command and arguments>)`.
///
/// nginx accepts a tag of 1..=64 non-space, non-control bytes followed by a
/// space and a command; anything else is an untagged protocol error.
fn split_imap_tag(line: &str) -> Option<(String, &str)> {
    let line = line.trim_end();
    let (tag, rest) = line.split_once(' ')?;
    if tag.is_empty()
        || tag.len() > 64
        || tag
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
        || rest.trim().is_empty()
    {
        return None;
    }
    Some((tag.to_owned(), rest.trim_start()))
}

/// An argument that is present and not empty.
fn non_empty(argument: Option<&str>) -> Option<&str> {
    argument.filter(|value| !value.is_empty())
}

// ---------------------------------------------------------------------------
// auth_http exchange
// ---------------------------------------------------------------------------

/// One parsed `auth_http` response.
#[derive(Debug, Default)]
struct AuthHttpResponse {
    status: String,
    server: Option<String>,
    port: Option<u16>,
    login: Option<String>,
    password: Option<String>,
    wait_seconds: Option<u64>,
    error_code: Option<String>,
}

/// Frontend TLS facts nginx forwards to the auth service.
struct AuthHttpTlsFacts<'a> {
    protocol: Option<&'a str>,
    cipher: Option<&'a str>,
}

/// Facts the auth_http request is built from.
struct AuthHttpRequestFacts<'a> {
    attempt: &'a AuthAttempt,
    attempt_number: u32,
    client_ip: std::net::IpAddr,
    proxy_header: Option<&'a ProxyProtocolV1>,
    tls: AuthHttpTlsFacts<'a>,
}

/// Percent-escape a credential the way nginx `NGX_ESCAPE_MAIL_AUTH` does for
/// the bytes that matter to the line-oriented auth protocol.
fn escape_mail_auth(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b' ' | b'%' | b'\r' | b'\n' | b'?' => {
                escaped.push('%');
                escaped.push_str(&format!("{byte:02X}"));
            }
            other => escaped.push(other as char),
        }
    }
    escaped
}

/// Build the nginx-shaped `auth_http` request (GET/HTTP/1.0).
fn build_auth_http_request(
    auth: &AuthHttpProfile,
    server: &MailConnectionProfile,
    facts: &AuthHttpRequestFacts<'_>,
) -> Vec<u8> {
    let mut request = String::with_capacity(512);
    request.push_str("GET ");
    request.push_str(&auth.path);
    request.push_str(" HTTP/1.0\r\n");
    request.push_str("Host: ");
    request.push_str(&auth.host_header);
    request.push_str("\r\n");
    request.push_str("Auth-Method: ");
    request.push_str(facts.attempt.method);
    request.push_str("\r\n");
    request.push_str("Auth-User: ");
    request.push_str(&escape_mail_auth(&facts.attempt.user));
    request.push_str("\r\n");
    if let Some(password) = facts.attempt.password.as_deref() {
        request.push_str("Auth-Pass: ");
        request.push_str(&escape_mail_auth(password));
        request.push_str("\r\n");
    }
    request.push_str("Auth-Protocol: ");
    request.push_str(server.protocol.as_str());
    request.push_str("\r\n");
    request.push_str(&format!("Auth-Login-Attempt: {}\r\n", facts.attempt_number));
    request.push_str("Client-IP: ");
    request.push_str(&facts.client_ip.to_string());
    request.push_str("\r\n");
    if matches!(server.protocol, MailProtocol::Smtp) {
        // nginx sets `s->host` to `[UNAVAILABLE]` for SMTP when no mail
        // resolver is configured (the bounded runtime never does PTR/FCrDNS).
        request.push_str("Client-Host: [UNAVAILABLE]\r\n");
    }
    if let Some(header) = facts.proxy_header {
        if let Some(source) = header.source() {
            request.push_str("Proxy-Protocol-Addr: ");
            request.push_str(&source.ip().to_string());
            request.push_str("\r\n");
            request.push_str(&format!("Proxy-Protocol-Port: {}\r\n", source.port()));
        }
        if let Some(destination) = header.destination() {
            request.push_str("Proxy-Protocol-Server-Addr: ");
            request.push_str(&destination.ip().to_string());
            request.push_str("\r\n");
            request.push_str(&format!(
                "Proxy-Protocol-Server-Port: {}\r\n",
                destination.port()
            ));
        }
    }
    if server.tls.as_ref().is_some_and(|tls| tls.implicit) || facts.tls.protocol.is_some() {
        request.push_str("Auth-SSL: on\r\n");
        if let Some(protocol) = facts.tls.protocol {
            request.push_str("Auth-SSL-Protocol: ");
            request.push_str(protocol);
            request.push_str("\r\n");
        }
        if let Some(cipher) = facts.tls.cipher {
            request.push_str("Auth-SSL-Cipher: ");
            request.push_str(cipher);
            request.push_str("\r\n");
        }
    }
    for header in &auth.headers {
        request.push_str(header);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    if auth.pass_client_cert {
        // Client-certificate pass-through requires a client-auth TLS policy
        // that the mail plane does not model; it fails closed at validation.
    }
    request.into_bytes()
}

/// Run one bounded auth_http request/response exchange.
async fn exchange_auth_http(
    auth: &AuthHttpProfile,
    request: &[u8],
) -> Result<AuthHttpResponse, ()> {
    let mut stream = timeout(auth.timeout, TcpStream::connect(auth.address))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    timeout(auth.timeout, stream.write_all(request))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            buffer.truncate(position + 4);
            break;
        }
        if buffer.len() > MAX_AUTH_HTTP_RESPONSE_BYTES {
            return Err(());
        }
        let read = timeout(auth.timeout, stream.read(&mut chunk))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
        if read == 0 {
            return Err(());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let _ = stream.shutdown().await;
    parse_auth_http_response(&buffer)
}

/// Parse an auth_http response: the status line is ignored, the nginx `Auth-*`
/// headers decide the outcome (last occurrence wins, like nginx).
fn parse_auth_http_response(response: &[u8]) -> Result<AuthHttpResponse, ()> {
    let text = std::str::from_utf8(response).map_err(|_| ())?;
    let status_line = text.split("\r\n").next().unwrap_or_default();
    if !status_line.starts_with("HTTP/") {
        return Err(());
    }
    let mut lines = text.split("\r\n");
    let _status_line = lines.next().ok_or(())?;
    let mut parsed = AuthHttpResponse::default();
    let mut seen_headers = 0usize;
    for line in lines {
        if line.is_empty() {
            break;
        }
        seen_headers = seen_headers.saturating_add(1);
        if seen_headers > MAX_AUTH_HTTP_HEADERS {
            return Err(());
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(());
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "auth-status" => parsed.status = value.to_owned(),
            "auth-server" => parsed.server = Some(value.to_owned()),
            "auth-port" => {
                parsed.port = Some(value.parse::<u16>().map_err(|_| ())?);
            }
            "auth-user" => parsed.login = Some(value.to_owned()),
            "auth-pass" => parsed.password = Some(value.to_owned()),
            "auth-wait" => {
                parsed.wait_seconds = Some(value.parse::<u64>().map_err(|_| ())?);
            }
            "auth-error-code" => parsed.error_code = Some(value.to_owned()),
            _ => {}
        }
    }
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// Backend relay
// ---------------------------------------------------------------------------

/// Buffered backend protocol I/O with a bounded line reader.
struct BackendIo {
    stream: BackendStream,
    pending: Vec<u8>,
    max_line_bytes: usize,
    timeout: Duration,
}

impl BackendIo {
    fn new(stream: BackendStream, max_line_bytes: usize, timeout: Duration) -> Self {
        Self {
            stream,
            pending: Vec::new(),
            max_line_bytes,
            timeout,
        }
    }

    async fn write_line(&mut self, line: &str) -> Result<(), ()> {
        let mut bytes = Vec::with_capacity(line.len() + 2);
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\r\n");
        timeout(self.timeout, self.stream.write_all(&bytes))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())
    }

    async fn read_line(&mut self) -> Result<String, ()> {
        loop {
            if let Some(position) = self.pending.windows(2).position(|window| window == b"\r\n") {
                let line = self.pending.drain(..position + 2).collect::<Vec<_>>();
                let text = String::from_utf8(line[..line.len() - 2].to_vec()).map_err(|_| ())?;
                return Ok(text);
            }
            if self.pending.len() > self.max_line_bytes {
                return Err(());
            }
            let mut chunk = [0_u8; 1024];
            let read = timeout(self.timeout, self.stream.read(&mut chunk))
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?;
            if read == 0 {
                return Err(());
            }
            self.pending.extend_from_slice(&chunk[..read]);
        }
    }

    /// Read one SMTP reply, joining continuation lines of a multiline reply.
    async fn read_smtp_reply(&mut self) -> Result<(u16, Vec<String>), ()> {
        let mut lines = Vec::new();
        for _ in 0..32 {
            let line = self.read_line().await?;
            let code = line
                .get(..3)
                .and_then(|code| code.parse::<u16>().ok())
                .ok_or(())?;
            let continuation = line.as_bytes().get(3) == Some(&b'-');
            lines.push(line);
            if !continuation {
                return Ok((code, lines));
            }
        }
        Err(())
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }

    fn into_stream(self) -> BackendStream {
        self.stream
    }
}

/// Authenticate the client against the auth service, then relay to the backend.
async fn authenticate_and_relay(
    profile: &MailConnectionProfile,
    state: &mut MailSessionState,
    stream: MailStream,
    attempt: &AuthAttempt,
    proxy_header: Option<&ProxyProtocolV1>,
    limits: &Limits,
) -> bool {
    let tls_facts_owned = stream.tls_facts();
    let mut stream = stream;
    let Some(auth) = profile.auth_http.as_ref() else {
        state.internal_server_error();
        write_step_reply(&mut stream, state, profile.timeout).await;
        return false;
    };
    let mut attempt_number = 1u32;
    let mut wait_retries = 0u32;
    let response = loop {
        let facts = AuthHttpRequestFacts {
            attempt,
            attempt_number,
            client_ip: state.client_ip,
            proxy_header,
            tls: AuthHttpTlsFacts {
                protocol: tls_facts_owned
                    .as_ref()
                    .map(|(protocol, _)| protocol.as_str()),
                cipher: tls_facts_owned.as_ref().map(|(_, cipher)| cipher.as_str()),
            },
        };
        let request = build_auth_http_request(auth, profile, &facts);
        let response = match exchange_auth_http(auth, &request).await {
            Ok(response) => response,
            Err(()) => {
                state.internal_server_error();
                write_step_reply(&mut stream, state, profile.timeout).await;
                return false;
            }
        };
        let status = response.status.trim();
        if status.eq_ignore_ascii_case("OK") {
            break response;
        }
        if status.eq_ignore_ascii_case("WAIT") {
            if wait_retries >= MAX_AUTH_HTTP_WAIT_RETRIES {
                state.internal_server_error();
                write_step_reply(&mut stream, state, profile.timeout).await;
                return false;
            }
            wait_retries = wait_retries.saturating_add(1);
            attempt_number = attempt_number.saturating_add(1);
            let wait = response.wait_seconds.unwrap_or(0).min(30);
            sleep(Duration::from_secs(wait)).await;
            continue;
        }
        state.auth_failure(status, response.error_code.as_deref());
        write_step_reply(&mut stream, state, profile.timeout).await;
        return false;
    };
    let Some(server) = response.server.as_deref() else {
        state.internal_server_error();
        write_step_reply(&mut stream, state, profile.timeout).await;
        return false;
    };
    let Some(port) = response.port else {
        state.internal_server_error();
        write_step_reply(&mut stream, state, profile.timeout).await;
        return false;
    };
    let login = response
        .login
        .clone()
        .unwrap_or_else(|| attempt.user.clone());
    let password = response
        .password
        .clone()
        .or_else(|| attempt.password.clone());
    if !matches!(profile.protocol, MailProtocol::Smtp) && password.is_none() {
        state.internal_server_error();
        write_step_reply(&mut stream, state, profile.timeout).await;
        return false;
    }
    let target = match resolve_backend_target(server, port, profile.timeout).await {
        Ok(target) => target,
        Err(()) => {
            state.internal_server_error();
            write_step_reply(&mut stream, state, profile.timeout).await;
            return false;
        }
    };
    // The backend leg: connect (bounded), then optional TLS, optional PROXY
    // header, then the protocol bootstrap.
    let backend_stream = match timeout(profile.timeout, TcpStream::connect(target)).await {
        Ok(Ok(stream)) => stream,
        _ => {
            state.internal_server_error();
            write_step_reply(&mut stream, state, profile.timeout).await;
            return false;
        }
    };
    let _ = backend_stream.set_nodelay(true);
    let backend_stream = if let Some(tls) = profile.proxy.tls.as_ref() {
        let server_name = tls.server_name.clone().unwrap_or_else(|| server.to_owned());
        let name = match rustls::pki_types::ServerName::try_from(server_name.clone()) {
            Ok(name) => name,
            Err(_) => {
                state.internal_server_error();
                write_step_reply(&mut stream, state, profile.timeout).await;
                return false;
            }
        };
        let connector = tokio_rustls::TlsConnector::from(Arc::clone(&tls.config));
        match timeout(
            MAIL_TLS_HANDSHAKE_TIMEOUT,
            connector.connect(name, backend_stream),
        )
        .await
        {
            Ok(Ok(tls_stream)) => BackendStream::Tls(Box::new(tls_stream)),
            _ => {
                state.internal_server_error();
                write_step_reply(&mut stream, state, profile.timeout).await;
                return false;
            }
        }
    } else {
        BackendStream::Plain(backend_stream)
    };
    let mut backend = BackendIo::new(
        backend_stream,
        profile
            .proxy
            .buffer_bytes
            .clamp(512, MAX_BACKEND_LINE_BYTES),
        profile.timeout,
    );
    if profile.proxy.proxy_protocol {
        let line = outbound_proxy_line(state, &target);
        if backend.write_line(&line).await.is_err() {
            state.internal_server_error();
            write_step_reply(&mut stream, state, profile.timeout).await;
            return false;
        }
    }
    let bootstrap = backend_bootstrap(profile, state, attempt, &login, password.as_deref(), limits);
    let bootstrap = match bootstrap {
        Ok(bootstrap) => bootstrap,
        Err(()) => {
            state.internal_server_error();
            write_step_reply(&mut stream, state, profile.timeout).await;
            return false;
        }
    };
    match run_backend_bootstrap(
        profile,
        state,
        &mut backend,
        &bootstrap,
        attempt,
        &login,
        password.as_deref(),
    )
    .await
    {
        Ok(success_reply) => {
            // Forward the client-visible authorization response, then any
            // bytes the backend already sent after it, then relay.
            let mut success = success_reply;
            success.extend_from_slice(&backend.take_pending());
            if write_mail(&mut stream, &success, profile.timeout)
                .await
                .is_err()
            {
                return false;
            }
            relay(stream, backend.into_stream(), profile.proxy.timeout).await;
            true
        }
        Err(passthrough) => {
            // nginx `proxy_pass_error_message on` relays the backend's error
            // text; otherwise the client gets the protocol's internal error.
            if profile.proxy.pass_error_message {
                if let Some(bytes) = passthrough {
                    if write_mail(&mut stream, &bytes, profile.timeout)
                        .await
                        .is_ok()
                    {
                        return false;
                    }
                }
            }
            state.internal_server_error();
            write_step_reply(&mut stream, state, profile.timeout).await;
            false
        }
    }
}

/// Write the reply a `DialogStep` produced (used by the auth path).
async fn write_step_reply(
    stream: &mut MailStream,
    state: &mut MailSessionState,
    deadline: Duration,
) {
    let reply = state.take_reply();
    if !reply.is_empty() {
        let _ = write_mail(stream, &reply, deadline).await;
    }
}

/// Resolve the `Auth-Server` backend target with a bounded DNS lookup.
async fn resolve_backend_target(
    server: &str,
    port: u16,
    timeout_duration: Duration,
) -> Result<SocketAddr, ()> {
    if let Ok(address) = server.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(address, port));
    }
    let mut resolved = timeout(timeout_duration, lookup_host((server, port)))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    resolved.next().ok_or(())
}

/// nginx outbound PROXY v1 line: client source, proxy local destination.
fn outbound_proxy_line(state: &MailSessionState, _target: &SocketAddr) -> String {
    let destination = state.proxy_local_addr.unwrap_or(state.peer);
    format!(
        "PROXY {} {} {} {} {}",
        if state.client_ip.is_ipv4() {
            "TCP4"
        } else {
            "TCP6"
        },
        state.client_ip,
        destination.ip(),
        state.peer.port(),
        destination.port()
    )
}

/// The SMTP/POP3/IMAP bootstrap commands sent to the backend.
struct BackendBootstrapPlan {
    lines: Vec<String>,
}

/// Build the backend bootstrap plan from the resolved configuration.
fn backend_bootstrap(
    profile: &MailConnectionProfile,
    state: &MailSessionState,
    attempt: &AuthAttempt,
    login: &str,
    password: Option<&str>,
    limits: &Limits,
) -> Result<BackendBootstrapPlan, ()> {
    let _ = attempt;
    let mut lines = Vec::new();
    match profile.protocol {
        MailProtocol::Pop3 => {}
        MailProtocol::Imap => {}
        MailProtocol::Smtp => {
            let ehlo = if state.smtp_esmtp || profile.proxy.xclient {
                format!("EHLO {}", profile.greeting_name)
            } else {
                format!("HELO {}", profile.greeting_name)
            };
            lines.push(ehlo);
            if profile.proxy.xclient {
                let mut request = XclientRequest::new();
                request
                    .push(XclientAttribute::Addr(state.client_ip), limits)
                    .map_err(|_| ())?;
                if !login.is_empty() && !profile.proxy.smtp_auth {
                    request
                        .push(XclientAttribute::Login(login.to_owned()), limits)
                        .map_err(|_| ())?;
                }
                // nginx `XCLIENT NAME` carries the resolved client host name;
                // without a mail resolver nginx uses `[UNAVAILABLE]` (verified
                // against nginx 1.29.6), not the HELO argument.
                request
                    .push(XclientAttribute::Name("[UNAVAILABLE]".to_owned()), limits)
                    .map_err(|_| ())?;
                lines.push(request.encode_line(limits).map_err(|_| ())?);
            }
            if let Some(helo) = &state.smtp_helo {
                lines.push(format!(
                    "{} {}",
                    if state.smtp_esmtp { "EHLO" } else { "HELO" },
                    helo
                ));
            }
            if profile.proxy.smtp_auth {
                let Some(password) = password else {
                    return Err(());
                };
                lines.push(format!(
                    "AUTH PLAIN {}",
                    sdkwork_webserver_mail_protocol::auth::encode_plain(login, password, limits)
                        .map_err(|_| ())?
                ));
            }
        }
    }
    Ok(BackendBootstrapPlan { lines })
}

/// Run the backend greeting + bootstrap sequence, returning the client-visible
/// authorization success reply on success.
async fn run_backend_bootstrap(
    profile: &MailConnectionProfile,
    _state: &MailSessionState,
    backend: &mut BackendIo,
    plan: &BackendBootstrapPlan,
    attempt: &AuthAttempt,
    login: &str,
    password: Option<&str>,
) -> Result<Vec<u8>, Option<Vec<u8>>> {
    match profile.protocol {
        MailProtocol::Pop3 => {
            let greeting = match backend.read_line().await {
                Ok(greeting) => greeting,
                Err(()) => return Err(None),
            };
            if !greeting.starts_with("+OK") {
                return Err(Some(reply_line(&greeting)));
            }
            if backend.write_line(&format!("USER {login}")).await.is_err() {
                return Err(None);
            }
            let user_reply = match backend.read_line().await {
                Ok(reply) => reply,
                Err(()) => return Err(None),
            };
            if !user_reply.starts_with("+OK") {
                return Err(Some(reply_line(&user_reply)));
            }
            let Some(password) = password else {
                return Err(None);
            };
            if backend
                .write_line(&format!("PASS {password}"))
                .await
                .is_err()
            {
                return Err(None);
            }
            let pass_reply = match backend.read_line().await {
                Ok(reply) => reply,
                Err(()) => return Err(None),
            };
            if !pass_reply.starts_with("+OK") {
                return Err(Some(reply_line(&pass_reply)));
            }
            Ok(reply_line(&pass_reply))
        }
        MailProtocol::Imap => {
            let greeting = match backend.read_line().await {
                Ok(greeting) => greeting,
                Err(()) => return Err(None),
            };
            if !(greeting.starts_with("* OK") || greeting.starts_with("* PREAUTH")) {
                return Err(Some(reply_line(&greeting)));
            }
            let tag = attempt.imap_tag.clone().unwrap_or_else(|| "a1".to_owned());
            if backend
                .write_line(&format!("{tag} LOGIN {{{}}}", login.len()))
                .await
                .is_err()
            {
                return Err(None);
            }
            let login_continuation = match backend.read_line().await {
                Ok(reply) => reply,
                Err(()) => return Err(None),
            };
            if !login_continuation.starts_with('+') {
                return Err(Some(reply_line(&login_continuation)));
            }
            let Some(password) = password else {
                return Err(None);
            };
            if backend
                .write_line(&format!("{login} {{{}}}", password.len()))
                .await
                .is_err()
            {
                return Err(None);
            }
            let password_continuation = match backend.read_line().await {
                Ok(reply) => reply,
                Err(()) => return Err(None),
            };
            if !password_continuation.starts_with('+') {
                return Err(Some(reply_line(&password_continuation)));
            }
            if backend.write_line(password).await.is_err() {
                return Err(None);
            }
            let completion = match backend.read_line().await {
                Ok(reply) => reply,
                Err(()) => return Err(None),
            };
            if !completion.starts_with(&tag) || !completion.contains("OK") {
                return Err(Some(reply_line(&completion)));
            }
            Ok(reply_line(&completion))
        }
        MailProtocol::Smtp => {
            let (code, _) = match backend.read_smtp_reply().await {
                Ok(reply) => reply,
                Err(()) => return Err(None),
            };
            if code != 220 {
                return Err(None);
            }
            for line in &plan.lines {
                if backend.write_line(line).await.is_err() {
                    return Err(None);
                }
                let (code, lines) = match backend.read_smtp_reply().await {
                    Ok(reply) => reply,
                    Err(()) => return Err(None),
                };
                let expected = if line.starts_with("XCLIENT") {
                    code == 250 || code == 220
                } else if line.starts_with("AUTH") {
                    code == 235
                } else {
                    code == 250
                };
                if !expected {
                    return Err(lines.last().map(|line| reply_line(line)));
                }
            }
            // nginx synthesizes the client's SMTP auth success reply; the
            // backend auth exchange itself is not forwarded.
            Ok(b"235 2.0.0 OK\r\n".to_vec())
        }
    }
}

/// Render one backend protocol line with its CRLF framing.
fn reply_line(line: &str) -> Vec<u8> {
    let mut bytes = line.as_bytes().to_vec();
    bytes.extend_from_slice(b"\r\n");
    bytes
}

/// Bidirectional bounded relay with half-close handling.
async fn relay(client: MailStream, backend: BackendStream, idle: Duration) {
    let (mut client_read, mut client_write) = tokio::io::split(client);
    let (mut backend_read, mut backend_write) = tokio::io::split(backend);
    let mut client_open = true;
    let mut backend_open = true;
    let mut client_buffer = [0_u8; 16 * 1024];
    let mut backend_buffer = [0_u8; 16 * 1024];
    loop {
        if !client_open && !backend_open {
            return;
        }
        let activity = timeout(idle, async {
            tokio::select! {
                read = client_read.read(&mut client_buffer), if client_open => {
                    Some(("client", read))
                }
                read = backend_read.read(&mut backend_buffer), if backend_open => {
                    Some(("backend", read))
                }
            }
        })
        .await;
        let Ok(Some((side, read))) = activity else {
            return;
        };
        match read {
            Ok(0) => {
                if side == "client" {
                    client_open = false;
                    let _ = backend_write.shutdown().await;
                } else {
                    backend_open = false;
                    let _ = client_write.shutdown().await;
                }
            }
            Ok(read) => {
                let written = if side == "client" {
                    backend_write.write_all(&client_buffer[..read]).await
                } else {
                    client_write.write_all(&backend_buffer[..read]).await
                };
                if written.is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(protocol: MailProtocol) -> MailConnectionProfile {
        MailConnectionProfile {
            id: "mail-test".to_owned(),
            protocol,
            greeting_name: "mail.example.com".to_owned(),
            timeout: Duration::from_secs(5),
            so_keepalive: false,
            inbound_proxy_protocol: false,
            max_line_bytes: 4096,
            greeting_delay: Duration::ZERO,
            mechanisms: vec![MailAuthMechanism::Plain, MailAuthMechanism::Login],
            max_errors: 5,
            auth_http: None,
            tls: None,
            proxy: MailProxyProfile {
                xclient: true,
                smtp_auth: false,
                proxy_protocol: false,
                pass_error_message: false,
                buffer_bytes: 4096,
                timeout: Duration::from_secs(5),
                tls: None,
            },
        }
    }

    fn new_state(protocol: MailProtocol) -> MailSessionState {
        let profile = profile(protocol);
        let limits = session_limits(profile.max_line_bytes);
        MailSessionState::new(
            &profile,
            "127.0.0.1:12345".parse().expect("peer"),
            "127.0.0.1".parse().expect("client"),
            false,
            &limits,
        )
    }

    fn command(state: &mut MailSessionState, line: &str) -> (String, bool, Option<AuthAttempt>) {
        let step = state.on_command(line);
        let reply = String::from_utf8(state.take_reply()).expect("utf-8 reply");
        match step {
            DialogStep::Continue => (reply, false, None),
            DialogStep::Close => (reply, true, None),
            DialogStep::StartTls => (reply, false, None),
            DialogStep::Authenticate(attempt) => (reply, false, Some(attempt)),
        }
    }

    #[test]
    fn pop3_greeting_and_dialog_capture_credentials() {
        let mut state = new_state(MailProtocol::Pop3);
        assert_eq!(state.greeting(), b"+OK POP3 ready\r\n");
        let (reply, close, _) = command(&mut state, "CAPA");
        assert_eq!(
            reply,
            "+OK Capability list follows\r\nTOP\r\nUSER\r\nUIDL\r\nSASL PLAIN LOGIN\r\n.\r\n"
        );
        assert!(!close);
        let (reply, _, _) = command(&mut state, "NOOP");
        assert_eq!(reply, "+OK\r\n");
        let (reply, _, _) = command(&mut state, "USER alice");
        assert_eq!(reply, "+OK\r\n");
        let (_, close, attempt) = command(&mut state, "PASS secret");
        assert!(!close);
        let attempt = attempt.expect("PASS completes the authorization attempt");
        assert_eq!(attempt.user, "alice");
        assert_eq!(attempt.password.as_deref(), Some("secret"));
        assert_eq!(attempt.method, "plain");
    }

    #[test]
    fn pop3_auth_login_prompts_then_completes_authorization() {
        let mut state = new_state(MailProtocol::Pop3);
        let (reply, _, _) = command(&mut state, "AUTH LOGIN");
        assert_eq!(reply, "+ VXNlcm5hbWU6\r\n");
        let (reply, _, _) = command(&mut state, "YWxpY2U=");
        assert_eq!(reply, "+ UGFzc3dvcmQ6\r\n");
        let (_, _, attempt) = command(&mut state, "c2VjcmV0");
        let attempt = attempt.expect("LOGIN completion");
        assert_eq!(attempt.user, "alice");
        assert_eq!(attempt.password.as_deref(), Some("secret"));
    }

    #[test]
    fn imap_capability_login_and_logout_match_nginx_1296() {
        let mut state = new_state(MailProtocol::Imap);
        assert_eq!(state.greeting(), b"* OK IMAP4 ready\r\n");
        let (reply, _, _) = command(&mut state, "a1 CAPABILITY");
        assert_eq!(
            reply,
            "* CAPABILITY IMAP4 IMAP4rev1 UIDPLUS AUTH=PLAIN AUTH=LOGIN\r\na1 OK completed\r\n"
        );
        let (reply, _, _) = command(&mut state, "a2 NOOP");
        assert_eq!(reply, "a2 OK completed\r\n");
        let (_, _, attempt) = command(&mut state, "a3 LOGIN alice secret");
        let attempt = attempt.expect("LOGIN completes the authorization attempt");
        assert_eq!(attempt.user, "alice");
        assert_eq!(attempt.imap_tag.as_deref(), Some("a3"));
        let mut logout_state = new_state(MailProtocol::Imap);
        let (reply, close, _) = command(&mut logout_state, "a4 LOGOUT");
        assert_eq!(reply, "* BYE\r\na4 OK completed\r\n");
        assert!(close);
        let (reply, _, _) = command(&mut logout_state, "BOGUS");
        assert_eq!(reply, "* BAD invalid command\r\n");
    }

    #[test]
    fn smtp_ehlo_helo_and_auth_match_nginx_1296() {
        let mut state = new_state(MailProtocol::Smtp);
        assert_eq!(state.greeting(), b"220 mail.example.com ESMTP ready\r\n");
        let (reply, _, _) = command(&mut state, "EHLO client.example");
        assert_eq!(reply, "250-mail.example.com\r\n250 AUTH PLAIN LOGIN\r\n");
        let (reply, _, _) = command(&mut state, "HELO client.example");
        assert_eq!(reply, "250 mail.example.com\r\n");
        let (reply, _, _) = command(&mut state, "NOOP");
        assert_eq!(reply, "250 2.0.0 OK\r\n");
        let (reply, _, _) = command(&mut state, "MAIL FROM:<a@b>");
        assert_eq!(reply, "530 5.7.1 Authentication required\r\n");
        let (reply, _, _) = command(&mut state, "AUTH CRAM-MD5");
        assert_eq!(reply, "500 5.5.1 Invalid command\r\n");
        let encoded = sdkwork_webserver_mail_protocol::auth::encode_plain(
            "alice",
            "secret",
            &session_limits(4096),
        )
        .expect("encode");
        let (_, _, attempt) = command(&mut state, &format!("AUTH PLAIN {encoded}"));
        let attempt = attempt.expect("AUTH PLAIN completes the authorization attempt");
        assert_eq!(attempt.user, "alice");
        assert_eq!(attempt.password.as_deref(), Some("secret"));
        let mut quit_state = new_state(MailProtocol::Smtp);
        let (reply, close, _) = command(&mut quit_state, "QUIT");
        assert_eq!(reply, "221 2.0.0 Bye\r\n");
        assert!(close);
    }

    #[test]
    fn smtp_auth_login_prompts_use_334_continuations() {
        let mut state = new_state(MailProtocol::Smtp);
        let (reply, _, _) = command(&mut state, "AUTH LOGIN");
        assert_eq!(reply, "334 VXNlcm5hbWU6\r\n");
        let (reply, _, _) = command(&mut state, "YWxpY2U=");
        assert_eq!(reply, "334 UGFzc3dvcmQ6\r\n");
        let (_, _, attempt) = command(&mut state, "c2VjcmV0");
        assert_eq!(
            attempt.expect("LOGIN completion").password.as_deref(),
            Some("secret")
        );
    }

    #[test]
    fn malformed_credentials_do_not_complete_authorization() {
        let mut state = new_state(MailProtocol::Pop3);
        let (reply, close, _) = command(&mut state, "AUTH LOGIN");
        assert_eq!(reply, "+ VXNlcm5hbWU6\r\n");
        assert!(!close);
        let (reply, close, attempt) = command(&mut state, "!!!not-base64!!!");
        assert_eq!(reply, "-ERR invalid command\r\n");
        assert!(!close);
        assert!(attempt.is_none());
        assert_eq!(
            state.session.state(),
            sdkwork_webserver_mail_protocol::frontend::FrontendState::Initial
        );
    }

    #[test]
    fn proxy_header_limits_reject_oversized_input() {
        let limits = session_limits(4096);
        let oversized = vec![b'A'; limits.max_relay_header_bytes + 1];
        assert!(ProxyProtocolV1::parse(&oversized, &limits).is_err());
        let valid = b"PROXY TCP4 203.0.113.7 198.51.100.9 12345 25\r\n";
        let (header, consumed) = ProxyProtocolV1::parse(valid, &limits).expect("valid header");
        assert_eq!(consumed, valid.len());
        assert_eq!(
            header.source().map(|address| address.ip().to_string()),
            Some("203.0.113.7".to_owned())
        );
    }

    fn auth_http_profile() -> AuthHttpProfile {
        AuthHttpProfile {
            address: "127.0.0.1:9000".parse().expect("address"),
            host_header: "127.0.0.1".to_owned(),
            path: "/auth".to_owned(),
            headers: vec!["X-Tenant: acme".to_owned()],
            timeout: Duration::from_secs(5),
            pass_client_cert: false,
        }
    }

    fn auth_attempt() -> AuthAttempt {
        AuthAttempt {
            user: "alice smith".to_owned(),
            password: Some("s3 cret%".to_owned()),
            method: "plain",
            imap_tag: Some("a1".to_owned()),
        }
    }

    #[test]
    fn auth_http_request_matches_the_nginx_exchange() {
        let auth = auth_http_profile();
        let profile = profile(MailProtocol::Smtp);
        let attempt = auth_attempt();
        let facts = AuthHttpRequestFacts {
            attempt: &attempt,
            attempt_number: 2,
            client_ip: "203.0.113.9".parse().expect("ip"),
            proxy_header: None,
            tls: AuthHttpTlsFacts {
                protocol: None,
                cipher: None,
            },
        };
        let request = String::from_utf8(build_auth_http_request(&auth, &profile, &facts))
            .expect("utf-8 request");
        assert_eq!(
            request,
            "GET /auth HTTP/1.0\r\n\
             Host: 127.0.0.1\r\n\
             Auth-Method: plain\r\n\
             Auth-User: alice%20smith\r\n\
             Auth-Pass: s3%20cret%25\r\n\
             Auth-Protocol: smtp\r\n\
             Auth-Login-Attempt: 2\r\n\
             Client-IP: 203.0.113.9\r\n\
             Client-Host: [UNAVAILABLE]\r\n\
             X-Tenant: acme\r\n\
             \r\n"
        );
    }

    #[test]
    fn auth_http_response_parses_nginx_headers() {
        let response = b"HTTP/1.0 200 OK\r\nAuth-Status: OK\r\nAuth-Server: 127.0.0.1\r\nAuth-Port: 2525\r\nAuth-User: bob\r\nAuth-Pass: pw\r\n\r\n";
        let parsed = parse_auth_http_response(response).expect("parse");
        assert_eq!(parsed.status, "OK");
        assert_eq!(parsed.server.as_deref(), Some("127.0.0.1"));
        assert_eq!(parsed.port, Some(2525));
        assert_eq!(parsed.login.as_deref(), Some("bob"));
        assert_eq!(parsed.password.as_deref(), Some("pw"));

        let failure = b"HTTP/1.0 200 OK\r\nAuth-Status: invalid credentials\r\nAuth-Error-Code: 535 5.7.0\r\n\r\n";
        let parsed = parse_auth_http_response(failure).expect("parse");
        assert_eq!(parsed.status, "invalid credentials");
        assert_eq!(parsed.error_code.as_deref(), Some("535 5.7.0"));
    }

    #[test]
    fn auth_http_response_rejects_malformed_input() {
        assert!(parse_auth_http_response(b"").is_err());
        assert!(parse_auth_http_response(b"HTTP/1.0 200 OK\r\nBroken\r\n\r\n").is_err());
        assert!(
            parse_auth_http_response(b"HTTP/1.0 200 OK\r\nAuth-Port: not-a-port\r\n\r\n").is_err()
        );
    }

    #[test]
    fn mail_auth_escaping_matches_nginx() {
        assert_eq!(escape_mail_auth("alice"), "alice");
        assert_eq!(escape_mail_auth("a b"), "a%20b");
        assert_eq!(escape_mail_auth("a%b"), "a%25b");
        assert_eq!(escape_mail_auth("a\r\nb"), "a%0D%0Ab");
    }
}
