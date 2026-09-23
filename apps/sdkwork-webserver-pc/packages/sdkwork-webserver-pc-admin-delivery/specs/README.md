# delivery

This package owns the Domains and Certificates capability on the backend-admin surface. Its two pages read the Web Server's own tenant-level infrastructure planes: the root domains and subdomains this edge serves (reconciled from its effective configuration at startup, so the inventory is not hand-maintained) and the TLS certificates covering them. The admin SDK client arrives through `WebserverAdminSdkProvider`, so the package composes no transport of its own.
