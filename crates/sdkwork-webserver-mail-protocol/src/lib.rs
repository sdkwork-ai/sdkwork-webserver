//! Bounded POP3, IMAP and SMTP dialog substrate for the nginx-compatible mail
//! proxy.
//!
//! The crate is transport-free on purpose: it owns the protocol state machines,
//! line framing, auth-line codecs, dot-stuffing, STARTTLS boundaries, frontend
//! session transitions, backend relay glue and the `XCLIENT` / PROXY-protocol
//! header builders. The eventual mail listener wiring opens sockets and drives
//! these types. Nothing here panics on adversarial input, performs I/O, allocates
//! beyond the configured [`Limits`], or accepts an out-of-state transition.
//!
//! # Entry points
//!
//! - [`line::LineReader`] / [`line::encode_line`]: bounded line framing shared by
//!   every protocol.
//! - [`auth::LoginDialog`], [`auth::encode_plain`], [`auth::decode_plain`]:
//!   SASL PLAIN/LOGIN auth-line codecs.
//! - [`dot_stuffing::DotStuffingReader`] / [`dot_stuffing::encode_multiline`]:
//!   multiline dot-stuffed framing (`+OK`/`CAPA`/`DATA`).
//! - [`pop3::Pop3Client`], [`imap::ImapClient`], [`smtp::SmtpClient`]: per-protocol
//!   client-side dialog state machines (command builders + reply parsers).
//! - [`frontend::FrontendSession`]: the client-facing session state machine with
//!   explicit states and fail-closed transitions.
//! - [`relay::BackendRelay`]: post-auth line relay/backpressure glue.
//! - [`xclient::XclientRequest`], [`proxy_protocol::ProxyProtocolV1`]: nginx
//!   `xclient` and `proxy_protocol` header builders/parsers for the backend leg.
//!
//! # Error model
//!
//! Every failure is a typed [`Error`]; protocol modules wrap it in
//! [`Result`]. A malformed, oversized, truncated or out-of-state input fails
//! closed instead of being coerced.

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod auth;
pub mod dot_stuffing;
pub mod error;
pub mod frontend;
pub mod imap;
pub mod limits;
pub mod line;
pub mod pop3;
pub mod proxy_protocol;
pub mod relay;
pub mod smtp;
pub mod xclient;

pub use error::{Error, Result};
pub use limits::Limits;
