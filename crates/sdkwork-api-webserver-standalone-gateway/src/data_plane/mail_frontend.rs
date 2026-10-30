//! nginx `mail {}` frontend listener plane (Wave 9A).
//!
//! This is the plaintext POP3/IMAP/SMTP session plane: TCP listeners, bounded
//! client command framing, the nginx greeting and authorization dialog, and
//! optional inbound PROXY protocol v1. The wire dialogs are driven from the
//! transport-free [`sdkwork_webserver_mail_protocol`] state machines
//! (`FrontendSession`, auth codecs, PROXY parser, `LineReader`, `Limits`).
//!
//! # Wave 9A authentication boundary
//!
//! nginx mail authentication is performed by `auth_http`; without it nginx
//! refuses to start (`no "auth_http" is defined for server`). Wave 9A has no
//! auth service, so a configured listener serves the greeting and the
//! authorization dialog, then answers that attempt exactly like nginx with an
//! unreachable auth service (`-ERR internal server error` /
//! `* BAD internal server error` / `451 4.3.2 Internal server error`,
//! differential-probed against nginx 1.29.6) and closes the connection. The
//! failure is logged with the missing service and the wave that provides it.
//!
//! Nothing is authenticated and nothing is relayed: `auth_http`, mail TLS,
//! XCLIENT and the backend relay are Wave 9B and are refused at configuration
//! time by the nginx materializer.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use sdkwork_webserver_core::{MailAuthMechanism, MailServerConfig, MailServerProtocol};
use sdkwork_webserver_mail_protocol::{
    auth::{decode_login_response, decode_plain},
    frontend::{FrontendAction, FrontendSession, MailProtocol},
    line::LineReader,
    proxy_protocol::ProxyProtocolV1,
    Limits,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    time::{sleep, timeout},
};

use super::{accept, DataPlaneError, DataPlaneRuntime};

/// Maximum client commands one Wave 9A session processes before closing.
///
/// The session always closes at the first authentication attempt, so this is a
/// defence against a client that only sends non-auth chatter.
const MAX_SESSION_COMMANDS: usize = 64;
/// Read chunk for one client command batch.
const MAIL_READ_CHUNK_BYTES: usize = 4096;
/// Base64 prompts nginx sends for LOGIN/`AUTH LOGIN` continuations.
const LOGIN_USER_PROMPT: &str = "VXNlcm5hbWU6";
const LOGIN_PASSWORD_PROMPT: &str = "UGFzc3dvcmQ6";

/// One prepared mail listener.
pub(crate) struct PreparedMailListener {
    pub(crate) id: String,
    socket: TcpListener,
    protocol: MailProtocol,
    greeting_name: String,
    timeout: Duration,
    so_keepalive: bool,
    proxy_protocol: bool,
    max_line_bytes: usize,
    greeting_delay: Duration,
    mechanisms: Vec<MailAuthMechanism>,
}

