import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { parse as parseYaml } from "yaml";

const root = process.cwd();
const checkMode = process.argv.slice(2).includes("--check");
const drift = [];

const surfaces = [
  {
    yamlPath: "apis/app-api/web/openapi.yaml",
    jsonAuthorityPath: "apis/app-api/web/sdkwork-webserver-app-api.openapi.json",
    sdkJsonPath: "sdks/sdkwork-webserver-app-sdk/openapi/sdkwork-webserver-app-api.openapi.json",
    routeManifestPath:
      "sdks/_route-manifests/app-api/sdkwork-routes-webserver-app-api.route-manifest.json",
    crateDir: "crates/sdkwork-routes-webserver-app-api",
    manifestFn: "app_route_manifest",
    packageName: "sdkwork-routes-webserver-app-api",
    surface: "app-api",
    apiAuthority: "sdkwork-webserver-app-api",
    sdkFamily: "sdkwork-webserver-app-sdk",
    prefix: "/app/v3/api",
    domainTag: "web",
  },
  {
    yamlPath: "apis/backend-api/web/openapi.yaml",
    jsonAuthorityPath: "apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json",
    sdkJsonPath: "sdks/sdkwork-webserver-backend-sdk/openapi/sdkwork-webserver-backend-api.openapi.json",
    routeManifestPath:
      "sdks/_route-manifests/backend-api/sdkwork-routes-webserver-backend-api.route-manifest.json",
    crateDir: "crates/sdkwork-routes-webserver-backend-api",
    manifestFn: "backend_route_manifest",
    packageName: "sdkwork-routes-webserver-backend-api",
    surface: "backend-api",
    apiAuthority: "sdkwork-webserver-backend-api",
    sdkFamily: "sdkwork-webserver-backend-sdk",
    prefix: "/backend/v3/api",
    domainTag: "web",
  },
  {
    yamlPath: "apis/internal-api/web/sdkwork-webserver-internal-api.openapi.yaml",
    jsonAuthorityPath:
      "apis/internal-api/web/sdkwork-webserver-internal-api.openapi.json",
    sdkJsonPath:
      "sdks/sdkwork-webserver-internal-sdk/openapi/sdkwork-webserver-internal-api.openapi.json",
    routeManifestPath:
      "sdks/_route-manifests/internal-api/sdkwork-routes-webserver-internal-api.route-manifest.json",
    crateDir: "crates/sdkwork-routes-webserver-internal-api",
    manifestFn: "internal_route_manifest",
    packageName: "sdkwork-routes-webserver-internal-api",
    surface: "internal-api",
    apiAuthority: "sdkwork-webserver-internal-api",
    sdkFamily: "sdkwork-webserver-internal-sdk",
    prefix: "/internal/v3/api",
    domainTag: "web",
    languages: ["typescript", "rust"],
    generatorType: "custom",
  },
];

function writeText(relativePath, content) {
  const target = path.join(root, relativePath);
  const desired = content.replace(/\r\n/g, "\n");
  if (checkMode) {
    const current = fs.existsSync(target)
      ? fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n")
      : null;
    if (current !== desired) drift.push(relativePath);
    return;
  }
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, desired, "utf8");
  console.log(`wrote ${relativePath}`);
}

function writeJson(relativePath, value) {
  writeText(relativePath, `${JSON.stringify(value, null, 2)}\n`);
}

function formatRustSource(relativePath, content) {
  const result = spawnSync(
    "rustfmt",
    ["--edition", "2021"],
    { encoding: "utf8", input: content },
  );
  if (result.status !== 0) {
    throw new Error(
      `rustfmt failed for ${relativePath}: ${result.stderr || result.stdout || "unknown error"}`,
    );
  }
  return result.stdout;
}

