//! Pure, bounded codecs for the nginx upstream protocols the SDKWork edge can
//! speak but does not yet wire into configuration.
//!
//! The crate is the *substrate* phase of the upstream-adapter work: every
//! module is a network-free, deterministic byte codec, and the edge's upstream
//! client (a later wave) drives it over its own sockets/HTTP/2 transport. That
//! separation keeps the wire formats testable against adversarial bytes without
//! a running upstream server, and keeps protocol policy (timeouts, retries,
//! connection reuse) in the caller.
//!
//! Codecs:
//!
//! - [`fastcgi`] — FastCGI 1.0 request encoder (`BEGIN_REQUEST`, `PARAMS`,
//!   `STDIN`, request-id allocation, `ABORT_REQUEST`) and a streaming response
//!   record parser (`STDOUT`/`STDERR`/`END_REQUEST`).
//! - [`uwsgi`] — uWSGI packet header and vars codec (`modifier1`, `datasize`,
//!   `modifier2`, length-prefixed key/value vars) plus the HTTP response parser
//!   used by `uwsgi_pass`.
//! - [`scgi`] — SCGI netstring request header and response parser.
//! - [`memcached`] — the memcached text protocol subset used by
//!   `memcached_pass` (`get`, plus a bounded `set` encoder and `VALUE`/`END`
//!   response parsing).
//! - [`grpc`] — gRPC-over-HTTP/2 adaptation helpers: request header plan,
//!   length-prefixed message framing, trailer (`grpc-status`/`grpc-message`)
//!   parsing, and timeout formatting.
//!
//! Shared pieces: [`cgi`] builds the CGI variable set that FastCGI, SCGI and
//! uWSGI all carry; [`header_block`] parses the CGI/HTTP-like response header
//! block those protocols return; [`limits`] holds every size bound; [`error`]
//! is the single typed error surface.
//!
//! All limits are hard bounds enforced before allocation, so malformed or
//! oversized frames fail with [`Error`] instead of growing a buffer. The crate
//! contains no `unsafe`, no I/O and no global state.

pub mod cgi;
pub mod error;
pub mod fastcgi;
pub mod grpc;
pub mod header_block;
pub mod limits;
pub mod memcached;
mod response_stream;
pub mod scgi;
pub mod uwsgi;

pub use cgi::UpstreamRequestFacts;
pub use error::{Error, Result};
pub use header_block::{HeaderStyle, HttpResponseHead};
pub use limits::Limits;