impl PreparedMailListener {
    /// Bind the listener socket and resolve the bounded session profile.
    pub(crate) async fn prepare(config: &MailServerConfig) -> Result<Self, DataPlaneError> {
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
        tracing::info!(
            mail_server_id = %config.id,
            protocol = config.protocol.as_str(),
            address = %address,
            proxy_protocol = config.proxy_protocol,
            "mail listener prepared (Wave 9A plaintext dialog; no auth_http)"
        );
        Ok(Self {
            id: config.id.clone(),
            socket,
            protocol,
            greeting_name: config.effective_server_name(),
            timeout: Duration::from_millis(config.timeout_ms),
            so_keepalive: config.so_keepalive,
            proxy_protocol: config.proxy_protocol,
            max_line_bytes: config.client_buffer_bytes(),
            greeting_delay,
            mechanisms: config.auth_mechanisms().to_vec(),
        })
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
                let id = listener.id.clone();
                let protocol = listener.protocol;
                let greeting_name = listener.greeting_name.clone();
                let session_timeout = listener.timeout;
                let so_keepalive = listener.so_keepalive;
                let proxy_protocol = listener.proxy_protocol;
                let max_line_bytes = listener.max_line_bytes;
                let greeting_delay = listener.greeting_delay;
                let mechanisms = listener.mechanisms.clone();
                tasks.spawn(async move {
                    let result = serve_mail_connection(
                        &id,
                        protocol,
                        &greeting_name,
                        session_timeout,
                        so_keepalive,
                        proxy_protocol,
                        max_line_bytes,
                        greeting_delay,
                        &mechanisms,
                        downstream,
                        peer,
                    )
                    .await;
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

/// Drive one plaintext mail session.
#[allow(clippy::too_many_arguments)]
async fn serve_mail_connection(
    server_id: &str,
    protocol: MailProtocol,
    greeting_name: &str,
    session_timeout: Duration,
    so_keepalive: bool,
    proxy_protocol: bool,
    max_line_bytes: usize,
    greeting_delay: Duration,
    mechanisms: &[MailAuthMechanism],
    mut stream: TcpStream,
    peer: SocketAddr,
) -> Result<(), ()> {
    if so_keepalive {
        if let Err(error) = socket2::SockRef::from(&stream).set_keepalive(true) {
            tracing::warn!(
                mail_server_id = server_id,
                peer = %peer,
                error = %error,
                "mail listener could not enable SO_KEEPALIVE"
            );
        }
    }
    let limits = session_limits(max_line_bytes);
    let mut pending: Vec<u8> = Vec::new();
    let client_ip = if proxy_protocol {
        match read_proxy_v1(&mut stream, &limits, session_timeout).await {
            Ok((header, leftover)) => {
                if !leftover.is_empty() {
                    pending = leftover;
                }
                header
                    .source()
                    .map(|address| address.ip())
                    .unwrap_or(peer.ip())
            }
            Err(reason) => {
                tracing::warn!(
                    mail_server_id = server_id,
                    peer = %peer,
                    reason,
                    "mail PROXY protocol header rejected; closing connection"
                );
                return Err(());
            }
        }
    } else {
        peer.ip()
    };
    let mut state = MailSessionState {
        id: server_id.to_owned(),
        peer,
        session: FrontendSession::new(protocol, false),
        mechanisms: mechanisms.to_vec(),
        greeting_name: greeting_name.to_owned(),
        limits,
        commands: 0,
        reply: Vec::new(),
        pop3_user: None,
        imap_tag: None,
        pending: PendingAuth::None,
    };
    if !greeting_delay.is_zero() {
        sleep(greeting_delay).await;
    }
    let greeting = state.greeting();
    if write_mail(&mut stream, &greeting, session_timeout)
        .await
        .is_err()
    {
        return Err(());
    }
    let mut reader = LineReader::new(protocol.as_str(), &state.limits);
    let mut chunk = [0_u8; MAIL_READ_CHUNK_BYTES];
    loop {
        if pending.is_empty() {
            match timeout(session_timeout, stream.read(&mut chunk)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(read)) => pending.extend_from_slice(&chunk[..read]),
                Ok(Err(error)) => {
                    tracing::debug!(mail_server_id = server_id, peer = %peer, error = %error, "mail connection read failed");
                    break;
                }
                Err(_) => {
                    tracing::debug!(mail_server_id = server_id, peer = %peer, "mail session idle timeout");
                    break;
                }
            }
        }
        let lines = match reader.feed(&pending) {
            Ok(lines) => {
                pending.clear();
                lines
            }
            Err(error) => {
                tracing::warn!(
                    mail_server_id = server_id,
                    peer = %peer,
                    error = %error,
                    "mail client command framing rejected; closing connection"
                );
                return Err(());
            }
        };
        let mut close = false;
        for line in lines {
            state.commands = state.commands.saturating_add(1);
            if state.commands > MAX_SESSION_COMMANDS {
                tracing::debug!(
                    mail_server_id = server_id,
                    peer = %peer,
                    "mail session command budget exhausted"
                );
                close = true;
                break;
            }
            let step = state.on_command(&line.text);
            // Write the reply before deciding to close: QUIT/LOGOUT and the
            // auth-service-unavailable error are final replies, not silence.
            let reply = state.take_reply();
            if !reply.is_empty()
                && write_mail(&mut stream, &reply, session_timeout)
                    .await
                    .is_err()
            {
                return Err(());
            }
            match step {
                DialogStep::Continue => {}
                DialogStep::Close => {
                    close = true;
                    break;
                }
                DialogStep::AuthUnavailable => {
                    tracing::info!(
                        mail_server_id = server_id,
                        peer = %peer,
                        client_ip = %client_ip,
                        protocol = protocol.as_str(),
                        "mail authentication is unavailable: auth_http is not configured (Wave 9B); closing after the authorization attempt"
                    );
                    close = true;
                    break;
                }
            }
        }
        if close {
            break;
        }
    }
    let _ = stream.shutdown().await;
    Ok(())
}

/// Bounded write with the session timeout.
async fn write_mail(stream: &mut TcpStream, bytes: &[u8], deadline: Duration) -> Result<(), ()> {
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
    /// Reply written, session closes (client QUIT/LOGOUT or a protocol error).
    Close,
    /// The client attempted authentication; the auth service is not configured
    /// in Wave 9A, so nginx's auth-service-unavailable reply was written and
    /// the session closes.
    AuthUnavailable,
}

/// Per-session dialog state shared by POP3/IMAP/SMTP.
struct MailSessionState {
    id: String,
    peer: SocketAddr,
    session: FrontendSession,
    mechanisms: Vec<MailAuthMechanism>,
    greeting_name: String,
    limits: Limits,
    commands: usize,
    reply: Vec<u8>,
    pop3_user: Option<String>,
    imap_tag: Option<String>,
    pending: PendingAuth,
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

impl MailSessionState {
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

    /// Take the reply produced by the last command.
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

    /// Record the client's authorization attempt and emit nginx's
    /// auth-service-unavailable reply for the protocol.
    fn begin_auth_unavailable(&mut self) -> DialogStep {
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
        let _ = self
            .session
            .apply(FrontendAction::Close, None, &self.limits);
        match self.session.protocol() {
            MailProtocol::Pop3 => self.push_reply_line("-ERR internal server error"),
            MailProtocol::Imap => self.push_reply_line("* BAD internal server error"),
            MailProtocol::Smtp => self.push_reply_line("451 4.3.2 Internal server error"),
        }
        DialogStep::AuthUnavailable
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
                if decode_login_response(line.trim(), &self.limits).is_err() {
                    self.protocol_bad_command();
                    return DialogStep::Continue;
                }
                self.pending = PendingAuth::LoginPassword;
                match self.session.protocol() {
                    MailProtocol::Pop3 => {
                        self.push_reply_line(&format!("+ {LOGIN_PASSWORD_PROMPT}"))
                    }
                    MailProtocol::Imap => {
                        self.push_reply_line(&format!("+ {LOGIN_PASSWORD_PROMPT}"))
                    }
                    MailProtocol::Smtp => {
                        self.push_reply_line(&format!("334 {LOGIN_PASSWORD_PROMPT}"))
                    }
                }
                DialogStep::Continue
            }
            PendingAuth::LoginPassword => {
                if decode_login_response(line.trim(), &self.limits).is_err() {
                    self.protocol_bad_command();
                    return DialogStep::Continue;
                }
                self.begin_auth_unavailable()
            }
            PendingAuth::PlainContinuation => {
                let decoded = line.trim();
                let decoded = if decoded == "=" { "" } else { decoded };
                if decode_plain(decoded, &self.limits).is_err() {
                    self.protocol_bad_command();
                    return DialogStep::Continue;
                }
                self.begin_auth_unavailable()
            }
            PendingAuth::None => {
                self.unknown_command();
                DialogStep::Continue
            }
        }
    }

    fn protocol_bad_command(&mut self) {
        match self.session.protocol() {
            MailProtocol::Pop3 => self.push_reply_line("-ERR invalid command"),
            MailProtocol::Imap => match self.imap_tag.clone() {
                Some(tag) => self.push_reply_line(&format!("{tag} BAD invalid command")),
                None => self.push_reply_line("* BAD invalid command"),
            },
            MailProtocol::Smtp => self.push_reply_line("501 5.5.2 Cannot decode response"),
        }
    }

    fn unknown_command(&mut self) {
        match self.session.protocol() {
            MailProtocol::Pop3 => self.push_reply_line("-ERR invalid command"),
            MailProtocol::Imap => match self.imap_tag.clone() {
                Some(tag) => self.push_reply_line(&format!("{tag} BAD invalid command")),
                None => self.push_reply_line("* BAD invalid command"),
            },
            MailProtocol::Smtp => self.push_reply_line("500 5.5.1 Invalid command"),
        }
    }

    // ---- POP3 ---------------------------------------------------------

    fn on_pop3(&mut self, line: &str) -> DialogStep {
        let (verb, argument) = split_mail_command(line);
        match verb.as_str() {
            "CAPA" => {
                let mut tokens =
                    String::from("+OK Capability list follows\r\nTOP\r\nUSER\r\nUIDL\r\nSASL");
                for mechanism in self.mechanism_tokens() {
                    tokens.push(' ');
                    tokens.push_str(mechanism);
                }
                tokens.push_str("\r\n.\r\n");
                self.push_reply(&tokens);
                DialogStep::Continue
            }
            "NOOP" => {
                self.push_reply_line("+OK");
                DialogStep::Continue
            }
            "QUIT" => {
                self.push_reply_line("+OK");
                let _ = self
                    .session
                    .apply(FrontendAction::Close, None, &self.limits);
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
                if non_empty(argument).is_none() || self.pop3_user.is_none() {
                    self.push_reply_line("-ERR invalid command");
                    return DialogStep::Continue;
                }
                self.begin_auth_unavailable()
            }
            "APOP" => {
                // APOP requires the compiled-in APOP credential path; the
                // bounded 9A vocabulary advertises PLAIN/LOGIN only (nginx's
                // default, differential-probed).
                self.push_reply_line("-ERR invalid command");
                DialogStep::Continue
            }
            "AUTH" => self.on_pop3_auth(argument),
            "STLS" => {
                // No mail TLS in Wave 9A.
                self.push_reply_line("-ERR invalid command");
                DialogStep::Continue
            }
            _ => {
                self.push_reply_line("-ERR invalid command");
                DialogStep::Continue
            }
        }
    }

    fn on_pop3_auth(&mut self, argument: Option<&str>) -> DialogStep {
        let (mechanism, initial) = split_mechanism(argument);
        match mechanism.to_ascii_uppercase().as_str() {
            "PLAIN" => {
                if let Some(initial) = initial.filter(|value| !value.is_empty()) {
                    let decoded = if initial == "=" { "" } else { initial };
                    if decode_plain(decoded, &self.limits).is_err() {
                        self.push_reply_line("-ERR invalid command");
                        return DialogStep::Continue;
                    }
                    self.begin_auth_unavailable()
                } else {
                    self.pending = PendingAuth::PlainContinuation;
                    self.push_reply_line("+ ");
                    DialogStep::Continue
                }
            }
            "LOGIN" => {
                self.pending = PendingAuth::LoginUser;
                self.push_reply_line(&format!("+ {LOGIN_USER_PROMPT}"));
                DialogStep::Continue
            }
            _ => {
                self.push_reply_line("-ERR invalid command");
                DialogStep::Continue
            }
        }
    }

    // ---- IMAP ---------------------------------------------------------

    fn on_imap(&mut self, line: &str) -> DialogStep {
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
                for mechanism in self.mechanism_tokens() {
                    text.push_str(" AUTH=");
                    text.push_str(mechanism);
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
                let _ = self
                    .session
                    .apply(FrontendAction::Close, None, &self.limits);
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
                self.begin_auth_unavailable()
            }
            "AUTHENTICATE" => self.on_imap_authenticate(&tag, argument),
            _ => {
                self.push_reply_line(&format!("{tag} BAD invalid command"));
                DialogStep::Continue
            }
        }
    }

    fn on_imap_authenticate(&mut self, tag: &str, argument: Option<&str>) -> DialogStep {
        let (mechanism, initial) = split_mechanism(argument);
        match mechanism.to_ascii_uppercase().as_str() {
            "PLAIN" => {
                if let Some(initial) = initial.filter(|value| !value.is_empty()) {
                    let decoded = if initial == "=" { "" } else { initial };
                    if decode_plain(decoded, &self.limits).is_err() {
                        self.push_reply_line(&format!("{tag} BAD invalid command"));
                        return DialogStep::Continue;
                    }
                    self.begin_auth_unavailable()
                } else {
                    self.pending = PendingAuth::PlainContinuation;
                    self.push_reply_line("+ ");
                    DialogStep::Continue
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
                let mut text = format!("250-{}\r\n250 AUTH", self.greeting_name);
                for mechanism in self.mechanism_tokens() {
                    text.push(' ');
                    text.push_str(mechanism);
                }
                text.push_str("\r\n");
                self.push_reply(&text);
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
                self.push_reply_line(&format!("250 {}", self.greeting_name));
                DialogStep::Continue
            }
            "NOOP" | "RSET" => {
                self.push_reply_line("250 2.0.0 OK");
                DialogStep::Continue
            }
            "QUIT" => {
                self.push_reply_line("221 2.0.0 Bye");
                let _ = self
                    .session
                    .apply(FrontendAction::Close, None, &self.limits);
                DialogStep::Close
            }
            "MAIL" => {
                self.push_reply_line("530 5.7.1 Authentication required");
                DialogStep::Continue
            }
            "RCPT" => {
                // No transaction can be open before authentication, so nginx
                // reports a bad sequence here rather than the auth hint.
                self.push_reply_line("503 5.5.1 Bad sequence of commands");
                DialogStep::Continue
            }
            "DATA" => {
                self.push_reply_line("500 5.5.1 Invalid command");
                DialogStep::Continue
            }
            "AUTH" => self.on_smtp_auth(argument),
            "STARTTLS" => {
                // No mail TLS in Wave 9A; nginx answers invalid command when
                // STARTTLS is not advertised.
                self.push_reply_line("500 5.5.1 Invalid command");
                DialogStep::Continue
            }
            _ => {
                self.push_reply_line("500 5.5.1 Invalid command");
                DialogStep::Continue
            }
        }
    }

    fn on_smtp_auth(&mut self, argument: Option<&str>) -> DialogStep {
        let (mechanism, initial) = split_mechanism(argument);
        match mechanism.to_ascii_uppercase().as_str() {
            "PLAIN" => {
                if let Some(initial) = initial.filter(|value| !value.is_empty()) {
                    let decoded = if initial == "=" { "" } else { initial };
                    if decode_plain(decoded, &self.limits).is_err() {
                        self.push_reply_line("501 5.5.2 Cannot decode response");
                        return DialogStep::Continue;
                    }
                    self.begin_auth_unavailable()
                } else {
                    self.pending = PendingAuth::PlainContinuation;
                    self.push_reply_line("334 ");
                    DialogStep::Continue
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state(protocol: MailProtocol) -> MailSessionState {
        MailSessionState {
            id: "mail-test".to_owned(),
            peer: "127.0.0.1:12345".parse().expect("peer"),
            session: FrontendSession::new(protocol, false),
            mechanisms: vec![MailAuthMechanism::Plain, MailAuthMechanism::Login],
            greeting_name: "mail.example.com".to_owned(),
            limits: session_limits(4096),
            commands: 0,
            reply: Vec::new(),
            pop3_user: None,
            imap_tag: None,
            pending: PendingAuth::None,
        }
    }

    fn command(state: &mut MailSessionState, line: &str) -> (String, bool) {
        let step = state.on_command(line);
        let reply = String::from_utf8(state.take_reply()).expect("utf-8 reply");
        let close = matches!(step, DialogStep::Close | DialogStep::AuthUnavailable);
        (reply, close)
    }

    #[test]
    fn pop3_greeting_and_dialog_match_nginx_1296() {
        let mut state = state(MailProtocol::Pop3);
        assert_eq!(state.greeting(), b"+OK POP3 ready\r\n");
        let (reply, close) = command(&mut state, "CAPA");
        assert_eq!(
            reply,
            "+OK Capability list follows\r\nTOP\r\nUSER\r\nUIDL\r\nSASL PLAIN LOGIN\r\n.\r\n"
        );
        assert!(!close);
        let (reply, _) = command(&mut state, "NOOP");
        assert_eq!(reply, "+OK\r\n");
        let (reply, _) = command(&mut state, "USER alice");
        assert_eq!(reply, "+OK\r\n");
        let (reply, close) = command(&mut state, "PASS secret");
        assert_eq!(reply, "-ERR internal server error\r\n");
        assert!(close);
        let (reply, _) = command(&mut state, "BOGUS");
        assert_eq!(reply, "-ERR invalid command\r\n");
    }

    #[test]
    fn pop3_auth_login_prompts_and_closes_with_auth_unavailable() {
        let mut state = state(MailProtocol::Pop3);
        let (reply, close) = command(&mut state, "AUTH LOGIN");
        assert_eq!(reply, "+ VXNlcm5hbWU6\r\n");
        assert!(!close);
        let (reply, close) = command(&mut state, "YWxpY2U=");
        assert_eq!(reply, "+ UGFzc3dvcmQ6\r\n");
        assert!(!close);
        let (reply, close) = command(&mut state, "c2VjcmV0");
        assert_eq!(reply, "-ERR internal server error\r\n");
        assert!(close);
    }

    #[test]
    fn imap_capability_login_and_logout_match_nginx_1296() {
        let mut state = state(MailProtocol::Imap);
        assert_eq!(state.greeting(), b"* OK IMAP4 ready\r\n");
        let (reply, _) = command(&mut state, "a1 CAPABILITY");
        assert_eq!(
            reply,
            "* CAPABILITY IMAP4 IMAP4rev1 UIDPLUS AUTH=PLAIN AUTH=LOGIN\r\na1 OK completed\r\n"
        );
        let (reply, _) = command(&mut state, "a2 NOOP");
        assert_eq!(reply, "a2 OK completed\r\n");
        let (reply, close) = command(&mut state, "a3 LOGIN alice secret");
        assert_eq!(reply, "* BAD internal server error\r\n");
        assert!(close);
        let (reply, close) = command(&mut state, "a4 LOGOUT");
        assert_eq!(reply, "* BYE\r\na4 OK completed\r\n");
        assert!(close);
        let (reply, _) = command(&mut state, "BOGUS");
        assert_eq!(reply, "* BAD invalid command\r\n");
        let (reply, _) = command(&mut state, "a5 BOGUS");
        assert_eq!(reply, "a5 BAD invalid command\r\n");
    }

    #[test]
    fn smtp_ehlo_helo_and_auth_match_nginx_1296() {
        let mut state = state(MailProtocol::Smtp);
        assert_eq!(state.greeting(), b"220 mail.example.com ESMTP ready\r\n");
        let (reply, _) = command(&mut state, "EHLO client.example");
        assert_eq!(reply, "250-mail.example.com\r\n250 AUTH PLAIN LOGIN\r\n");
        let (reply, _) = command(&mut state, "HELO client.example");
        assert_eq!(reply, "250 mail.example.com\r\n");
        let (reply, _) = command(&mut state, "NOOP");
        assert_eq!(reply, "250 2.0.0 OK\r\n");
        let (reply, _) = command(&mut state, "MAIL FROM:<a@b>");
        assert_eq!(reply, "530 5.7.1 Authentication required\r\n");
        let (reply, _) = command(&mut state, "RCPT TO:<a@b>");
        assert_eq!(reply, "503 5.5.1 Bad sequence of commands\r\n");
        let (reply, _) = command(&mut state, "AUTH CRAM-MD5");
        assert_eq!(reply, "500 5.5.1 Invalid command\r\n");
        let (reply, close) = command(&mut state, "AUTH PLAIN AGEAYg==");
        assert_eq!(reply, "451 4.3.2 Internal server error\r\n");
        assert!(close);
        let (reply, close) = command(&mut state, "QUIT");
        assert_eq!(reply, "221 2.0.0 Bye\r\n");
        assert!(close);
    }

    #[test]
    fn smtp_auth_login_prompts_use_334_continuations() {
        let mut state = state(MailProtocol::Smtp);
        let (reply, _) = command(&mut state, "AUTH LOGIN");
        assert_eq!(reply, "334 VXNlcm5hbWU6\r\n");
        let (reply, _) = command(&mut state, "YWxpY2U=");
        assert_eq!(reply, "334 UGFzc3dvcmQ6\r\n");
        let (reply, close) = command(&mut state, "c2VjcmV0");
        assert_eq!(reply, "451 4.3.2 Internal server error\r\n");
        assert!(close);
    }

    #[test]
    fn malformed_credentials_do_not_complete_authentication() {
        let mut state = state(MailProtocol::Pop3);
        let (reply, _) = command(&mut state, "AUTH LOGIN");
        assert_eq!(reply, "+ VXNlcm5hbWU6\r\n");
        let (reply, close) = command(&mut state, "!!!not-base64!!!");
        assert_eq!(reply, "-ERR invalid command\r\n");
        assert!(!close);
        assert!(state.session.authenticated_user().is_none());
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
}