function enrichOpenApi(openapi, profile) {
  const enriched = structuredClone(openapi);
  if (profile.surface === "internal-api") {
    const runtimeSetSchema = JSON.parse(
      fs.readFileSync(
        path.join(
          root,
          "specs/sdkwork.website-runtime-set.snapshot.schema.json",
        ),
        "utf8",
      ),
    );
    enriched.components.schemas.WebsiteRuntimeSetSnapshot =
      rewriteRuntimeSetSchemaReferences(runtimeSetSchema);
  }
  const componentResponses = enriched.components?.responses ?? {};
  for (const [pathKey, pathItem] of Object.entries(enriched.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!["get", "post", "put", "patch", "delete"].includes(method)) {
        continue;
      }
      if (operation["x-sdkwork-owner"] !== SDK_OWNER) {
        throw new Error(
          `${profile.yamlPath} ${method.toUpperCase()} ${pathKey} must declare x-sdkwork-owner: ${SDK_OWNER}`,
        );
      }
      operation["x-sdkwork-api-surface"] = profile.surface;
      operation["x-sdkwork-api-authority"] = profile.apiAuthority;
      operation["x-sdkwork-request-context"] = "WebRequestContext";
      operation["x-sdkwork-auth-mode"] =
        operation["x-sdkwork-auth-mode"] ??
        (profile.surface === "internal-api" ? "ingress-token" : "dual-token");
      // Derive x-sdkwork-route-auth from auth-mode when not explicitly declared (C8-C9).
      if (!operation["x-sdkwork-route-auth"]) {
        operation["x-sdkwork-route-auth"] = operation["x-sdkwork-auth-mode"];
      }
      if (operation["x-sdkwork-permission"] === false) {
        // Explicit no-permission marker: authentication still follows
        // x-sdkwork-auth-mode, but no specific permission is required.
        delete operation["x-sdkwork-permission"];
      } else if (!operation["x-sdkwork-permission"] && operation.operationId) {
        const [resource, action] = operation.operationId.split(".");
        const verb = action?.includes("list") || action?.includes("retrieve")
          ? "read"
          : "write";
        operation["x-sdkwork-permission"] = `web.${resource}.${verb}`;
      }
      validateIdempotencyContract(pathKey, method, pathItem, operation, enriched);
      // C4-C7: expand $ref responses to inline definitions for sdkgen compatibility.
      // sdkgen requires error responses (4xx/5xx) to include application/problem+json
      // content type, but cannot resolve $ref to #/components/responses.
      if (operation.responses) {
        for (const [code, response] of Object.entries(operation.responses)) {
          if (response?.$ref?.startsWith("#/components/responses/")) {
            const responseName = response.$ref.split("/").pop();
            const expanded = componentResponses[responseName];
            if (expanded) {
              operation.responses[code] = structuredClone(expanded);
            }
          }
        }
      }
      // Preserve the authored AgentToken security declaration. Generated clients
      // must not classify agent-token operations as anonymous.
    }
  }
  return enriched;
}

function validateIdempotencyContract(pathKey, method, pathItem, operation, openapi) {
  const marker = operation["x-sdkwork-idempotent"];
  const parameters = [
    ...(Array.isArray(pathItem.parameters) ? pathItem.parameters : []),
    ...(Array.isArray(operation.parameters) ? operation.parameters : []),
  ].map((parameter) => resolveLocalParameter(parameter, openapi));
  const headers = parameters.filter(
    (parameter) =>
      parameter?.in === "header" &&
      typeof parameter.name === "string" &&
      parameter.name.toLowerCase() === "idempotency-key",
  );
  const label = `${method.toUpperCase()} ${pathKey}`;
  if ((marker === true) !== (headers.length > 0)) {
    throw new Error(
      `${label} must declare x-sdkwork-idempotent: true and required Idempotency-Key together`,
    );
  }
  for (const header of headers) {
    if (
      header.name !== "Idempotency-Key" ||
      header.required !== true ||
      header.schema?.type !== "string" ||
      header.schema?.minLength !== 1 ||
      header.schema?.maxLength !== 128
    ) {
      throw new Error(
        `${label} Idempotency-Key must be required string with minLength 1 and maxLength 128`,
      );
    }
  }
}

function resolveLocalParameter(parameter, openapi) {
  const match = parameter?.$ref?.match(/^#\/components\/parameters\/([^/]+)$/u);
  if (!match) return parameter;
  const name = decodeURIComponent(match[1].replace(/~1/gu, "/").replace(/~0/gu, "~"));
  return openapi.components?.parameters?.[name] ?? parameter;
}

function rewriteRuntimeSetSchemaReferences(value) {
  if (Array.isArray(value)) {
    return value.map(rewriteRuntimeSetSchemaReferences);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [
        key,
        key === "$ref" && typeof child === "string"
          ? child.replace(
              "#/$defs/",
              "#/components/schemas/WebsiteRuntimeSetSnapshot/$defs/",
            )
          : rewriteRuntimeSetSchemaReferences(child),
      ]),
    );
  }
  return value;
}

function extractRoutes(openapi, profile) {
  const routes = [];
  for (const [pathKey, pathItem] of Object.entries(openapi.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!["get", "post", "put", "patch", "delete"].includes(method)) {
        continue;
      }
      routes.push({
        method: method.toUpperCase(),
        path: pathKey,
        operationId: operation.operationId,
        tags: operation.tags ?? [profile.domainTag],
        auth: {
          mode: operation["x-sdkwork-auth-mode"] ?? "dual-token",
          required: true,
        },
        routeAuth: operation["x-sdkwork-route-auth"] ?? null,
        handler: { module: "crate::routes", name: null },
        ownership: {
          owner: "sdkwork-webserver",
          apiAuthority: profile.apiAuthority,
        },
        requestContext: "WebRequestContext",
        apiSurface: profile.surface,
        permission: operation["x-sdkwork-permission"] ?? null,
        idempotent: operation["x-sdkwork-idempotent"] === true,
        rateLimitTier: operation["x-sdkwork-rate-limit-tier"] ?? null,
      });
    }
  }
  return routes;
}

