# Nginx Wave 11B — `http2_chunk_size` and `random_index`

- Date: 2026-10-31
- Profile: `http-core-v1` (nginx 1.29.6 OSS reference)
- Scope: the last two implementable module-sized behavior-gap rows plus HTTP/2 DATA chunking
- Result: both directives are executable; 362 of 744 inventory rows are executable, 170
  accepted-unenforced, 199 fail-closed, 13 excluded

## 1. `http2_chunk_size <size>`

### What changed

- `http2_chunk_size` is accepted and materialized at **http, server and location** context
  (nginx's declared contexts). The value uses nginx's size grammar (`4k`, `8k`, `1m`, …),
  rejects `0` with nginx's *"the http2 chunk size cannot be zero"* rule, and clamps values
  above `NGX_HTTP_V2_MAX_FRAME_SIZE` (`2^24 - 1`) like nginx's post-handler.
- The typed plane has the same knob: `virtualHosts[].http2ChunkSize` (http → server
  resolved) and `routes[].http2ChunkSize` (location override). The typed `server.toml`
  materializer accepts `http2ChunkSize` at server and location too, and the JSON schema
  documents it.
- nginx's default is preserved at **8 KiB** (`ngx_http_v2_merge_loc_conf:
  ngx_conf_merge_size_value(conf->chunk_size, prev->chunk_size, 8 * 1024)`). nginx 1.29.6
  does not default to 4 KiB; the wave task's illustrative "default 4k" was not used because
  it would be a compatibility deviation.

### The hyper/h2 API limitation (reported, not faked)

The pinned `hyper` 1.x / `h2` 0.4 server path exposes **no server-side DATA-frame
chunk-size control**. `h2::SendStream::send_data` writes each body chunk as one DATA frame
and may split a chunk further for the peer's `SETTINGS_MAX_FRAME_SIZE` or flow control, but
it never merges chunks and offers no API to cap or request a frame boundary. There is no
nginx-equivalent knob to set on the hyper h2 builder (`max_frame_size` advertises the
peer-frame ceiling; it is not nginx's response chunk size).

The runtime therefore implements a **bounded response-body re-chunker**:

- the handler resolves the effective value (location → host/http/server → 8 KiB default,
  clamped to `2^24 - 1`) and attaches it to the response as an internal extension;
- an outermost data-plane layer (`data_plane/http2_chunk.rs`) wraps the fully finalized
  body **after** `CompressionLayer` and after every other body-producing layer, and splits
  each body chunk into at most the configured size with `Bytes::split_off` (zero-copy,
  byte-identical reassembly; trailers pass through);
- the wrapper runs only for HTTP/2 requests — nginx's directive only affects the v2 filter,
  so HTTP/1 framing is unchanged;
- chunks at or below the boundary pass through untouched.

The observable contract is therefore "every DATA frame ≤ configured chunk and the
reassembled entity body is byte-identical", not "the runtime controls h2 internally".

### Evidence

- Gateway wire test:
  `tests/http2_tuning.rs::http2_data_frames_respect_the_configured_chunk_size`. It serves a
  64 KiB static body over a real TLS/ALPN h2 connection through a recording client stream,
  parses the raw server frames, and asserts:
  - every DATA frame ≤ 1024 bytes on a route configured with `http2ChunkSize: 1024`;
  - the 1 KiB boundary is actually reached;
  - the default route's frames are ≤ 8192, include frames larger than 1 KiB, and use fewer
    frames than the 1 KiB route;
  - both reassembled bodies equal the 64 KiB fixture byte for byte.
- Mapping/typed tests: `http2_tuning_diagnostics_are_truthful` (http → server → location
  inheritance, 16 MiB clamp, zero/invalid rejection) and
  `typed_http2_chunk_size_and_random_index_materialize` (typed `server.toml` and JSON model).
- Real-nginx config-acceptance probe: `tools/check-nginx-http2-chunk-acceptance.mjs` runs
  `nginx -t` against a configuration that declares the directive at http, server and
  location and then against `http2_chunk_size 0;`; it requires nginx to accept the former
  and reject the latter with *"chunk size cannot be zero"*. The probe is invoked by
  `tools/run-nginx-differential-http.mjs`; it needs an nginx built with
  `--with-http_v2_module` and skips loudly (exit 3) when the host has none.
- **No HTTP/1 differential case was fabricated.** The differential-http harness speaks
  HTTP/1 and cannot observe an h2 DATA-frame boundary; the frame-capture test plus the
  config-acceptance probe are the honest evidence.

## 2. `random_index on|off`

### What changed

- nginx allows `random_index` **only in `location {}`** (inventory context:
  `http-location`); the materializer now accepts it there and continues to reject it at
  http/server context, exactly like nginx.
- `routes[].randomIndex` is the typed equivalent (`server.toml` location `randomIndex`).
- Directory handling: for a trailing-slash directory request the runtime first collects a
  **bounded, sorted** candidate set of non-hidden regular files (4096-entry scan cap,
  `403` above the cap), then selects one candidate. An empty candidate set falls through to
  the ordinary `index`/`autoindex` behavior, like nginx's `NGX_DECLINED`.
- Selection is a **deterministic seeded SplitMix64**: the seed is derived from the
  root-relative directory path (FNV-1a + SplitMix64) and mixed with a per-process atomic
  request counter. It is reproducible (and therefore testable) but distributes successive
  requests across candidates; a unit test proves every 64× candidate-count window covers
  every candidate.
- Unreadable/partially-unreadable directories log a path-located `tracing::warn!` and map
  the error to the existing nginx status semantics (403/500); the autoindex bounded-scan
  policy is reused.
- **Symlinked files are skipped**, even when they resolve to a regular file, because every
  other static delivery path refuses symlinks; nginx would follow the link. This is a
  documented security-posture difference, not a candidate-set difference for regular files.
- If the selected file disappears between the scan and the open, the runtime returns 404
  (nginx's internal redirect into the static handler would do the same).

### Evidence

- `tests/nginx/differential-http` adds a real-nginx candidate-set case
  (`random-index.candidates`): 192 requests against a directory containing three visible
  regular files, one hidden name and one subdirectory. The battery records the set of
  `{status, body hash, content type, ETag}` candidates and `compare.py` compares the set
  exactly — nginx's random order is not compared, only the set of possible bodies.
- Two exact cases cover the rest of the nginx contract: `random-index.empty` (no regular
  file → 403 with `autoindex off`) and `random-index.off` (undeclared location → 403).
  The oracle was regenerated from nginx 1.29.6 and the case floor moved from 364 to 367.
- Unit tests in `data_plane/static_path.rs` cover visible-file-only selection, seeded
  coverage, empty-directory fall-through and symlink refusal; the mapping test covers the
  location-only context and inheritance.

### Honest differences

- nginx uses `ngx_random()` over the filesystem's readdir order; the runtime uses a
  deterministic seeded selection over a sorted set. Individual choices are not comparable
  (and are not compared); the candidate set is.
- Symlinked candidates are excluded by the runtime (see above).

## 3. Gates run

- `cargo test -p sdkwork-webserver-core --lib materializes_` / targeted mapping tests
- `cargo test -p sdkwork-api-webserver-standalone-gateway --lib data_plane::static_path`
  and `--lib data_plane::http2_chunk`
- `cargo test -p sdkwork-api-webserver-standalone-gateway --test http2_tuning`
- `node tools/run-nginx-differential-http.mjs` (367/367 cases vs real nginx 1.29.6,
  including the random_index candidate-set case)
- `node tools/check-nginx-http2-chunk-acceptance.mjs` (real v2-enabled nginx config
  acceptance; pass)
- `node tools/check-nginx-directive-matrix.mjs` (catalog/source/matrix/evidence drift)
- `node tools/check-nginx-gap.mjs`,
  `node tools/check-nginx-behavioral-corpus.mjs`,
  `node tools/gen-nginx-directive-audit.mjs --check --verify-evidence`
- The aggregate gate `node tools/run-nginx-compat-gate.mjs` remains the release entrypoint.
