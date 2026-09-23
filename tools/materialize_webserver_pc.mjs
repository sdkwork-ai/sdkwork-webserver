import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const appRoot = resolve(repositoryRoot, "apps/sdkwork-webserver-pc");

// Dependency module catalogs inherited by reference (`COMPOSABLE_ARCHITECTURE_SPEC.md`
// section 7): a core package that consumes a dependency SDK must point at that
// dependency's own IAM module manifest instead of restating its permissions.
// `moduleId` is the identity the composition validator matches against the
// dependency's declared `moduleId`/`domain`, so it tracks the manifest, not the
// permission-code prefix.
const DEPENDENCY_MODULE_CATALOG_REFS = {
  deploy: {
    moduleId: "deployments",
    manifestRef: "../../../../../sdkwork-deployments/specs/iam.module.manifest.json",
  },
  drive: {
    moduleId: "drive",
    manifestRef: "../../../../../sdkwork-iam/iam/modules/drive/iam.module.manifest.json",
  },
  // IAM's own catalog lives in the kernel module and carries the `iam` domain, so
  // the identity the validator matches is the same one `iam.provider_accounts.*`
  // is prefixed with. (sdkwork-appstore references this exact manifest.)
  iam: {
    moduleId: "iam-kernel",
    manifestRef: "../../../../../sdkwork-iam/iam/modules/iam-kernel/iam.module.manifest.json",
  },
};