const RATE_LIMIT_TIER_VARIANTS = {
  "auth-critical": "AuthCritical",
  "open-api-default": "OpenApiDefault",
};

function rateLimitTierRust(tier) {
  const variant = RATE_LIMIT_TIER_VARIANTS[tier];
  if (!variant) {
    throw new Error(
      `Unsupported x-sdkwork-rate-limit-tier value: ${tier}. ` +
        `Canonical framework values are: auth-critical, open-api-default.`,
    );
  }
  return `RateLimitTier::${variant}`;
}

const ROUTE_AUTH_VARIANTS = {
  "dual-token": "dual_token",
  "api-key": "api_key",
  "oauth": "oauth",
  "open-api-flexible": "open_api_flexible",
  "refresh-token": "refresh_token",
  "public": "public",
  "agent-token": "agent_token",
  "ingress-token": "ingress_token",
};

function httpRouteAuthHelper(routeAuth, authMode) {
  const label = routeAuth ?? authMode ?? "dual-token";
  const variant = ROUTE_AUTH_VARIANTS[label];
  if (!variant) {
    throw new Error(
      `Unsupported x-sdkwork-route-auth value: ${label}. ` +
        `Canonical values are: ${Object.keys(ROUTE_AUTH_VARIANTS).join(", ")}.`,
    );
  }
  return variant;
}

function httpMethodRust(method) {
  return { GET: "Get", POST: "Post", PATCH: "Patch", PUT: "Put", DELETE: "Delete" }[
    method
  ];
}

function writeHttpRouteManifestRust(crateDir, fnName, routes) {
  const lines = [
    "// @generated by tools/materialize_web_phase1_contracts.mjs - do not edit",
    "",
    "use sdkwork_web_core::{HttpMethod, HttpRoute, HttpRouteManifest, RateLimitTier};",
    "",
    "const HTTP_ROUTES: &[HttpRoute] = &[",
  ];
  for (const route of routes) {
    const auth = httpRouteAuthHelper(route.routeAuth, route.auth?.mode);
    const suffix = [
      route.permission ? `.with_required_permission("${route.permission}")` : "",
      route.idempotent ? ".with_idempotent(true)" : "",
      route.rateLimitTier
        ? `.with_rate_limit_tier(${rateLimitTierRust(route.rateLimitTier)})`
        : "",
    ].join("");
    lines.push(`    HttpRoute::${auth}(`);
    lines.push(`        HttpMethod::${httpMethodRust(route.method)},`);
    lines.push(`        "${route.path}",`);
    lines.push(`        "${route.tags[0] ?? "web"}",`);
    lines.push(`        "${route.operationId}",`);
    lines.push(`    )${suffix},`);
  }
  lines.push("];", "", `pub fn ${fnName}() -> HttpRouteManifest {`, "    HttpRouteManifest::new(HTTP_ROUTES)", "}", "");
  const relativePath = `${crateDir}/src/http_route_manifest.rs`;
  writeText(relativePath, formatRustSource(relativePath, lines.join("\n")));
}

// C4-C7: SDK family metadata, sdkgen input, and generate-sdk wrapper scripts.
// Canonical language list mirrors sdkwork-iam gold standard (12 languages).

const SDK_VERSION = "1.0.0";
const STANDARD_VERSION = "2026-06-26";
const SDK_OWNER = "sdkwork-webserver";
const STANDARD_PROFILE = "sdkwork-v3";

const LANGUAGE_LIST = [
  "typescript",
  "dart",
  "python",
  "go",
  "java",
  "kotlin",
  "swift",
  "csharp",
  "flutter",
  "rust",
  "php",
  "ruby",
];

const LANGUAGE_MANIFEST_FILE = {
  typescript: "package.json",
  dart: "pubspec.yaml",
  flutter: "pubspec.yaml",
  python: "pyproject.toml",
  go: "go.mod",
  java: "pom.xml",
  kotlin: "build.gradle.kts",
  swift: "Package.swift",
  csharp: ".csproj",
  rust: "Cargo.toml",
  php: "composer.json",
  ruby: ".gemspec",
};

function packageNameFor(family, language) {
  const surfaceLabel = sdkSurfaceLabel(family);
  const pascalSurface = pascalCase(surfaceLabel);
  switch (language) {
    case "typescript":
      return `@sdkwork/webserver-${surfaceLabel}-sdk`;
    case "dart":
    case "flutter":
      return `sdkwork_webserver_${surfaceLabel}_sdk`;
    case "python":
    case "swift":
    case "rust":
    case "ruby":
      return `sdkwork-webserver-${surfaceLabel}-sdk`;
    case "go":
      return `github.com/sdkwork/sdkwork-webserver-${surfaceLabel}-sdk`;
    case "java":
    case "kotlin":
      return `com.sdkwork:sdkwork-webserver-${surfaceLabel}-sdk`;
    case "csharp":
      return `SDKWork.Webserver${pascalSurface}Sdk`;
    case "php":
      return `sdkwork/webserver-${surfaceLabel}-sdk`;
    default:
      return `sdkwork-webserver-${surfaceLabel}-sdk-${language}`;
  }
}

