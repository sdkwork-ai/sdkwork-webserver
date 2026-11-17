import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "delivery",
  label: "delivery",
  surface: "backend-admin",
  entries: [
    // The carrier ledger leads the module because it is the row the two menus
    // below hang off: a site carrier owns the domains and certificates this
    // module then serves. Its entry gate is the read code the Domains ledger
    // already uses; the page itself hides its write controls unless the caller
    // also holds `web.applications.write`, which is what the create/update/pause
    // operations require.
    { resource: "site-carriers", label: "Site Carriers", description: "Web Server carrier rows that pair one SDKWork Deploy application with a site, its domains, certificates, and reverse-proxy configuration", permission: "web.applications.read", order: 1 },
    { resource: "domains", label: "Domains", description: "Root domains and subdomains this edge serves, reconciled from its configuration", permission: "web.applications.read", order: 2 },
    { resource: "certificates", label: "Certificates", description: "TLS certificate lifecycle for the served hostnames", permission: "web.certificates.read", order: 3 }
  ],
} as const satisfies WebserverPcModuleDefinition;
