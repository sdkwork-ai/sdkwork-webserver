# SDKWork Web Server Internal API Route Specs

`component.spec.json` declares the authenticated runtime-set distribution router owned by
`sdkwork-webserver-internal-api`. The route crate consumes the Web internal service port directly and
does not depend on its own generated SDK.

Generated `src/http_route_manifest.rs` is materialized from the internal OpenAPI authority and
must not be edited by hand.