function csharpProjectName(family) {
  return `SDKWork.Webserver${pascalCase(sdkSurfaceLabel(family))}Sdk`;
}

function csharpManifestFile(family) {
  return `${csharpProjectName(family)}.csproj`;
}

function rubyGemspecFile(family) {
  return `${family}.gemspec`;
}

function manifestFileFor(family, language) {
  switch (language) {
    case "csharp":
      return csharpManifestFile(family);
    case "ruby":
      return rubyGemspecFile(family);
    default:
      return LANGUAGE_MANIFEST_FILE[language];
  }
}

function namespaceArgsFor(family, language) {
  const surfaceLabel = sdkSurfaceLabel(family);
  const pascalSurface = pascalCase(surfaceLabel);
  const ns = `com.sdkwork.webserver.${surfaceLabel}.sdk`;
  const csharpNs = `SDKWork.Webserver${pascalSurface}Sdk`;
  const phpNs = `SDKWork\\Webserver\\${pascalSurface}Sdk`;
  switch (language) {
    case "java":
    case "kotlin":
      return ["--namespace", ns];
    case "csharp":
      return ["--namespace", csharpNs];
    case "php":
      return ["--namespace", phpNs];
    default:
      return [];
  }
}

function sdkSurfaceLabel(family) {
  if (family.endsWith("-app-sdk")) return "app";
  if (family.endsWith("-backend-sdk")) return "backend";
  if (family.endsWith("-internal-sdk")) return "internal";
  throw new Error(`Unsupported Web SDK family surface: ${family}`);
}

function sdkGeneratorType(profile) {
  return profile.generatorType ?? sdkSurfaceLabel(profile.sdkFamily);
}

function pascalCase(value) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function sdkDependenciesFor(surface) {
  if (surface === "app-api") {
    return [
      {
        family: "sdkwork-iam-app-sdk",
        reason: "IAM login/session and tenant request context for protected app-api routes.",
      },
    ];
  }
  if (surface === "internal-api") {
    return [];
  }
  return [
    {
      family: "sdkwork-iam-backend-sdk",
      reason: "Platform operator IAM for protected backend-api routes.",
    },
  ];
}

function countOwnerOperations(openapi) {
  let count = 0;
  for (const pathItem of Object.values(openapi.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (
        ["get", "post", "put", "patch", "delete"].includes(method) &&
        operation["x-sdkwork-owner"] === SDK_OWNER
      ) {
        count += 1;
      }
    }
  }
  return count;
}

function syncSdkManifest(profile, openapi) {
  const relativePath = `sdks/${profile.sdkFamily}/sdk-manifest.json`;
  const manifestPath = path.join(root, relativePath);
  if (!fs.existsSync(manifestPath)) {
    writeJson(relativePath, newSdkManifest(profile, openapi));
  }

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const expectedIdentity = {
    sdkFamily: profile.sdkFamily,
    sdkName: profile.sdkFamily,
    sdkOwner: SDK_OWNER,
    apiAuthority: profile.apiAuthority,
  };
  for (const [field, expected] of Object.entries(expectedIdentity)) {
    if (manifest[field] !== expected) {
      throw new Error(
        `${relativePath} ${field} must be ${JSON.stringify(expected)}, got ${JSON.stringify(manifest[field])}`,
      );
    }
  }

  manifest.ownerOnlyOperationCount = countOwnerOperations(openapi);
  manifest.standardProfile = STANDARD_PROFILE;
  manifest.sdkDependencies = sdkDependenciesFor(profile.surface);
  manifest.sdkSurface = sdkSurfaceLabel(profile.sdkFamily);
  manifest.sdkType = sdkGeneratorType(profile);
  manifest.metadata = {
    ...manifest.metadata,
    managedBy: "tools/materialize_web_phase1_contracts.mjs",
    standardVersion: STANDARD_VERSION,
  };
  writeJson(relativePath, manifest);
}