const packages = [
  { id: "core", surface: "pc", capability: "runtime-core", deps: {}, coreComposition: true },
  { id: "commons", surface: "pc", capability: "shared-ui", deps: { "@sdkwork/iam-contracts": "workspace:*", "@sdkwork/utils": "workspace:*", fflate: "^0.8.2", ignore: "^7.0.6", react: "catalog:", "react-router-dom": "^7.15.0", "lucide-react": "catalog:" }, canonicalSpecs: frontendCanonicalSpecs("PC package and component naming."), layerRole: "frontend-core", publicExports: ["."], providedPorts: [{ name: "webserverWorkspace", export: "." }, { name: "webserverResourceContracts", export: "." }, { name: "webserverWorkspaceI18n", export: "." }], requiredPorts: [], dependencyApiExports: [], dependencyApiSurfaces: [], permissionComposition: false, dependencyPolicy: "Console and admin shells consume the shared workspace, navigation, i18n, and resource contracts through the package root export.", sdkPolicy: "This package owns no SDK client; resource services remain injected by console-core or admin-core.", readme: "This package owns shared resource contracts and the reusable PC workspace chrome for console and backend-admin surfaces. Shell packages provide navigation and SDK-backed resource registries through typed props; this package does not construct SDK clients or own runtime configuration." },
  // Console SDK wiring only: the application lifecycle this package used to
  // own as a resource registry now lives in sdkwork-deployments (`deploy_app`)
  // and is bridged by console-delivery, so the shared commons dependency and
  // the `applicationSourceStorageAdapter` port went with it — and with that
  // consumer gone, commons no longer declares the matching
  // `applicationSourceStorage` providedPort either (a port nobody requires is
  // not a seam, and `check-component-port-bindings` only validates port shape,
  // so an orphaned port would have stayed green forever). The app SDK the
  // console constructs is deployments' own generated client as well — the
  // legacy webserver app-api surface has no console consumer left.
  //
  // The IAM cloud account center added a third pair. IAM's generated clients sit
  // on the capability-import denylist (`verify-repo` rejects a capability package
  // whose source imports `@sdkwork/iam-app-sdk` or `@sdkwork/iam-backend-sdk`), so
  // this core is the one place in the console allowed to compose them: the cloud
  // account page is handed the `SdkworkIamService` facade built here instead of a
  // transport. Only the app SDK is declared in `sdkDependencies` — the contract
  // check permits an app-console core to inherit app-api catalog entries and
  // rejects a `backend-api` one, and both faces belong to the same IAM module, so
  // the backend client needs no catalog entry of its own.
  { id: "console-core", surface: "app-console", capability: "console-core", deps: { "@sdkwork/deployments-app-sdk": "workspace:*", "@sdkwork/drive-app-sdk": "workspace:*", "@sdkwork/iam-app-sdk": "workspace:*", "@sdkwork/iam-backend-sdk": "workspace:*", "@sdkwork/iam-sdk-adapter": "workspace:*", "@sdkwork/iam-service": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:" }, sdk: "sdkwork-deployments-app-sdk", sdkPackage: "@sdkwork/deployments-app-sdk", sdkAuthority: "sdkwork-deploy-app-api", sdkClients: ["SdkworkDeployAppClient", "SdkworkDriveAppClient", "SdkworkAppClient", "SdkworkBackendClient"], sdkDependencies: [{ workspace: "sdkwork-deployments-app-sdk", permissionModuleId: "deploy", surface: "app-api", credentialMode: "authenticated-app-api" }, { workspace: "sdkwork-drive-app-sdk", permissionModuleId: "drive", surface: "app-api", credentialMode: "authenticated-app-api" }, { workspace: "sdkwork-iam-app-sdk", permissionModuleId: "iam", surface: "app-api", credentialMode: "authenticated-app-api" }], coreComposition: true },
  { id: "console-shell", surface: "app-console", capability: "console-shell", deps: { "@sdkwork/webserver-pc-commons": "workspace:*", react: "catalog:" }, canonicalSpecs: frontendCanonicalSpecs("Console package naming."), layerRole: "frontend-feature", publicExports: ["."], providedPorts: [{ name: "webserverConsoleShell", export: "." }], requiredPorts: [{ name: "webserverWorkspace", export: ".", provider: "@sdkwork/webserver-pc-commons" }, { name: "portalNavigation", export: "." }, { name: "notificationCenterNavigation", export: "." }], dependencyApiExports: [], dependencyApiSurfaces: [], permissionComposition: false, dependencyPolicy: "The application root injects Portal and Messaging notification-center navigation while the shell consumes the shared workspace through its public root export.", sdkPolicy: "The shell owns no SDK client; app SDK access remains isolated behind console-core.", readme: "This package owns the app-console shell boundary. The application root injects a required Portal navigation target, an optional Messaging notification-center target, authenticated viewer context, and the console resource registry. Feature packages remain unaware of Portal, Messaging, and shell chrome." },
  // Applications, domains, and certificates all render the canonical
  // sdkwork-deployments pages. `deploy_app`, `deploy_domain_zone`, and the
  // certificate entities are owned by that module and its console packages are
  // host-agnostic (clients arrive as props), so this host keeps only the menu
  // entries plus the SDK and authority wiring — there is no local
  // re-implementation of the application lifecycle here any more.
  //
  // These three entries are the *per-user* half of the domain story: an
  // authenticated operator creates and manages the domains and certificates they
  // own, and which rows those are is decided server-side by the Deployments
  // module against the shared IAM session rather than by a client-side filter.
  // The tenant-level half — the root domains and subdomains this edge actually
  // serves, reconciled from its own configuration at startup — is a different
  // plane and lives on the operations surface (`admin-delivery`, over
  // `webserver_root_domain` / `webserver_domain`). Same two menus, two ownership
  // levels; neither surface is a copy of the other.
  { id: "console-delivery", surface: "app-console", capability: "delivery", deps: { "@sdkwork/deployments-pc-commons": "workspace:*", "@sdkwork/deployments-pc-console-core": "workspace:*", "@sdkwork/deployments-pc-console-delivery": "workspace:*", "@sdkwork/deployments-pc-console-publishing": "workspace:*", "@sdkwork/sdk-common": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", react: "catalog:" }, module: [["apps", "Applications", "Publish and operate deploy_app applications", "deploy.apps.read"], ["domains", "Domains", "Your own domain ownership and routing", "deploy.domainZones.read"], ["certificates", "Certificates", "TLS certificates over the domains you own", "deploy.certificates.read"]], extraIndexExports: ['export * from "./DeployAppsManagementSurface.tsx";', 'export * from "./DeployDomainManagementSurface.tsx";'] },
  { id: "console-skills", surface: "app-console", capability: "skills", deps: { "@sdkwork/skills-pc-core": "workspace:*", "@sdkwork/skills-pc-console-skills": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["skills", "My Skills", "Skill packages owned by the authenticated user", "skills.marketplace.read"]], extraIndexExports: ['export * from "./SkillsConsoleSurface.tsx";'] },
  { id: "console-mcp", surface: "app-console", capability: "mcp", deps: { "@sdkwork/mcp-pc-core": "workspace:*", "@sdkwork/mcp-pc-console-mcp": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["mcp", "My MCP Servers", "MCP servers registered by the authenticated user", "mcp.marketplace.read"]], extraIndexExports: ['export * from "./McpConsoleSurface.tsx";'] },
  // The cloud account center hosts the IAM-owned provider account plane
  // (`iam_provider_account` plus `iam_provider_credential`) inside the Web Server
  // console. It is a thin host adapter: the page, controller, vocabulary, and
  // service port all come from the shared `sdkwork-iam-pc-console-cloud-account`
  // package, and the transport is composed by console-core, so the Web Server
  // keeps no second implementation of the account lifecycle.
  //
  // One menu entry covers all three ownership levels (personal, organization,
  // tenant): they share one server-side route set and the tabs only pin a filter,
  // so splitting them would claim a separation the API does not have.
  {
    id: "console-cloud-account",
    surface: "app-console",
    capability: "cloud-account",
    deps: {
      "@sdkwork/iam-contracts": "workspace:*",
      "@sdkwork/iam-pc-console-cloud-account": "workspace:*",
      "@sdkwork/iam-service": "workspace:*",
      "@sdkwork/webserver-pc-console-core": "workspace:*",
      react: "catalog:",
    },
    dependencyPolicy: "Host adapter only. The page, controller, vocabulary, i18n, and list pagination are consumed from the shared @sdkwork/iam-pc-console-cloud-account package, the permission predicates from @sdkwork/iam-contracts, and the SdkworkIamService facade from @sdkwork/webserver-pc-console-core; no page, form, or request shape is re-implemented here.",
    sdkPolicy: "This package owns no SDK client and imports no generated SDK. IAM's generated clients may only be composed by a core package, so the console-core built service facade arrives as a prop.",
    readme: "This package owns the cloud account capability on the app-console surface. It is a thin host adapter over the IAM-owned provider account plane: the page, its controller, the scope vocabulary, and its i18n all come from `@sdkwork/iam-pc-console-cloud-account`, and the `SdkworkIamService` facade it drives is composed by `@sdkwork/webserver-pc-console-core` from the IAM app and backend clients. The adapter contributes the menu entry, the resource key, and the `manageShared` rendering decision read off the session permission scope.",
    module: [["cloud-accounts", "Cloud Accounts", "Provider accounts owned by you, your organization, or the tenant", "iam.provider_accounts.read"]],
    extraIndexExports: ['export * from "./CloudAccountManagementSurface.tsx";'],
  },
  { id: "admin-core", surface: "backend-admin", capability: "admin-core", deps: { "@sdkwork/drive-app-sdk": "workspace:*", "@sdkwork/webserver-backend-sdk": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:" }, sdk: "sdkwork-webserver-backend-sdk", sdkPackage: "@sdkwork/webserver-backend-sdk", sdkAuthority: "sdkwork-webserver-backend-api", coreComposition: true },
  { id: "admin-shell", surface: "backend-admin", capability: "admin-shell", deps: { "@sdkwork/webserver-pc-commons": "workspace:*", react: "catalog:" } },
  // The backend-admin "Applications" entry renders the exact same canonical
  // `deploy_app` publishing page as the app-console one, and the bridge that
  // turns base URLs plus a token manager into that page's two clients is
  // surface-neutral — it has no admin/console divergence at all. Both copies
  // were introduced by the same commit and never diverged since, so this
  // package re-exports console-delivery's adapter instead of keeping a second
  // byte-identical copy. Only the menu entry (the part that really is
  // surface-specific) stays local. `frontend-composition` forbids a commons/
  // core package from depending on a capability package, and this direction
  // mirrors the existing `admin-plugins` -> `console-plugins` precedent.
  { id: "admin-apps", surface: "backend-admin", capability: "apps", deps: { "@sdkwork/webserver-pc-commons": "workspace:*", "@sdkwork/webserver-pc-console-delivery": "workspace:*" }, module: [["apps", "Applications", "Publish and operate deploy_app applications", "deploy.apps.read"]], extraIndexExports: ['export { DeployAppsManagementSurface as DeployAppsAdminSurface } from "@sdkwork/webserver-pc-console-delivery";'] },
  // Domains and Certificates here are the *tenant-level* half of the domain
  // story: the hostnames this edge actually answers for (`webserver_root_domain`
  // / `webserver_domain`, reconciled from the effective sidecar and module
  // imports at startup) and the TLS certificates that cover them
  // (`webserver_certificate`). Both planes are tenant-wide by construction — the
  // root table has no `user_id` column at all and the reconciled subdomains carry
  // `user_id IS NULL` — which is exactly what makes the operations surface the
  // place the shared edge inventory is read and managed from. The console keeps
  // its own Domains / Certificates entries (see `console-delivery` above), but
  // those are the *per-user* assets served by the Deployments plane: same two
  // menu labels, a different ownership level, and a different table. The pages
  // are authored here and read the injected admin SDK client off
  // `WebserverAdminSdkProvider`, so this package imports no generated SDK and
  // constructs no transport.
  { id: "admin-delivery", surface: "backend-admin", capability: "delivery", deps: { "@sdkwork/sdk-common": "workspace:*", "@sdkwork/ui-pc-react": "workspace:*", "@sdkwork/webserver-pc-admin-core": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", "lucide-react": "catalog:", react: "catalog:", "react-router-dom": "^7.15.0" }, sdkPolicy: "This package consumes the backend admin SDK exclusively through @sdkwork/webserver-pc-admin-core public exports (useWebserverAdminSdk and wire types); raw HTTP and direct generated-SDK imports are forbidden in authored UI code.", dependencyPolicy: "Host adapter only. The pages are authored here because the root-domain/subdomain relationship and the certificate lifecycle are Web Server-owned planes with no shared upstream implementation; the SDK client and its wire types are consumed from admin-core.", readme: "This package owns the Domains and Certificates capability on the backend-admin surface. Its two pages read the Web Server's own tenant-level infrastructure planes: the root domains and subdomains this edge serves (reconciled from its effective configuration at startup, so the inventory is not hand-maintained) and the TLS certificates covering them. The admin SDK client arrives through `WebserverAdminSdkProvider`, so the package composes no transport of its own.", module: [["domains", "Domains", "Root domains and subdomains this edge serves, reconciled from its configuration", "web.sites.read"], ["certificates", "Certificates", "TLS certificate lifecycle for the served hostnames", "web.certificates.read"]], extraIndexExports: ['export * from "./ServedDomainAdminSurface.tsx";', 'export * from "./ServedCertificateAdminSurface.tsx";'] },
  { id: "admin-nginx", surface: "backend-admin", capability: "nginx", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["nginx", "Nginx", "Validate, deploy and reload Nginx configuration", "web.nginx.write"]] },
  { id: "admin-skills", surface: "backend-admin", capability: "skills", deps: { "@sdkwork/skills-pc-core": "workspace:*", "@sdkwork/skills-pc-admin-core": "workspace:*", "@sdkwork/skills-pc-admin-skill": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["skills", "Skills Admin", "Manage skill packages, categories, and capabilities", "skills.packages.manage"]], extraIndexExports: ['export * from "./SkillsAdminSurface.tsx";'] },
  { id: "admin-mcp", surface: "backend-admin", capability: "mcp", deps: { "@sdkwork/mcp-pc-core": "workspace:*", "@sdkwork/mcp-pc-admin": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["mcp", "MCP Admin", "Manage MCP servers, categories, and invocations", "mcp.admin.server.manage"]], extraIndexExports: ['export * from "./McpAdminSurface.tsx";'] },
  { id: "admin-servers", surface: "backend-admin", capability: "servers", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["servers", "Servers", "Managed Web Server inventory", "web.servers.read"]] },
  { id: "admin-cluster", surface: "backend-admin", capability: "cluster", deps: { "@sdkwork/webserver-pc-admin-core": "workspace:*", "@sdkwork/sdk-common": "workspace:*", "@sdkwork/ui-pc-react": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", "lucide-react": "catalog:", react: "catalog:", "react-router-dom": "^7.15.0" }, sdkPolicy: "This package consumes the backend admin SDK exclusively through @sdkwork/webserver-pc-admin-core public exports (createWebserverAdminSdkClient and wire types); raw HTTP and direct generated-SDK imports are forbidden in authored UI code.", module: [["cluster-overview", "Cluster", "Distributed cluster health and liveness overview", "web.cluster.read", "cluster/overview"], ["cluster-clusters", "Clusters", "Cluster grouping and heartbeat thresholds", "web.cluster.read", "cluster/clusters"], ["cluster-hosts", "Cluster Hosts", "Host machines with system and network identity", "web.cluster.read", "cluster/hosts"], ["cluster-instances", "Cluster Instances", "Webserver process instances and liveness", "web.cluster.read", "cluster/instances"], ["cluster-events", "Cluster Events", "Cluster lifecycle event evidence", "web.cluster.read", "cluster/events"]], moduleNote: [
      "Every entry sits under the module's own `/admin/cluster` prefix, exactly",
      "like Storage Center's `/admin/storage/<child>`. The overview deliberately",
      "does **not** claim `path: \"cluster\"` (the bare prefix): that path is the",
      "`clusterCenter` tab's landing route, and an entry owning it would make",
      "`/admin/cluster` resolve to a page while the tab treats it as its own",
      "root — two different \"cluster\" URLs for the same operator.",
    ], extraIndexExports: ['export * from "./ClusterOverviewSurface.tsx";'] },
  { id: "admin-servers-explorer", surface: "backend-admin", capability: "servers-explorer", deps: { "@sdkwork/webserver-pc-admin-core": "workspace:*", "@sdkwork/sdk-common": "workspace:*", "@sdkwork/ui-pc-react": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", "lucide-react": "catalog:", react: "catalog:", "react-router-dom": "^7.15.0" }, sdkPolicy: "This package consumes the backend admin SDK exclusively through @sdkwork/webserver-pc-admin-core public exports (createWebserverAdminSdkClient and wire types); raw HTTP and direct generated-SDK imports are forbidden in authored UI code.", module: [["servers-explorer", "Server Files", "Browse, classify, and operate server deployment projects and files", "web.servers.files.read"]], extraIndexExports: ['export * from "./ServerFilesExplorerSurface.tsx";', 'export * from "./server-files-client.ts";', 'export * from "./project-detection.ts";'] },
  { id: "admin-webserver-config", surface: "backend-admin", capability: "webserver-config", deps: { "@sdkwork/webserver-pc-admin-core": "workspace:*", "@sdkwork/sdk-common": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", "@monaco-editor/react": "catalog:", "monaco-editor": "catalog:", "lucide-react": "catalog:", react: "catalog:", "react-router-dom": "^7.15.0" }, sdkPolicy: "This package consumes the backend admin SDK exclusively through @sdkwork/webserver-pc-admin-core public exports (createWebserverAdminSdkClient and wire types); raw HTTP and direct generated-SDK imports are forbidden in authored UI code.", module: [["webserver-config", "Server Config", "Edit the deployed default config, import plane, and module sidecar configuration online", "web.servers.files.read"]], extraIndexExports: ['export * from "./WebserverConfigSurface.tsx";', 'export * from "./webserver-config-client.ts";', 'export * from "./config-language.ts";'] },
  { id: "admin-diagnostics", surface: "backend-admin", capability: "diagnostics", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["diagnostics", "Diagnostics", "Runtime status and convergence diagnostics", "web.servers.read"]] },
  { id: "admin-audit", surface: "backend-admin", capability: "audit", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["audit", "Audit", "Operator action evidence", "web.auditLogs.read"]] },
  // Storage Center hosts the drive-owned admin storage plane (providers, kinds,
  // buckets, bindings) inside the Web Server edge. The surface is a thin host
  // adapter: the pages, service, types, and i18n all come from the shared
  // `sdkwork-drive-pc-admin-storage-providers` package that cloudrouter mounts
  // too, so the two applications reuse one module instead of forking it.
  {
    id: "admin-storage",
    surface: "backend-admin",
    capability: "storage",
    deps: {
      "@sdkwork/sdk-common": "workspace:*",
      "@sdkwork/webserver-pc-commons": "workspace:*",
      "sdkwork-drive-pc-admin-core": "workspace:*",
      "sdkwork-drive-pc-admin-storage-providers": "workspace:*",
      "sdkwork-drive-pc-commons": "workspace:*",
      "sdkwork-drive-pc-core": "workspace:*",
      react: "catalog:",
    },
    sdkPolicy: "The drive-owned admin storage SDK is composed through createDriveAdminStorageHostClient from sdkwork-drive-pc-admin-core, which accepts the host API base URL directly; raw HTTP and direct generated-SDK imports are forbidden in authored UI code.",
    dependencyPolicy: "Host adapter only. Pages, components, service, types, and i18n are consumed from the shared sdkwork-drive-pc-admin-storage-providers package, and the drive session snapshot is adapted from host session primitives at the surface boundary; no drive page is copied or re-implemented here.",
    readme: "This package owns the storage capability on the backend-admin surface. It is a thin host adapter over the drive-owned admin storage plane: the Storage Providers / Provider Catalog / Buckets / Bindings pages, their service, types, and i18n all come from `sdkwork-drive-pc-admin-storage-providers` + `sdkwork-drive-pc-commons`, which cloudrouter consumes as well. The adapter only composes the shared admin storage SDK client (`createDriveAdminStorageHostClient`), supplies the host API base URL, and adapts the host session into the drive session snapshot the shared pages expect.",
    module: [
      ["storage-providers", "Storage Providers", "Configure and manage the object storage backends Drive writes to", "drive.storage.admin", "storage/providers"],
      ["storage-kinds", "Provider Catalog", "Enable or disable the storage provider kinds operators may choose", "drive.storage.admin", "storage/kinds"],
      ["storage-buckets", "Buckets", "Inspect and create the bucket behind each storage provider", "drive.storage.admin", "storage/buckets"],
      ["storage-bindings", "Bindings", "Route each space type to a default storage provider", "drive.storage.admin", "storage/bindings"],
    ],
    extraIndexExports: ['export * from "./StorageCenterSurface.tsx";'],
  },
  // The platform admin needs the same cloud account center the tenant console
  // mounts, because publishing the global (`platform`) default is precisely the
  // operation no tenant console may reach. The resource, the route set, and the
  // page implementation are one thing, so this package is an alias over the
  // console adapter — the same shape `admin-apps` -> `console-delivery` and
  // `admin-plugins` -> `console-plugins` already use — and only the menu entry
  // and the `surface` marker are surface-specific. What separates the two surfaces
  // is the ownership levels the host projects, which the shared contract helper
  // derives from the session; this package never re-decides it.
  {
    id: "admin-cloud-account",
    surface: "backend-admin",
    capability: "cloud-account",
    deps: {
      "@sdkwork/webserver-pc-commons": "workspace:*",
      "@sdkwork/webserver-pc-console-cloud-account": "workspace:*",
      react: "catalog:",
    },
    dependencyPolicy: "Alias only. The page, controller, scope vocabulary, i18n, and the level projection are consumed from @sdkwork/webserver-pc-console-cloud-account, which in turn consumes them from the IAM-owned @sdkwork/iam-pc-console-cloud-account; nothing is re-implemented here.",
    sdkPolicy: "This package owns no SDK client and imports no generated SDK. The IAM service facade arrives through the console SDK provider that already wraps the admin route tree.",
    readme: "This package owns the cloud account capability on the backend-admin surface. It renders the same IAM-owned provider account page the tenant console does — one resource, one route set — and adds only the admin menu entry and the `admin` surface marker that selects the admin descriptive copy. The offered ownership levels are not decided here: the shared contract helper projects them from the session, so the platform admin and the tenant console cannot drift apart in which levels they offer.",
    module: [["cloud-accounts", "Cloud Accounts", "Provider accounts across the personal, organization, tenant, and platform levels", "iam.provider_accounts.read"]],
    extraIndexExports: ['export * from "./CloudAccountAdminSurface.tsx";'],
  },
];

for (const definition of packages) {
  const directory = resolve(appRoot, "packages", `sdkwork-webserver-pc-${definition.id}`);
  mkdirSync(resolve(directory, "src"), { recursive: true });
  mkdirSync(resolve(directory, "specs"), { recursive: true });
  writeJson(resolve(directory, "package.json"), packageManifest(definition));
  writeJson(resolve(directory, "specs/component.spec.json"), componentSpec(definition));
  writeFileSync(resolve(directory, "specs/README.md"), specsReadme(definition), "utf8");
  if (definition.module) {
    writeFileSync(resolve(directory, "src/module.ts"), moduleSource(definition), "utf8");
    writeFileSync(resolve(directory, "src/index.ts"), moduleIndexSource(definition), "utf8");
  }
  if (definition.coreComposition) {
    materializeCoreComposition(directory, definition);
  }
}

function moduleIndexSource(definition) {
  const exports = ["export { webserverModule } from \"./module.ts\";"];
  if (definition.dataSource) exports.push(`export * from "${definition.dataSource}";`);
  exports.push(...(definition.extraIndexExports ?? []));
  return `${exports.join("\n")}\n`;
}

function packageManifest(definition) {
  const packageExports = {
    ".": packageExport("./src/index.ts"),
  };
  if (definition.coreComposition) {
    packageExports["./sdk"] = packageExport("./src/sdk/index.ts");
    packageExports["./modules"] = packageExport("./src/modules/index.ts");
    packageExports["./host"] = packageExport("./src/host/index.ts");
    packageExports["./session"] = packageExport("./src/session/index.ts");
    packageExports["./composition"] = packageExport("./src/composition/index.ts");
  }
  return {
    name: `@sdkwork/webserver-pc-${definition.id}`,
    version: "0.1.0",
    private: true,
    type: "module",
    main: "./src/index.ts",
    exports: packageExports,
    dependencies: definition.deps,
    sdkwork: {
      applicationCode: "webserver",
      architecture: "pc-react",
      capability: definition.capability,
      surface: definition.surface,
      managedBy: "tools/materialize_webserver_pc.mjs",
    },
  };
}

function componentSpec(definition) {
  const sdkDependencies = effectiveSdkDependencies(definition);
  const publicExports = definition.publicExports ?? (definition.coreComposition
    ? [".", "./sdk", "./modules", "./host", "./session", "./composition"]
    : ["src/index.ts"]);
  return {
    schemaVersion: 1,
    kind: "sdkwork.component.spec",
    component: {
      name: `@sdkwork/webserver-pc-${definition.id}`,
      displayName: `SDKWork Webserver PC ${definition.capability}`,
      version: "0.1.0",
      type: "node-package",
      root: `sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-${definition.id}`,
      domain: "infrastructure",
      capability: definition.capability,
      surface: definition.surface,
      languages: ["typescript"],
      generated: false,
      private: true,
      status: "active",
      manifests: ["package.json", "specs/component.spec.json"],
    },
    canonicalSpecs: [
      ...(definition.canonicalSpecs ?? []),
      { file: "COMPONENT_SPEC.md", path: "../../../../../sdkwork-specs/COMPONENT_SPEC.md", purpose: "Component contract." },
      { file: "APP_PC_ARCHITECTURE_SPEC.md", path: "../../../../../sdkwork-specs/APP_PC_ARCHITECTURE_SPEC.md", purpose: "PC package and surface boundaries." },
      { file: "APP_PC_REACT_UI_SPEC.md", path: "../../../../../sdkwork-specs/APP_PC_REACT_UI_SPEC.md", purpose: "React PC implementation." },
      { file: "SDK_SPEC.md", path: "../../../../../sdkwork-specs/SDK_SPEC.md", purpose: "Generated SDK consumption." },
      { file: "TEST_SPEC.md", path: "../../../../../sdkwork-specs/TEST_SPEC.md", purpose: "Verification." },
    ],
    contracts: {
      ...(definition.layerRole ? { layerRole: definition.layerRole } : {}),
      publicExports,
      ...(definition.providedPorts ? { providedPorts: definition.providedPorts } : {}),
      ...(definition.requiredPorts ? { requiredPorts: definition.requiredPorts } : {}),
      runtimeEntrypoints: [],
      routeManifest: null,
      sdkClients: definition.sdkClients ?? [],
      sdkDependencies,
      ...(definition.dependencyApiExports ? { dependencyApiExports: definition.dependencyApiExports } : {}),
      ...(definition.dependencyApiSurfaces ? { dependencyApiSurfaces: definition.dependencyApiSurfaces } : {}),
      ...(definition.permissionComposition === false ? {} : { permissionComposition: permissionComposition(definition) }),
      events: [],
      configKeys: [],
    },
    integration: {
      authority: "Root SDKWork specs remain authoritative.",
      dependencyPolicy: definition.dependencyPolicy ?? "Consume sibling packages through public exports only.",
      sdkPolicy: definition.sdkPolicy ?? (definition.surface === "backend-admin" ? "Backend SDK access is isolated behind admin-core." : "App SDK access is isolated behind console-core."),
    },
    verification: { commands: ["pnpm --dir apps/sdkwork-webserver-pc typecheck", "pnpm --dir apps/sdkwork-webserver-pc test"] },
    metadata: { managedBy: "tools/materialize_webserver_pc.mjs", standardVersion: "2026-07-24" },
  };
}

function packageExport(path) {
  return { types: path, import: path, default: path };
}

/**
 * The SDK dependencies a core package actually declares. `componentSpec` and
 * `permissionComposition` both read this so the emitted dependency list and the
 * emitted catalog references can never disagree about which modules the package
 * inherits from.
 */
function effectiveSdkDependencies(definition) {
  if (definition.sdkDependencies) return definition.sdkDependencies;
  if (!definition.sdk) return [];
  const backendAdmin = definition.surface === "backend-admin";
  return [{
    workspace: definition.sdk,
    permissionModuleId: "web",
    surface: backendAdmin ? "backend-api" : "app-api",
    credentialMode: backendAdmin ? "authenticated-backend-admin" : "authenticated-app-api",
  }];
}

function permissionComposition(definition) {
  if (!definition.coreComposition) {
    return {
      inheritanceMode: "openapi-with-explicit-ui-hints",
      routePermissionHints: { inheritFromOpenApi: true, overrides: [] },
      consumerPolicy: { forbidLocalPermissionCatalogForDependencyDomains: true, allowFrontendHintsWithoutServerDuplication: true },
    };
  }
  if (!definition.sdk) {
    return {
      inheritanceMode: "module-catalog-with-overrides",
      moduleCatalogRefs: [],
      routePermissionHints: { inheritFromOpenApi: true, inheritFromModuleManifests: true, overrides: [] },
      consumerPolicy: { forbidLocalPermissionCatalogForDependencyDomains: true, allowExplicitOverridesOnly: true, allowFrontendHintsWithoutServerDuplication: true },
    };
  }
  // The host's own module catalog leads, then every dependency module inherits
  // its own catalog by reference — the host never restates a dependency's
  // permissions (`COMPOSABLE_ARCHITECTURE_SPEC.md` section 7).
  const moduleCatalogRefs = [{ moduleId: "web", manifestRef: "../../../../specs/iam.module.manifest.json", inheritPermissions: true, inheritRoles: true }];
  for (const dependency of effectiveSdkDependencies(definition)) {
    const catalogRef = DEPENDENCY_MODULE_CATALOG_REFS[dependency.permissionModuleId];
    if (catalogRef) {
      moduleCatalogRefs.push({ ...catalogRef, inheritPermissions: true, inheritRoles: true });
    }
  }
  return {
    inheritanceMode: "module-catalog-with-overrides",
    moduleCatalogRefs,
    bootstrapAccessTokenScope: { inheritFrom: "sdkwork.app.config.json#backend.accessTokenPermissionScope", supplement: [], overrideReplace: false },
    routePermissionHints: { inheritFromOpenApi: true, inheritFromModuleManifests: true, overrides: [] },
    consumerPolicy: { forbidLocalPermissionCatalogForDependencyDomains: true, allowExplicitOverridesOnly: true, allowFrontendHintsWithoutServerDuplication: true },
  };
}

function materializeCoreComposition(directory, definition) {
  for (const child of ["composition", "host", "modules", "sdk", "session"]) {
    mkdirSync(resolve(directory, "src", child), { recursive: true });
  }
  const emptyExport = "export {};\n";
  writeFileSync(resolve(directory, "src/host/index.ts"), emptyExport, "utf8");
  writeFileSync(resolve(directory, "src/modules/index.ts"), emptyExport, "utf8");
  writeFileSync(resolve(directory, "src/session/index.ts"), emptyExport, "utf8");
  writeFileSync(
    resolve(directory, "src/sdk/index.ts"),
    definition.sdk ? 'export * from "../index.tsx";\n' : emptyExport,
    "utf8",
  );
  writeFileSync(resolve(directory, "src/composition/dependency-manifest.ts"), 'export const webserverComponentSpecPath = "../../specs/component.spec.json" as const;\n', "utf8");
  writeFileSync(resolve(directory, "src/composition/sdk-inventory.ts"), sdkInventorySource(definition), "utf8");
  writeFileSync(resolve(directory, "src/composition/module-registry.ts"), "export function createWebserverCoreModuleRegistry() {\n  return {} as const;\n}\n", "utf8");
  writeFileSync(resolve(directory, "src/composition/host-registry.ts"), "export function createWebserverCoreHostRegistry() {\n  return {} as const;\n}\n", "utf8");
  writeFileSync(resolve(directory, "src/composition/index.ts"), [
    'export * from "./dependency-manifest.ts";',
    'export * from "./sdk-inventory.ts";',
    'export * from "./module-registry.ts";',
    'export * from "./host-registry.ts";',
    "",
  ].join("\n"), "utf8");
}

function sdkInventorySource(definition) {
  const inventory = [];
  if (definition.sdkPackage) {
    inventory.push({ packageName: definition.sdkPackage, authority: definition.sdkAuthority, surface: definition.surface === "backend-admin" ? "backend-api" : "app-api" });
  }
  if (definition.sdkDependencies?.some((dependency) => dependency.workspace === "sdkwork-drive-app-sdk")) {
    inventory.push({ packageName: "@sdkwork/drive-app-sdk", authority: "sdkwork-drive-app-api", surface: "app-api" });
  }
  const entries = inventory.map((item) => `    { packageName: "${item.packageName}", authority: "${item.authority}", surface: "${item.surface}" },`).join("\n");
  return `export function listWebserverCoreSdkInventory() {\n  return [\n${entries}\n  ] as const;\n}\n`;
}

function moduleSource(definition) {
  // `module` tuples are [resource, label, description, permission, path?].
  // `path` is the route segment under the surface base path; omit it when the
  // route segment equals the resource key. A module sub-path (`storage/providers`)
  // is what lets a module group entries under its own URL subtree while keeping
  // a stable, slash-free resource key for i18n and the data-source registry.
  //
  // `moduleNote` carries authored prose that has to survive regeneration: the
  // routing invariants a hand-edited `src/module.ts` accumulated (why cluster's
  // overview must not claim the bare `/admin/cluster` prefix, say). Without a
  // slot here that comment is dropped the next time this script runs, and the
  // invariant it documents regresses with it — which is exactly how the cluster
  // overview lost `path: "cluster/overview"` before.
  const noteLines = (definition.moduleNote ?? []).map((line) => `  // ${line}\n`).join("");
  const entries = definition.module.map(([resource, label, description, permission, path], index) => {
    const pathField = path ? `, path: "${path}"` : "";
    return `    { resource: "${resource}", label: "${label}", description: "${description}", permission: "${permission}", order: ${index + 1}${pathField} }`;
  }).join(",\n");
  return `import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";\n\nexport const webserverModule = {\n  id: "${definition.capability}",\n  label: "${definition.capability.replaceAll("-", " ")}",\n  surface: "${definition.surface}",\n${noteLines}  entries: [\n${entries}\n  ],\n} as const satisfies WebserverPcModuleDefinition;\n`;
}

function specsReadme(definition) {
  const description = definition.readme
    ?? `This package owns the ${definition.capability} capability on the ${definition.surface} surface. Its component contract links the canonical SDKWork standards; normative text is not duplicated locally.`;
  return `# ${definition.capability}\n\n${description}\n`;
}

function frontendCanonicalSpecs(namingPurpose) {
  return [
    { file: "CODE_STYLE_SPEC.md", path: "../../../../../sdkwork-specs/CODE_STYLE_SPEC.md", purpose: "Authored code and public export boundaries." },
    { file: "NAMING_SPEC.md", path: "../../../../../sdkwork-specs/NAMING_SPEC.md", purpose: namingPurpose },
    { file: "TYPESCRIPT_CODE_SPEC.md", path: "../../../../../sdkwork-specs/TYPESCRIPT_CODE_SPEC.md", purpose: "TypeScript package structure." },
    { file: "FRONTEND_CODE_SPEC.md", path: "../../../../../sdkwork-specs/FRONTEND_CODE_SPEC.md", purpose: "Frontend implementation and verification." },
  ];
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}
