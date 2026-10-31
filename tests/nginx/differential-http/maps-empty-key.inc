# nginx `map` declarations whose key is the empty quoted string (`"" key value;`).
#
# nginx parses `map` block content with its own tokenizer, so the first token of
# an entry may be empty; it matches an absent or empty source value. This file is
# injected at the empty-key marker comment by
# `tools/run-nginx-differential-http.mjs` (gateway side) and `run-nginx.sh`
# (oracle side). It is deliberately not a `*.conf` file: the repository's JS
# configuration parser (used by the directive-matrix gate to scan committed
# fixtures) cannot tokenise an empty directive name, while nginx and the Rust
# importer can. Removing the marker instead would silently weaken the matrix.
map $http_x_tenant $tenant_zone {
    default             shared-zone;
    ""                  anonymous-zone;
    acme                acme-zone;
    ~*^beta-[0-9]+      beta-zone;
    ~^gamma-(?<n>[0-9]+)$  gamma-$1;
}

# `$arg_tag` is compared raw (nginx does not percent-decode `$arg_`), so
# `?tag=a%20b` matches the encoded key, not a literal space.
map $arg_tag $tag_value {
    default   no-tag;
    ""        empty-tag;
    "a%20b"   encoded-tag;
}