function newSdkManifest(profile, openapi) {
  const family = profile.sdkFamily;
  const surfaceLabel = sdkSurfaceLabel(family);
  const languages = profile.languages ?? LANGUAGE_LIST;
  const sdkgenFileName = `sdkwork-webserver-${surfaceLabel}-api.sdkgen.yaml`;
  const authorityFileName = path.basename(profile.sdkJsonPath);
  const languageEntries = languages.map((language) => {
    const workspace = `${family}-${language}`;
    const generatedPath = `${workspace}/generated/server-openapi`;
    const entry = {
      language,
      workspace,
      generationState: "materialized",
      releaseState: "not_published",
      packagePath: generatedPath,
      manifestPath: `${generatedPath}/${manifestFileFor(family, language)}`,
      name: packageNameFor(family, language),
      version: SDK_VERSION,
      description: `Generator-owned ${language} transport SDK for SDKWork Web ${pascalCase(surfaceLabel)} API.`,
      generatedPath,
    };
    if (language === "typescript") {
      entry.consumerPackageName = `@sdkwork/webserver-${surfaceLabel}-sdk`;
      entry.transportPackageName = `${family}-generated-typescript`;
    }
    return entry;
  });
  return {
    schemaVersion: 1,
    sdkFamily: family,
    sdkName: family,
    packageName: `@sdkwork/webserver-${surfaceLabel}-sdk`,
    transportPackageName: `${family}-generated-typescript`,
    typescript: {
      composedRoot: `${family}-typescript`,
      composedEntry: `${family}-typescript/src/index.ts`,
      transportRoot: `${family}-typescript/generated/server-openapi`,
      transportEntry: `${family}-typescript/generated/server-openapi/src/index.ts`,
    },
    workspace: family,
    title: `SDKWork Webserver ${pascalCase(surfaceLabel)} API SDK`,
    apiVersion: SDK_VERSION,
    openapiVersion: "3.1.2",
    authoritySpec: `openapi/${authorityFileName}`,
    generationInputSpec: `openapi/${sdkgenFileName}`,
    derivedSpecs: {
      default: `openapi/${sdkgenFileName}`,
      flutter: `openapi/${sdkgenFileName}`,
    },
    sdkOwner: SDK_OWNER,
    apiAuthority: profile.apiAuthority,
    ownerOnlyOperationCount: countOwnerOperations(openapi),
    standardProfile: STANDARD_PROFILE,
    sdkSurface: surfaceLabel,
    sdkType: sdkGeneratorType(profile),
    discoverySurface: {
      sdkTarget: surfaceLabel,
      apiPrefix: profile.prefix,
      schemaUrl: "/internal/v3/openapi.json",
      generatedProtocols: ["http-openapi"],
      manualTransports: [],
    },
    metadata: {
      managedBy: "tools/materialize_web_phase1_contracts.mjs",
      standardVersion: STANDARD_VERSION,
    },
    languages: languageEntries,
    sdkDependencies: sdkDependenciesFor(profile.surface),
  };
}

