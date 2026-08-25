//! Stock nginx configuration compatibility (SDKWORK_WEBSERVER_SPEC
//! `[compatibility]`): parse ordinary nginx `http` / `stream` configuration
//! (nginx.conf / sites-enabled `*.conf`) and materialize it into the runtime
//! `WebServerAppConfig` model so the data plane can serve without nginx.
//!
//! Supported surface (http-core-v1 + stream):
//! - `upstream { server …; keepalive …; ip_hash | least_conn | hash …; }`
//! - `server { listen …; server_name …; ssl_certificate(_key) …; location …; }`
//! - `location` `= /exact`, `^~ /prefix`, `/prefix`, `~` / `~*` regex with
//!   `proxy_pass`, `return`, `root` + `try_files`, `alias`, `rewrite`,
//!   `allow`/`deny`, `limit_req`, `auth_basic` + `auth_basic_user_file`
//! - http `limit_req_zone`, `gzip` / `gzip_types` / `gzip_min_length`
//! - http `proxy_cache_path` / `proxy_cache` / `proxy_cache_valid`
//! - `stream { upstream …; server { listen; proxy_pass; ssl_preread; … } }`
//! - `include` expansion inside the loaded directory / file chain
//!
//! Safe nginx tuning directives are accepted and ignored (the runtime owns
//! timeouts, buffering, header forwarding, and TLS defaults). Directives the
//! runtime cannot enforce — named locations, `try_files =code` fallbacks,
//! regex `server_name`, trailing-wildcard names, `return 444`, `unix:`
//! sockets — fail closed with a precise diagnostic.
//!
//! nginx configuration language details are tokenizer-exact with
//! `ngx_conf_read_token()` (quotes, escapes, comments, `${vars}`), and the
//! materialized model follows nginx semantics: `listen` defaults to port 80,
//! the first server on a socket is its default server (`default_server`
//! overrides), `proxy_pass` URI parts replace the matched prefix, `root`
//! appends the full request path while `alias` replaces the prefix, and
//! leading-wildcard `server_name` matches subdomains of any depth.

mod adaptive_web;
mod load;
mod mapping;
mod merge;
mod parser;

pub use adaptive_web::prefer_h5_surface;
pub use load::{load_nginx_compat, NginxLoadReport};
pub use mapping::{materialize_nginx_app, NginxConfigError};
pub use merge::merge_nginx_apps;
pub use parser::{expand_includes, parse_nginx_config, NginxDirective, NginxParseError};

pub(crate) use mapping::redirect_variables_ok;
