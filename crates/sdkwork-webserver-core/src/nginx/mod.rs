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
//! runtime cannot enforce — `sub_filter`, `limit_conn`, variable `proxy_pass`
//! — fail closed with a precise diagnostic.

mod load;
mod mapping;
mod merge;
mod parser;

pub use load::{load_nginx_compat, NginxLoadReport};
pub use mapping::{materialize_nginx_app, NginxConfigError};
pub use merge::merge_nginx_apps;
pub use parser::{expand_includes, parse_nginx_config, NginxDirective, NginxParseError};

pub(crate) use mapping::redirect_variables_ok;