function writeComponentSpec(profile, openapi) {
  const family = profile.sdkFamily;
  const surfaceLabel = sdkSurfaceLabel(family);
  const sdkType = sdkGeneratorType(profile);
  const displayName = `SDKWork Web ${pascalCase(surfaceLabel)} SDK`;
  const capability = `web-${surfaceLabel}-sdk`;
  const languages = profile.languages ?? LANGUAGE_LIST;
  const componentSpec = {
    schemaVersion: 1,
    name: family,
    type: "sdk-family",
    domain: "web",
    apiAuthority: profile.apiAuthority,
    apiPrefix: profile.prefix,
    sdkSurface: surfaceLabel,
    sdkType,
    languages,
    sdk: {
      family,
      authority: profile.apiAuthority,
      sdkOwner: SDK_OWNER,
      generationInputSpec: `openapi/sdkwork-webserver-${surfaceLabel}-api.sdkgen.yaml`,
      apiPrefix: profile.prefix,
      packageName: `@sdkwork/webserver-${surfaceLabel}-sdk`,
      sdkName: family,
      sdkType,
      sdkSurface: surfaceLabel,
      standardProfile: STANDARD_PROFILE,
      ownerOnlyOperationCount: countOwnerOperations(openapi),
    },
    generator: {
      package: "@sdkwork/sdk-generator",
      entrypoint: "../sdkwork-sdk-generator/bin/sdkgen.js",
      type: sdkType,
      standardProfile: STANDARD_PROFILE,
    },
    contracts: {
      layerRole: "sdk-generated",
      sdkDependencies: sdkDependenciesFor(profile.surface),
      publicExports: ["."],
      runtimeEntrypoints: [],
      routeManifest: null,
      sdkClients: [],
      events: [],
      configKeys: [],
    },
    auth:
      profile.surface === "internal-api"
        ? {
            mode: "ingress-token",
            apiKeyHeader: "X-API-Key",
            requestIdHeader: "X-Request-Id",
            requestIdOwnership: "server",
          }
        : {
            mode: "dual-token",
            authTokenHeader: "Authorization",
            accessTokenHeader: "Access-Token",
            requestIdHeader: "X-Request-Id",
            requestIdOwnership: "server",
          },
    requestContextFramework: {
      apiSurface: profile.surface,
      contextType: "WebRequestContext",
      resolver:
        profile.surface === "app-api"
          ? "WebRequestContextResolver"
          : "WebRequestContextResolver + MachineCredentialResolverDecorator",
      standardInterceptors: [
        "request_identity",
        "surface_classification",
        "cors",
        "method_guard",
        "header_security",
        "cross_site_request",
        "sql_injection_guard",
        "request_size_limit",
        "rate_limit",
        "idempotency",
        "request_context_resolution",
        "authentication",
        "authorization",
        "tenant_isolation",
        "context_injection",
        "logging",
        "audit",
        "response_identity",
      ],
    },
    kind: "sdkwork.component.spec",
    component: {
      name: family,
      displayName,
      version: "0.1.0",
      type: "sdk-family",
      root: `sdkwork-webserver/sdks/${family}`,
      domain: "web",
      capability,
      languages,
      generated: false,
      manifests: [],
    },
    canonicalSpecs: [
      { file: "COMPONENT_SPEC.md", path: "../../../sdkwork-specs/COMPONENT_SPEC.md", purpose: "Component-local contract and discovery rules." },
      { file: "CODE_STYLE_SPEC.md", path: "../../../sdkwork-specs/CODE_STYLE_SPEC.md", purpose: "Authored source structure and generated code boundaries." },
      { file: "NAMING_SPEC.md", path: "../../../sdkwork-specs/NAMING_SPEC.md", purpose: "Canonical SDKWork naming rules." },
      { file: "MODULE_SPEC.md", path: "../../../sdkwork-specs/MODULE_SPEC.md", purpose: "Reusable module and package boundary rules." },
      { file: "TEST_SPEC.md", path: "../../../sdkwork-specs/TEST_SPEC.md", purpose: "Verification and contract testing expectations." },
      { file: "TYPESCRIPT_CODE_SPEC.md", path: "../../../sdkwork-specs/TYPESCRIPT_CODE_SPEC.md", purpose: "TypeScript and Node package rules." },
      { file: "RUST_CODE_SPEC.md", path: "../../../sdkwork-specs/RUST_CODE_SPEC.md", purpose: "Rust crate and module rules." },
      { file: "JAVA_CODE_SPEC.md", path: "../../../sdkwork-specs/JAVA_CODE_SPEC.md", purpose: "Java and Maven module rules." },
      { file: "FRONTEND_CODE_SPEC.md", path: "../../../sdkwork-specs/FRONTEND_CODE_SPEC.md", purpose: "Frontend authored source structure." },
      { file: "FRONTEND_SPEC.md", path: "../../../sdkwork-specs/FRONTEND_SPEC.md", purpose: "Frontend service, state, SDK, and UI boundary rules." },
      { file: "UI_ARCHITECTURE_SPEC.md", path: "../../../sdkwork-specs/UI_ARCHITECTURE_SPEC.md", purpose: "UI architecture selection rules." },
      { file: "APP_FLUTTER_UI_SPEC.md", path: "../../../sdkwork-specs/APP_FLUTTER_UI_SPEC.md", purpose: "Flutter package rules." },
      { file: "SDK_SPEC.md", path: "../../../sdkwork-specs/SDK_SPEC.md", purpose: "SDK family and generated client integration rules." },
      { file: "SDK_WORKSPACE_GENERATION_SPEC.md", path: "../../../sdkwork-specs/SDK_WORKSPACE_GENERATION_SPEC.md", purpose: "SDK workspace and generated artifact placement rules." },
      { file: "API_SPEC.md", path: "../../../sdkwork-specs/API_SPEC.md", purpose: "OpenAPI authority rules for SDK generation." },
    ],
    verification: {
      commands: [
        `node --input-type=module -e "import { readFileSync } from 'node:fs'; JSON.parse(readFileSync('specs/component.spec.json','utf8'));"`,
        "node tools/materialize_web_phase1_contracts.mjs",
        `node sdks/${family}/bin/generate-sdk.mjs`,
      ],
    },
    metadata: {
      managedBy: "sdkwork-webserver",
      standardVersion: STANDARD_VERSION,
    },
  };
  writeJson(`sdks/${family}/specs/component.spec.json`, componentSpec);
}

function writeSdkgenInput(profile, openapi) {
  const family = profile.sdkFamily;
  const surfaceLabel = sdkSurfaceLabel(family);
  const sdkgenFileName = `sdkwork-webserver-${surfaceLabel}-api.sdkgen.yaml`;
  writeJson(`sdks/${family}/openapi/${sdkgenFileName}`, openapi);
}

