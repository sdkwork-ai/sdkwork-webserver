# Real-world nginx fixture: api.birdcoder.com (production snapshot)

Source: `/etc/nginx` on `ubuntu@api.birdcoder.com` (`nginx/1.28.0`, Ubuntu package build,
`--with-mail --with-mail_ssl_module --with-http_v3_module --with-stream --with-stream_ssl_preread_module`,
`nginx -T` output captured 2026-10-30).

- 33 configuration files (`nginx.conf`, `conf.d/`, `sites-enabled/{sdkwork,noaper}/`,
  `upstream.conf.d/`, `tcp.conf.d/`, `mime.types`, `fastcgi_params`, `uwsgi_params`, `scgi_params`).
- Absolute `/etc/nginx/` include prefixes were rewritten to fixture-relative paths so the tree is
  self-contained; no secret material is present (certificate **paths** only, no keys).
- This fixture is the compatibility acceptance target for the rewrite/variable engine wave:
  `node tools/import-nginx-conf.mjs --conf tests/nginx/real-world/production-api-birdcoder/nginx.conf`
  must classify it without unsupported directives, and the Rust materializer must load it with the
  operator's document-root allowlist.
