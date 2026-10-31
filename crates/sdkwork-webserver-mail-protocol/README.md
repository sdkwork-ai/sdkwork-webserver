# sdkwork-webserver-mail-protocol

Bounded, transport-free POP3/IMAP/SMTP dialog substrate for the nginx-compatible
mail proxy (`mail {}` module). This is substrate only: the mail listener wiring
(sockets, TLS termination, `auth_http` lookup, backend dial) happens in a later
wave and calls these types.

## Modules

| Module | Responsibility |
| --- | --- |
| `line` | Incremental bounded CRLF/LF line framing (`LineReader`, `encode_line`). |
| `auth` | SASL `PLAIN` encode/decode, `LOGIN` continuation codec, two-prompt `LoginDialog`, mechanism tokens. |
| `dot_stuffing` | Dot-stuffed multiline encode/decode (`DotStuffingReader`, `encode_multiline`). |
| `pop3` | POP3 client dialog: greeting, `USER`/`PASS`/`APOP`, `AUTH PLAIN/LOGIN`, `CAPA`, `STLS` + STARTTLS boundary, `QUIT`; `+OK`/`-ERR` and multiline reply parsing. |
| `imap` | IMAP client dialog: greeting, tagged commands, `CAPABILITY`, `STARTTLS`, `LOGIN`, `AUTHENTICATE PLAIN/LOGIN`, bounded literals, `BYE`/`NO`/`BAD`. |
| `smtp` | SMTP client dialog: `220` greeting, `EHLO`/`HELO` capability negotiation, `STARTTLS`, `AUTH PLAIN/LOGIN`, `XCLIENT`, `MAIL FROM`/`RCPT TO` (enumerated + pipelined), bounded dot-stuffed `DATA`, `QUIT`. |
| `frontend` | `FrontendSession`: explicit states (`initial`, `authenticating`, `authenticated`, `tls-upgrade`, `relaying`, `closed`) and a fail-closed transition table per protocol. |
| `relay` | `BackendRelay`: backend greeting validation, bounded line relay, `finish`; `bootstrap_lines` builds the `EHLO`/`XCLIENT`/`CAPA` bootstrap. |
| `xclient` | nginx-compatible `XCLIENT ADDR=[IPV6:]addr [LOGIN=xtext] NAME=host` builder/parser. |
| `proxy_protocol` | PROXY protocol v1 header builder/parser (`TCP4`/`TCP6`/`UNKNOWN`). |

## Design rules

- **Pure**: no sockets, no timers, no filesystem, no async. Deterministic.
- **Bounded before growth**: every buffer is capped by `Limits`; the decoders fail
  closed with typed `Error` values (`LineTooLong`, `LimitExceeded`,
  `LiteralTooLarge`, `UnterminatedMultiline`, `TrailingBytes`, ...).
- **Fail closed**: out-of-order replies, invalid transitions, forbidden
  credential bytes and malformed wire lines are errors, never coerced.
- **No panics / no `unwrap`** in public paths; adversarial bytes are covered by
  the integration tests.
- **Neutral API**: callers own the transport/TLS; the crate exposes command
  *builders* (one command at a time, with pending-reply tracking) and *parsers*
  (`on_line`, `feed_literal`), plus the transition/relay glue.

## Entry points for the wiring wave

```text
line::LineReader::new(protocol, limits) / feed(&[u8]) / finish()
line::encode_line(text, limits)
auth::{encode_plain, decode_plain, encode_login_response, decode_login_response, LoginDialog, AuthMechanism}
dot_stuffing::{DotStuffingReader, encode_multiline, decode_multiline}
pop3::Pop3Client::{new, state, greeting, user, pass, apop, stls, capa, auth_plain, auth_login, auth_login_response, quit, on_line, tls_established}
imap::ImapClient::{new, state, next_tag, capability, starttls, login, authenticate_plain, authenticate_login, auth_response, logout, on_line, feed_literal, tls_established}
smtp::SmtpClient::{new, state, capabilities, has_capability, greeting, ehlo, helo, starttls, auth_plain, auth_login, auth_continuation, xclient, mail_from, rcpt_to, pipeline, data, data_body, quit, on_line, tls_established}
frontend::{MailProtocol, FrontendState, FrontendAction, FrontendSession}
relay::{BackendRelay, RelayPhase, BackendBootstrap, bootstrap_lines}
xclient::{XclientRequest, XclientAttribute}
proxy_protocol::ProxyProtocolV1
error::{Error, Result}; limits::Limits
```

## Verification

```bash
cargo test -p sdkwork-webserver-mail-protocol
cargo clippy -p sdkwork-webserver-mail-protocol --all-targets -- -D warnings
cargo fmt --all -- --check
RUSTFLAGS="-D missing_docs" cargo check -p sdkwork-webserver-mail-protocol
node ../sdkwork-specs/tools/check-rust-crate-naming-standard.mjs --root .
node ../sdkwork-specs/tools/check-rust-manifest-standard.mjs --root .
```

Out of scope for this substrate (documented blockers for the wiring wave):
implicit-TLS transport itself, `auth_http` request formatting, backend TLS,
mail-module directive/catalog materialization, and data-plane wiring.