function writeGenerateSdkScripts(profile) {
  const family = profile.sdkFamily;
  const surfaceLabel = sdkSurfaceLabel(family);
  const sdkName = family;
  const apiPrefix = profile.prefix;
  const generatorType = sdkGeneratorType(profile);
  const sdkgenFileName = `sdkwork-webserver-${surfaceLabel}-api.sdkgen.yaml`;
  const familyDir = `sdks/${family}`;
  const defaultLanguages = (profile.languages ??
    (profile.surface === "app-api" ? ["typescript"] : ["typescript", "rust"])
  ).join(",");

  // generate-sdk.mjs (cross-platform entry point)
  const mjs = `#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const languages = process.env.LANGUAGES || process.argv[2] || '${defaultLanguages}';

const result = process.platform === 'win32'
  ? spawnSync(
    'powershell',
    [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      path.join(__dirname, 'generate-sdk.ps1'),
      '-Languages',
      languages,
    ],
    { stdio: 'inherit' },
  )
  : spawnSync(
    'bash',
    [path.join(__dirname, 'generate-sdk.sh')],
    {
      stdio: 'inherit',
      env: {
        ...process.env,
        LANGUAGES: languages,
      },
    },
  );

process.exit(result.status ?? 1);
`;

  // generate-sdk.ps1 (Windows)
  const ps1 = `param(
    [string[]]$Languages = @("typescript", "dart", "python", "go", "java", "kotlin", "swift", "csharp", "flutter", "rust", "php", "ruby"),
    [string]$BaseUrl = "http://localhost:3800",
    [string]$SdkVersion = "${SDK_VERSION}"
)

$ErrorActionPreference = "Stop"

function Resolve-PackageName {
    param([string]$Language)

    switch ($Language) {
${LANGUAGE_LIST.map((lang) => {
  const pkg = packageNameFor(sdkName, lang);
  return `        "${lang}" { return "${pkg}" }`;
}).join("\n")}
        default { return "${sdkName}-$Language" }
    }
}

function Resolve-NamespaceArgs {
    param([string]$Language)

    switch ($Language) {
${LANGUAGE_LIST.filter((lang) => namespaceArgsFor(sdkName, lang).length > 0)
  .map((lang) => {
    const args = namespaceArgsFor(sdkName, lang);
    return `        "${lang}" { return @("${args[0]}", "${args[1]}") }`;
  })
  .join("\n")}
        default { return @() }
    }
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$FamilyRoot = (Get-Item $ScriptDir).Parent.FullName
$WebRoot = (Get-Item $FamilyRoot).Parent.Parent.FullName
$WorkspaceRoot = (Get-Item (Join-Path $FamilyRoot "..\\..\\..")).FullName
$GeneratorPath = Join-Path $WorkspaceRoot "sdkwork-sdk-generator\\bin\\sdkgen.js"
$InputPath = Join-Path $FamilyRoot "openapi\\${sdkgenFileName}"
$SdkName = "${sdkName}"
$ApiPrefix = "${apiPrefix}"
$SupportedLanguages = @(${LANGUAGE_LIST.map((language) => `"${language}"`).join(", ")})

if (-not (Test-Path $GeneratorPath)) {
    throw "Canonical SDK generator not found: $GeneratorPath"
}
if (-not (Test-Path $InputPath)) {
    & node (Join-Path $WebRoot "tools\\materialize_web_phase1_contracts.mjs")
}
if (-not (Test-Path $InputPath)) {
    throw "OpenAPI sdkgen input not found: $InputPath"
}

foreach ($LanguageValue in $Languages) {
    foreach ($LanguagePart in "$LanguageValue".Split(",")) {
        $Language = $LanguagePart.Trim()
        if ([string]::IsNullOrWhiteSpace($Language)) {
            continue
        }
        if ($Language -notin $SupportedLanguages) {
            throw "Unsupported SDK language: $Language"
        }

        $LanguageWorkspace = Join-Path $FamilyRoot "$SdkName-$Language"
        $OutputPath = Join-Path $LanguageWorkspace "generated\\server-openapi"
        $PackageName = Resolve-PackageName $Language
        $NamespaceArgs = Resolve-NamespaceArgs $Language
        $ResolvedLanguageWorkspace = [System.IO.Path]::GetFullPath($LanguageWorkspace)
        $ResolvedOutputPath = [System.IO.Path]::GetFullPath($OutputPath)
        $LanguageWorkspacePrefix = $ResolvedLanguageWorkspace.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar

        if (-not $ResolvedOutputPath.StartsWith($LanguageWorkspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing SDK output outside language workspace: $ResolvedOutputPath"
        }

        Write-Host "Generating $Language SDK at $OutputPath" -ForegroundColor Cyan
        & node $GeneratorPath generate \`
            -i $InputPath \`
            -o $OutputPath \`
            -n $SdkName \`
            -t ${generatorType} \`
            -l $Language \`
            --fixed-sdk-version $SdkVersion \`
            --base-url $BaseUrl \`
            --api-prefix $ApiPrefix \`
            --package-name $PackageName \`
            --standard-profile sdkwork-v3 \`
            --sdk-root $FamilyRoot \`
            --sdk-name $SdkName \`
            --no-sync-published-version \`
            @NamespaceArgs

        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
}
`;

  // generate-sdk.sh (Unix)
  const sh = `#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
FAMILY_ROOT="$(cd "\${SCRIPT_DIR}/.." && pwd)"
WEB_ROOT="$(cd "\${FAMILY_ROOT}/../.." && pwd)"
WORKSPACE_ROOT="$(cd "\${FAMILY_ROOT}/../../.." && pwd)"
GENERATOR_PATH="\${WORKSPACE_ROOT}/sdkwork-sdk-generator/bin/sdkgen.js"
INPUT_PATH="\${FAMILY_ROOT}/openapi/${sdkgenFileName}"
SDK_NAME="${sdkName}"
BASE_URL="\${BASE_URL:-http://localhost:3800}"
SDK_VERSION="\${SDK_VERSION:-${SDK_VERSION}}"
API_PREFIX="${apiPrefix}"
LANGUAGES="\${LANGUAGES:-typescript,dart,python,go,java,kotlin,swift,csharp,flutter,rust,php,ruby}"

if [[ ! -f "\${GENERATOR_PATH}" ]]; then
  echo "Canonical SDK generator not found: \${GENERATOR_PATH}" >&2
  exit 1
fi

if [[ ! -f "\${INPUT_PATH}" ]]; then
  node "\${WEB_ROOT}/tools/materialize_web_phase1_contracts.mjs"
fi

if [[ ! -f "\${INPUT_PATH}" ]]; then
  echo "OpenAPI sdkgen input not found: \${INPUT_PATH}" >&2
  exit 1
fi

package_name() {
  case "$1" in
${LANGUAGE_LIST.map((lang) => {
  const pkg = packageNameFor(sdkName, lang);
  return `    ${lang}) echo "${pkg}" ;;`;
}).join("\n")}
    *) echo "Unsupported SDK language: $1" >&2; return 1 ;;
  esac
}

namespace_args() {
  case "$1" in
${LANGUAGE_LIST.filter((lang) => namespaceArgsFor(sdkName, lang).length > 0)
  .map((lang) => {
    const args = namespaceArgsFor(sdkName, lang);
    return `    ${lang}) printf '%s\\n' "${args[0]}" "${args[1]}" ;;`;
  })
  .join("\n")}
  esac
}

IFS=',' read -r -a language_array <<< "\${LANGUAGES}"
for language in "\${language_array[@]}"; do
  language="$(echo "\${language}" | xargs)"
  [[ -z "\${language}" ]] && continue
  output_path="\${FAMILY_ROOT}/\${SDK_NAME}-\${language}/generated/server-openapi"
  mapfile -t ns_args < <(namespace_args "\${language}")
  node "\${GENERATOR_PATH}" generate \\
    -i "\${INPUT_PATH}" \\
    -o "\${output_path}" \\
    -n "\${SDK_NAME}" \\
    -t ${generatorType} \\
    -l "\${language}" \\
    --fixed-sdk-version "\${SDK_VERSION}" \\
    --base-url "\${BASE_URL}" \\
    --api-prefix "\${API_PREFIX}" \\
    --package-name "$(package_name "\${language}")" \\
    --standard-profile sdkwork-v3 \\
    --sdk-root "\${FAMILY_ROOT}" \\
    --sdk-name "\${SDK_NAME}" \\
    --no-sync-published-version \\
    "\${ns_args[@]}"
done
`;

  writeText(`${familyDir}/bin/generate-sdk.mjs`, mjs);
  writeText(`${familyDir}/bin/generate-sdk.ps1`, ps1);
  writeText(`${familyDir}/bin/generate-sdk.sh`, sh);
}

