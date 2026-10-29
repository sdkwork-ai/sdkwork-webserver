# sdkwork-webserver-upstream-protocol

Pure, bounded, network-free codecs for the nginx upstream protocols that are
still out of the SDKWork edge's `http-core-v1` profile:

| Codec | nginx module | Direction | Scope |
| --- | --- | --- | --- |
| FastCGI 1.0 | `ngx_http_fastcgi_module` | request encoder + response parser | `BEGIN_REQUEST`, `PARAMS`, `STDIN`, `ABORT_REQUEST`, request-id allocation; `STDOUT`/`STDERR`/`END_REQUEST` parsing |
| uWSGI | `ngx_http_uwsgi_module` | request encoder + response parser | 4-byte packet header, length-prefixed vars, optional `uwsgi_string`; raw-HTTP response parsing |
| SCGI | `ngx_http_scgi_module` | request encoder + response parser | netstring header with mandatory `CONTENT_LENGTH`; HTTP/CGI-header response parsing |
| memcached (text) | `ngx_http_memcached_module` | request encoder + response parser | `get` (and a bounded `set`), `VALUE`/`END`/store replies, nginx key escaping |
| gRPC over HTTP/2 | `ngx_http_grpc_module` | call plan + framing helpers | header plan, length-prefixed messages, trailer `grpc-status`/`grpc-message` parsing |

## Phase: transport substrate only

This crate is the **substrate phase** of the upstream-adapter work. It owns the
wire formats and nothing else: no sockets, no TLS, no connection pool, no
retries, no timeout policy, and no configuration wiring. The edge's upstream
client (a later wave) drives these codecs over its existing transport and owns
all runtime policy. Keeping the substrate pure makes it testable against
adversarial bytes without a live upstream server.

This crate is intentionally **not** wired into `agent.app.config.json`, the
directive catalog, or the runtime configuration model yet.

## Design rules

- **Bounded before allocation.** Every decoder checks the configured
  [`limits::Limits`] bound (header block, header line, value, metadata, message)
  before extending a buffer, so a hostile upstream cannot grow memory.
- **Fail closed.** Malformed, truncated, oversized, duplicate or unknown
  structures return the crate's typed [`error::Error`]; nothing is silently
  skipped.
- **No panics on input.** Library code has no `unwrap`/`expect`/`panic!`/
  `unreachable!`; adversarial-byte tests in `tests/codecs.rs` drive every parser.
- **Deterministic.** No clocks, randomness, I/O or global state; the request-id
  allocator is the only stateful helper and is explicit.
- **Neutral types.** The API uses `Vec<(String, String)>` / byte buffers so the
  caller can map to its own HTTP/2 or HTTP/1 representation without this crate
  depending on it.

## Reference behaviour

The encoders/parsers follow nginx 1.29.6's implementation and the published
protocols:

- FastCGI parameter length encoding, 8-byte record padding and the
  `BEGIN_REQUEST`/`PARAMS`/`STDIN` sequence match
  `ngx_http_fastcgi_create_request`; nginx's single connection id (`1`) is
  available through [`fastcgi::RequestIdAllocator`] for pooling callers.
- uWSGI's 4-byte header layout (`modifier1`, little-endian `datasize`,
  `modifier2`) and `u16`-prefixed key/value vars match the uWSGI protocol and
  `ngx_http_uwsgi_create_request`; request bodies follow the packet and are not
  counted in `datasize`.
- SCGI's netstring header and mandatory first `CONTENT_LENGTH` match
  `ngx_http_scgi_create_request`; responses accept the HTTP status-line and
  CGI-header forms nginx tries in order.
- memcached key escaping matches nginx `NGX_ESCAPE_MEMCACHED` (bytes
  `0x00..=0x1f`, space and `%`). The memcached **binary** protocol is out of
  scope; values are binary-safe on the text protocol.
- gRPC framing is `1` flag byte + 4-byte big-endian length + payload; trailers
  are percent-decoded per the gRPC protocol. Compression is signalled (the flag
  is preserved), never performed here.

## Public surface

```rust
use sdkwork_webserver_upstream_protocol::{
    cgi::{cgi_variables, UpstreamRequestFacts},
    fastcgi::{FastCgiParams, FastCgiResponseParser, RequestIdAllocator},
    grpc::{grpc_request_headers, GrpcMessageDecoder, parse_grpc_trailers},
    limits::Limits,
    memcached::{encode_get, MemcachedResponseParser},
    scgi::ScgiResponseParser,
    uwsgi::{encode_request as encode_uwsgi_request, UwsgiResponseParser},
};
```

Entry points by protocol:

- **FastCGI**: `FastCgiParams::{new,push,from_variables,encode}` with
  `decode_params` for round-trips → `encode_request` / `encode_request_head` +
  `encode_stdin_chunks` / `encode_begin_request` / `encode_abort_request`;
  responses through
  `FastCgiResponseParser::{new,feed,finish,is_complete,head,take_remaining,reset}`
  yielding `FastCgiResponseEvent::{Head,Body,Stderr,End}` and
  `FastCgiEndRequest { app_status, protocol_status }`.
- **uWSGI**: `UwsgiPacketHeader::{new,encode,decode}`,
  `encode_vars`/`decode_vars`, `encode_request(vars, extra, modifier1,
  modifier2, limits)`; responses through `UwsgiResponseParser`.
- **SCGI**: `encode_request(variables, body, limits)` /
  `encode_request_head(variables, content_length, limits)`; responses through
  `ScgiResponseParser`.
- **memcached**: `escape_key`, `encode_get`, `encode_set`; responses through
  `MemcachedResponseParser` or the one-shot `parse_get_response`
  (`MemcachedGetResponse::{Hit,Miss}`).
- **gRPC**: `grpc_request_headers`, `GrpcTimeout`, `encode_message` /
  `encode_message_with_flag`, `GrpcMessageDecoder`, `parse_grpc_trailers`,
  `grpc_status_name`, `grpc_status_is_ok`.

Shared: `cgi::cgi_variables` builds the ordered CGI variable set (dedicated
variables, `HTTP_*` mapping, `Cookie` joining) that all three CGI-family
encoders consume; `header_block::parse_response_head` is the incremental
header-block parser behind the FastCGI/SCGI/uWSGI response parsers.

## Limits

`limits::Limits` defaults: 64 KiB parameters / 512 entries, 32 KiB response
header block / 8 KiB header line / 256 header fields, 32 KiB FastCGI stdin
chunk, 64 KiB SCGI header, 250-byte memcached key / 1 MiB value, 4 MiB gRPC
message / 64 metadata fields. `Limits::sanitized()` clamps zeroed fields and the
16-bit wire fields, so a zeroed configuration cannot disable a bound.

## Verification

```bash
cargo test -p sdkwork-webserver-upstream-protocol
cargo clippy -p sdkwork-webserver-upstream-protocol -- -D warnings
cargo fmt -- --check
```
