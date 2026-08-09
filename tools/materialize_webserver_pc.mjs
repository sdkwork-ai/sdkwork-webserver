import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const appRoot = resolve(repositoryRoot, "apps/sdkwork-webserver-pc");

const packages = [
  { id: "core", surface: "pc", capability: "runtime-core", deps: {}, coreComposition: true },
  { id: "commons", surface: "pc", capability: "shared-ui", deps: { "@sdkwork/iam-contracts": "workspace:*", fflate: "^0.8.2", ignore: "^7.0.6", react: "catalog:", "react-router-dom": "^7.15.0", "lucide-react": "catalog:" }, canonicalSpecs: frontendCanonicalSpecs("PC package and component naming."), layerRole: "frontend-core", publicExports: ["."], providedPorts: [{ name: "webserverWorkspace", export: "." }, { name: "webserverResourceContracts", export: "." }, { name: "webserverWorkspaceI18n", export: "." }, { name: "applicationSourceStorage", export: "." }], requiredPorts: [], dependencyApiExports: [], dependencyApiSurfaces: [], permissionComposition: false, dependencyPolicy: "Console and admin shells consume the shared workspace, navigation, i18n, and resource contracts through the package root export.", sdkPolicy: "This package owns no SDK client; resource services remain injected by console-core or admin-core.", readme: "This package owns shared resource contracts and the reusable PC workspace chrome for console and backend-admin surfaces. Shell packages provide navigation and SDK-backed resource registries through typed props; this package does not construct SDK clients or own runtime configuration." },
  { id: "console-core", surface: "app-console", capability: "console-core", deps: { "@sdkwork/drive-app-sdk": "workspace:*", "@sdkwork/sdk-common": "workspace:*", "@sdkwork/web-app-sdk": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", react: "catalog:" }, sdk: "sdkwork-web-app-sdk", sdkPackage: "@sdkwork/web-app-sdk", sdkAuthority: "sdkwork-web.app", sdkClients: ["SdkworkAppClient", "SdkworkDriveAppClient"], sdkDependencies: [{ workspace: "sdkwork-web-app-sdk", permissionModuleId: "web", surface: "app-api", credentialMode: "authenticated-app-api" }, { workspace: "sdkwork-drive-app-sdk", permissionModuleId: "drive", surface: "app-api", credentialMode: "authenticated-app-api" }], providedPorts: [{ name: "applicationSourceStorageAdapter", export: "." }], coreComposition: true },
  { id: "console-shell", surface: "app-console", capability: "console-shell", deps: { "@sdkwork/webserver-pc-commons": "workspace:*", react: "catalog:" }, canonicalSpecs: frontendCanonicalSpecs("Console package naming."), layerRole: "frontend-feature", publicExports: ["."], providedPorts: [{ name: "webserverConsoleShell", export: "." }], requiredPorts: [{ name: "webserverWorkspace", export: ".", provider: "@sdkwork/webserver-pc-commons" }, { name: "portalNavigation", export: "." }, { name: "notificationCenterNavigation", export: "." }], dependencyApiExports: [], dependencyApiSurfaces: [], permissionComposition: false, dependencyPolicy: "The application root injects Portal and Messaging notification-center navigation while the shell consumes the shared workspace through its public root export.", sdkPolicy: "The shell owns no SDK client; app SDK access remains isolated behind console-core.", readme: "This package owns the app-console shell boundary. The application root injects a required Portal navigation target, an optional Messaging notification-center target, authenticated viewer context, and the console resource registry. Feature packages remain unaware of Portal, Messaging, and shell chrome." },
  { id: "console-sites", surface: "app-console", capability: "sites", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["applications", "Applications", "Application lifecycle and availability", "web.applications.read"]] },
  { id: "console-site-configuration", surface: "app-console", capability: "site-configuration", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["configuration", "Configuration", "Environment variables and health checks", "web.applications.write"]] },
  { id: "console-delivery", surface: "app-console", capability: "delivery", deps: { "@sdkwork/deployments-pc-commons": "workspace:*", "@sdkwork/deployments-pc-console-core": "workspace:*", "@sdkwork/deployments-pc-console-delivery": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:" }, module: [["domains", "Domains", "Domain ownership and routing", "web.applications.write"], ["certificates", "Certificates", "TLS certificate lifecycle", "web.certificates.read"]], extraIndexExports: ['export * from "./DeployDomainManagementSurface.tsx";'] },
  { id: "console-skills", surface: "app-console", capability: "skills", deps: { "@sdkwork/skills-pc-core": "workspace:*", "@sdkwork/skills-pc-console-skills": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["skills", "My Skills", "Skill packages owned by the authenticated user", "skills.marketplace.read"]], extraIndexExports: ['export * from "./SkillsConsoleSurface.tsx";'] },
  { id: "console-mcp", surface: "app-console", capability: "mcp", deps: { "@sdkwork/mcp-pc-core": "workspace:*", "@sdkwork/mcp-pc-console-mcp": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["mcp", "My MCP Servers", "MCP servers registered by the authenticated user", "mcp.marketplace.read"]], extraIndexExports: ['export * from "./McpConsoleSurface.tsx";'] },
  { id: "console-deployments", surface: "app-console", capability: "deployments", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["source-versions", "Source versions", "Immutable Drive-backed application source versions", "web.applications.write"], ["deployments", "Deployments", "Release history and rollback", "web.applications.write"]] },
  { id: "admin-core", surface: "backend-admin", capability: "admin-core", deps: { "@sdkwork/web-backend-sdk": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:" }, sdk: "sdkwork-web-backend-sdk", sdkPackage: "@sdkwork/web-backend-sdk", sdkAuthority: "sdkwork-web.backend", coreComposition: true },
  { id: "admin-shell", surface: "backend-admin", capability: "admin-shell", deps: { "@sdkwork/webserver-pc-commons": "workspace:*", react: "catalog:" } },
  { id: "admin-applications", surface: "backend-admin", capability: "applications", deps: { "@sdkwork/webserver-pc-admin-core": "workspace:*", "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["applications", "Applications", "Deploy WEB and API applications", "web.sites.read"], ["application-source-versions", "Application source versions", "Immutable Drive-backed application source versions", "web.sites.read"], ["application-deployments", "Application deployments", "Application deployment history", "web.sites.read"]], dataSource: "./data-source.ts", requiredPorts: [{ name: "applicationSourceStorage", export: ".", provider: "@sdkwork/webserver-pc-commons" }] },
  { id: "admin-nginx", surface: "backend-admin", capability: "nginx", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["nginx", "Nginx", "Validate, deploy and reload Nginx configuration", "web.nginx.write"]] },
  { id: "admin-skills", surface: "backend-admin", capability: "skills", deps: { "@sdkwork/skills-pc-core": "workspace:*", "@sdkwork/skills-pc-admin-core": "workspace:*", "@sdkwork/skills-pc-admin-skill": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["skills", "Skills Admin", "Manage skill packages, categories, and capabilities", "skills.packages.manage"]], extraIndexExports: ['export * from "./SkillsAdminSurface.tsx";'] },
  { id: "admin-mcp", surface: "backend-admin", capability: "mcp", deps: { "@sdkwork/mcp-pc-core": "workspace:*", "@sdkwork/mcp-pc-admin": "workspace:*", "@sdkwork/sdk-common": "workspace:*", react: "catalog:", "react-router-dom": "^7.15.0" }, module: [["mcp", "MCP Admin", "Manage MCP servers, categories, and invocations", "mcp.admin.server.manage"]], extraIndexExports: ['export * from "./McpAdminSurface.tsx";'] },
  { id: "admin-servers", surface: "backend-admin", capability: "servers", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["servers", "Servers", "Managed Web Server inventory", "web.servers.read"]] },
  { id: "admin-diagnostics", surface: "backend-admin", capability: "diagnostics", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["diagnostics", "Diagnostics", "Runtime status and convergence diagnostics", "web.servers.read"]] },
  { id: "admin-audit", surface: "backend-admin", capability: "audit", deps: { "@sdkwork/webserver-pc-commons": "workspace:*" }, module: [["audit", "Audit", "Operator action evidence", "web.auditLogs.read"]] },
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
  const sdkDependencies = definition.sdkDependencies ?? (definition.sdk ? [{ workspace: definition.sdk, permissionModuleId: "web", surface: definition.surface === "backend-admin" ? "backend-api" : "app-api", credentialMode: definition.surface === "backend-admin" ? "authenticated-backend-admin" : "authenticated-app-api" }] : []);
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
      root: `sdkwork-web-server/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-${definition.id}`,
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
  const moduleCatalogRefs = [{ moduleId: "web", manifestRef: "../../../../specs/iam.module.manifest.json", inheritPermissions: true, inheritRoles: true }];
  if (definition.sdkDependencies?.some((dependency) => dependency.permissionModuleId === "drive")) {
    moduleCatalogRefs.push({ moduleId: "drive", manifestRef: "../../../../../sdkwork-iam/iam/modules/drive/iam.module.manifest.json", inheritPermissions: true, inheritRoles: true });
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
  const entries = definition.module.map(([resource, label, description, permission], index) => `    { resource: "${resource}", label: "${label}", description: "${description}", permission: "${permission}", order: ${index + 1} }`).join(",\n");
  return `import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";\n\nexport const webserverModule = {\n  id: "${definition.capability}",\n  label: "${definition.capability.replaceAll("-", " ")}",\n  surface: "${definition.surface}",\n  entries: [\n${entries}\n  ],\n} as const satisfies WebserverPcModuleDefinition;\n`;
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