for (const profile of surfaces) {
  const yaml = parseYaml(fs.readFileSync(path.join(root, profile.yamlPath), "utf8"));
  const openapi = enrichOpenApi(yaml, profile);
  writeJson(profile.jsonAuthorityPath, openapi);
  writeJson(profile.sdkJsonPath, openapi);
  const routes = extractRoutes(openapi, profile);
  writeJson(profile.routeManifestPath, {
    schemaVersion: 1,
    kind: "sdkwork.route.manifest",
    packageName: profile.packageName,
    surface: profile.surface,
    owner: "sdkwork-webserver",
    domain: "platform",
    capability: "webserver",
    apiAuthority: profile.apiAuthority,
    sdkFamily: profile.sdkFamily,
    prefix: profile.prefix,
    source: {
      crateRoot: profile.crateDir,
      crateImport: profile.packageName.replaceAll("-", "_"),
      openApiAuthority: profile.sdkJsonPath,
    },
    routes,
  });
  writeHttpRouteManifestRust(profile.crateDir, profile.manifestFn, routes);
  // C4-C7: emit full family metadata, sdkgen input, and generate-sdk wrappers.
  writeComponentSpec(profile, openapi);
  syncSdkManifest(profile, openapi);
  writeSdkgenInput(profile, openapi);
  writeGenerateSdkScripts(profile);
}

writeJson("apis/authority-manifest.json", {
  schemaVersion: 1,
  kind: "sdkwork.api.authority.manifest",
  surfaces: surfaces.map((profile) => ({
    authorityPath: profile.jsonAuthorityPath,
    sdkPath: profile.sdkJsonPath,
  })),
});

if (checkMode && drift.length > 0) {
  throw new Error(`web phase-1 contract drift: ${drift.join(", ")}`);
}
console.log(checkMode ? "web phase-1 contracts are current" : "web phase-1 contracts materialized");
