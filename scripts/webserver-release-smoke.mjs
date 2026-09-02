#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { extract as extractTar } from 'tar';

import {
  canonicalizeWorkspaceDatabaseEnv,
  IAM_APPLICATION_BOOTSTRAP_ENV,
  resolveIamDevEnv,
} from './lib/webserver-topology.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const MAX_PROCESS_OUTPUT_BYTES = 256 * 1024;
const MAX_RESPONSE_BYTES = 64 * 1024;
const COMMAND_TIMEOUT_MS = 30 * 1000;
const START_TIMEOUT_MS = 15 * 1000;
const STOP_TIMEOUT_MS = 10 * 1000;
const SUPPORTED_ARCHITECTURES = new Set(['x64', 'arm64']);
const STANDALONE_PC_ROOT = 'share/sdkwork/webserver-pc';
const STANDALONE_H5_ROOT = 'share/sdkwork/webserver-h5';
const STANDALONE_STATIC_FALLBACK_ROOT = 'share/sdkwork/webserver-static';
const STANDALONE_IAM_ROOT = 'share/sdkwork/iam';
const STANDALONE_DRIVE_ROOT = 'share/sdkwork/drive';
const STANDALONE_SAME_ORIGIN_PATHS = Object.freeze({
  shell: '/',
  runtimeEnv: '/runtime-env.json',
  navigation: '/console/applications',
  openapi: '/openapi.json',
  webApplications: '/app/v3/api/applications',
  iamSession: '/app/v3/api/auth/sessions/current',
  driveAssets: '/app/v3/api/assets',
  missingApi: '/app/v3/api/__sdkwork_release_smoke_missing__',
});
const EXPECTED_BINARIES = [
  'sdkwork-api-webserver-standalone-gateway',
  'sdkwork-webserver-website-delivery-edge-runtime',
  'sdkwork-webserver-node-daemon',
  'sdkwork-webserver-agent',
  'sdkwork-webserver-certificate-worker',
];
// The packaged binaries fall back to the host canonical runtime config
// (`/etc/sdkwork/webserver/config.toml`, RUNTIME_DIRECTORY_SPEC §4.1) whenever
// SDKWORK_WEBSERVER_CONFIG_FILE is unset. A host that already carries a native
// install of another environment then injects that environment's
// [database]/[ingress]/[app_roots] values into the process, which silently
// corrupts the verification. The smoke therefore pins a self-owned config file
// that declares the profile only and leaves every other value to the env.
const HERMETIC_RUNTIME_CONFIG = `# sdkwork-webserver release smoke hermetic runtime configuration.
# Declares the process profile and nothing else: every other value is supplied
# by the smoke environment so the verification never depends on host state.
[profile]
deployment_profile = "standalone"
environment = "production"
profile_id = "standalone.production"
node_id = 0
`;

function parseArgs(argv) {
  const settings = {
    deploymentProfile: process.env.SDKWORK_DEPLOYMENT_PROFILE,
    architecture: process.env.SDKWORK_PACKAGE_ARCHITECTURE,
    version: undefined,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--deployment-profile') {
      settings.deploymentProfile = argv[++index];
    } else if (argument === '--architecture') {
      settings.architecture = argv[++index];
    } else if (argument === '--version') {
      settings.version = argv[++index];
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  return settings;
}

function resolveArtifact(settings) {
  if (!['standalone', 'cloud'].includes(settings.deploymentProfile)) {
    throw new Error('--deployment-profile must be standalone or cloud');
  }
  const manifest = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'sdkwork.app.config.json'), 'utf8'),
  );
  const packageVersion = process.env.SDKWORK_PACKAGE_VERSION?.trim();
  const compatibilityVersion = process.env.SDKWORK_RELEASE_VERSION?.trim();
  if (packageVersion && compatibilityVersion && packageVersion !== compatibilityVersion) {
    throw new Error('SDKWORK_PACKAGE_VERSION conflicts with SDKWORK_RELEASE_VERSION');
  }
  const version =
    settings.version || packageVersion || compatibilityVersion || manifest.release?.defaultVersion;
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/u.test(version ?? '')) {
    throw new Error('release version must be an explicit semantic version');
  }
  const architecture = settings.architecture?.trim() || process.arch;
  if (!SUPPORTED_ARCHITECTURES.has(architecture)) {
    throw new Error('release architecture must be x64 or arm64');
  }
  const artifactBase = `sdkwork-webserver-linux-${architecture}-${settings.deploymentProfile}-server-${version}`;
  return {
    version,
    architecture,
    artifactBase,
    archive: path.join(OUTPUT_ROOT, `${artifactBase}.tar.gz`),
  };
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    env: options.env ?? process.env,
    stdio: 'pipe',
    timeout: options.timeoutMs ?? COMMAND_TIMEOUT_MS,
    maxBuffer: MAX_PROCESS_OUTPUT_BYTES,
    killSignal: 'SIGKILL',
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail =
      result.error?.message || result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result;
}

function captureBounded(stream) {
  const chunks = [];
  let retainedBytes = 0;
  stream.on('data', (chunk) => {
    if (retainedBytes >= MAX_PROCESS_OUTPUT_BYTES) {
      return;
    }
    const retained = Buffer.from(chunk).subarray(0, MAX_PROCESS_OUTPUT_BYTES - retainedBytes);
    chunks.push(retained);
    retainedBytes += retained.length;
  });
  return () => Buffer.concat(chunks, retainedBytes).toString('utf8');
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function reservePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : undefined;
      server.close((error) => {
        if (error) {
          reject(error);
        } else if (!port) {
          reject(new Error('ephemeral port allocation returned no port'));
        } else {
          resolve(port);
        }
      });
    });
  });
}

function requestHealth(protocol, port) {
  return new Promise((resolve, reject) => {
    const transport = protocol === 'https' ? https : http;
    const request = transport.request(
      {
        host: '127.0.0.1',
        port,
        path: '/healthz',
        method: 'GET',
        headers: { host: 'localhost' },
        rejectUnauthorized: protocol === 'https' ? false : undefined,
        servername: protocol === 'https' ? 'localhost' : undefined,
      },
      (response) => {
        const chunks = [];
        let bytes = 0;
        response.on('data', (chunk) => {
          bytes += chunk.length;
          if (bytes > MAX_RESPONSE_BYTES) {
            request.destroy(new Error(`smoke response exceeds ${MAX_RESPONSE_BYTES} bytes`));
            return;
          }
          chunks.push(Buffer.from(chunk));
        });
        response.once('error', reject);
        response.once('end', () => {
          resolve({
            statusCode: response.statusCode,
            body: Buffer.concat(chunks, bytes).toString('utf8'),
          });
        });
      },
    );
    request.setTimeout(2_000, () => request.destroy(new Error('smoke request timed out')));
    request.once('error', reject);
    request.end();
  });
}

function requestHttp(port, requestPath, headers = {}) {
  return new Promise((resolve, reject) => {
    const request = http.request(
      {
        host: '127.0.0.1',
        port,
        path: requestPath,
        method: 'GET',
        headers: { host: 'localhost', ...headers },
      },
      (response) => {
        const chunks = [];
        let bytes = 0;
        response.on('data', (chunk) => {
          bytes += chunk.length;
          if (bytes > MAX_RESPONSE_BYTES) {
            request.destroy(new Error(`smoke response exceeds ${MAX_RESPONSE_BYTES} bytes`));
            return;
          }
          chunks.push(Buffer.from(chunk));
        });
        response.once('error', reject);
        response.once('end', () => {
          resolve({
            statusCode: response.statusCode,
            headers: response.headers,
            body: Buffer.concat(chunks, bytes).toString('utf8'),
          });
        });
      },
    );
    request.setTimeout(2_000, () => request.destroy(new Error('smoke request timed out')));
    request.once('error', reject);
    request.end();
  });
}

async function waitForHealth(protocol, port, child, readOutput) {
  const deadline = Date.now() + START_TIMEOUT_MS;
  let lastError;
  while (Date.now() < deadline) {
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error(`packaged gateway exited before readiness: ${readOutput()}`);
    }
    try {
      const response = await requestHealth(protocol, port);
      if (response.statusCode === 200 && response.body === 'release-smoke\n') {
        return;
      }
      lastError = new Error(
        `${protocol} health returned status=${response.statusCode} body=${JSON.stringify(response.body)}`,
      );
    } catch (error) {
      lastError = error;
    }
    await delay(100);
  }
  throw new Error(`${protocol} health did not become ready: ${lastError?.message ?? 'unknown'}`);
}

async function waitForManagementIngress(port, child, readOutput) {
  const deadline = Date.now() + START_TIMEOUT_MS;
  let lastError;
  while (Date.now() < deadline) {
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error(`packaged standalone ingress exited before readiness: ${readOutput()}`);
    }
    try {
      const response = await requestHttp(port, '/readyz');
      if (response.statusCode === 200) {
        return;
      }
      lastError = new Error(
        `standalone readiness returned status=${response.statusCode} body=${JSON.stringify(response.body)}`,
      );
    } catch (error) {
      lastError = error;
    }
    await delay(100);
  }
  throw new Error(
    `standalone application ingress did not become ready: ${lastError?.message ?? 'unknown'}`,
  );
}

function assertContentType(response, expected, label) {
  const contentType = String(response.headers['content-type'] ?? '').toLowerCase();
  if (!contentType.includes(expected)) {
    throw new Error(`${label} must use ${expected}; received ${JSON.stringify(contentType)}`);
  }
}

function assertStatus(response, expected, label) {
  if (response.statusCode !== expected) {
    throw new Error(
      `${label} returned status=${response.statusCode} body=${JSON.stringify(response.body)}`,
    );
  }
}

function assertUnauthenticatedOwnerRoute(response, expected, label) {
  assertStatus(response, 401, label);
  assertContentType(response, 'application/problem+json', label);
  let problem;
  try {
    problem = JSON.parse(response.body);
  } catch (error) {
    throw new Error(`${label} returned invalid problem JSON: ${error.message}`);
  }
  for (const [field, value] of Object.entries(expected)) {
    if (problem[field] !== value) {
      throw new Error(
        `${label} problem.${field} must equal ${JSON.stringify(value)}; received ${JSON.stringify(problem[field])}`,
      );
    }
  }
  if (!Number.isInteger(problem.code) || problem.code !== 40101) {
    throw new Error(`${label} problem.code must equal 40101`);
  }
  if (typeof problem.traceId !== 'string' || problem.traceId.length === 0) {
    throw new Error(`${label} problem.traceId must be non-empty`);
  }
}

function standaloneManagementEnv(packageRoot, port, runtimeConfigFile) {
  const iamRoot = path.join(packageRoot, ...STANDALONE_IAM_ROOT.split('/'));
  const driveRoot = path.join(packageRoot, ...STANDALONE_DRIVE_ROOT.split('/'));
  const databaseEnv = canonicalizeWorkspaceDatabaseEnv(
    resolveIamDevEnv(process.env, { postgresEnvFile: '.env.postgres' }),
  );
  return {
    ...databaseEnv,
    ...IAM_APPLICATION_BOOTSTRAP_ENV,
    SDKWORK_WEBSERVER_CONFIG_FILE: runtimeConfigFile,
    RUST_LOG: 'info',
    SDKWORK_APP_ROOT: packageRoot,
    SDKWORK_WEBSERVER_APP_ROOT: packageRoot,
    SDKWORK_WEBSERVER_SERVER_APP_ROOT: packageRoot,
    SDKWORK_IAM_APP_ROOT: iamRoot,
    SDKWORK_DRIVE_APP_ROOT: driveRoot,
    SDKWORK_DEPLOYMENT_PROFILE: 'standalone',
    SDKWORK_ENVIRONMENT: 'production',
    SDKWORK_PROFILE_ID: 'standalone.production',
    SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE: 'standalone',
    SDKWORK_WEBSERVER_ENVIRONMENT: 'production',
    SDKWORK_WEBSERVER_PROFILE_ID: 'standalone.production',
    SDKWORK_WEBSERVER_RUNTIME_TARGET: 'server',
    SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID: '0',
    SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND: `127.0.0.1:${port}`,
    SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL: `http://127.0.0.1:${port}`,
    SDKWORK_WEBSERVER_APPLICATION_APP_HTTP_URL: `http://127.0.0.1:${port}`,
    SDKWORK_WEBSERVER_APPLICATION_BACKEND_HTTP_URL: `http://127.0.0.1:${port}`,
    SDKWORK_WEBSERVER_PC_STATIC_ROOT: STANDALONE_PC_ROOT,
    SDKWORK_WEBSERVER_H5_STATIC_ROOT: STANDALONE_H5_ROOT,
    SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT: STANDALONE_STATIC_FALLBACK_ROOT,
    SDKWORK_DATABASE_AUTO_MIGRATE: 'true',
    SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY:
      'sdkwork-release-smoke-web-secret-encryption-key-2026',
    // `production` is a production-like environment: the certificate issuer
    // requires a durable ACME account store and a contact address, and refuses
    // to start without them. The smoke keeps both under its own temporary root.
    SDKWORK_WEBSERVER_ACME_PROFILE: 'staging',
    SDKWORK_WEBSERVER_ACME_CONTACT_EMAIL: 'release-smoke@example.invalid',
    SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT: path.join(packageRoot, '..', 'smoke-acme-accounts'),
    SDKWORK_WEBSERVER_ACME_WEBROOT: path.join(packageRoot, '..', 'smoke-acme-webroot'),
    SDKWORK_DRIVE_DOWNLOAD_TOKEN_HMAC_SECRET:
      'sdkwork-release-smoke-drive-download-token-secret-2026',
  };
}

async function verifyStandaloneSameOriginIngress({
  gateway,
  packageRoot,
  temporaryRoot,
  runtimeConfigFile,
}) {
  const port = await reservePort();
  const child = spawn(gateway, ['serve-management'], {
    cwd: temporaryRoot,
    env: standaloneManagementEnv(packageRoot, port, runtimeConfigFile),
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  const readStdout = captureBounded(child.stdout);
  const readStderr = captureBounded(child.stderr);
  const readOutput = () => `${readStdout()}\n${readStderr()}`.trim();

  try {
    await waitForManagementIngress(port, child, readOutput);

    const shell = await requestHttp(port, STANDALONE_SAME_ORIGIN_PATHS.shell, {
      accept: 'text/html',
    });
    assertStatus(shell, 200, 'standalone shell');
    assertContentType(shell, 'text/html', 'standalone shell');

    const mobileShell = await requestHttp(port, STANDALONE_SAME_ORIGIN_PATHS.shell, {
      accept: 'text/html',
      'sec-ch-ua-mobile': '?1',
      'user-agent': 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)',
    });
    assertStatus(mobileShell, 200, 'standalone adaptive H5 shell');
    assertContentType(mobileShell, 'text/html', 'standalone adaptive H5 shell');
    const vary = String(mobileShell.headers.vary ?? '').toLowerCase();
    if (!vary.includes('user-agent') && !vary.includes('sec-ch-ua-mobile')) {
      throw new Error(
        `standalone Adaptive Web response missing Vary for device selection: vary=${mobileShell.headers.vary ?? ''}`,
      );
    }

    const runtimeEnv = await requestHttp(port, STANDALONE_SAME_ORIGIN_PATHS.runtimeEnv);
    assertStatus(runtimeEnv, 200, 'standalone runtime config');
    assertContentType(runtimeEnv, 'application/json', 'standalone runtime config');
    const runtimeConfig = JSON.parse(runtimeEnv.body);
    if (
      runtimeConfig.browserOriginMode !== 'same-origin'
      || runtimeConfig.deploymentProfile !== 'standalone'
      || [
        runtimeConfig.appApiBaseUrl,
        runtimeConfig.backendApiBaseUrl,
        runtimeConfig.driveAppApiBaseUrl,
        runtimeConfig.appbaseAppApiBaseUrl,
      ].some((baseUrl) => baseUrl !== '/')
    ) {
      throw new Error('standalone runtime config does not preserve the canonical same-origin root');
    }

    const navigation = await requestHttp(port, STANDALONE_SAME_ORIGIN_PATHS.navigation, {
      accept: 'text/html',
    });
    assertStatus(navigation, 200, 'standalone SPA navigation');
    assertContentType(navigation, 'text/html', 'standalone SPA navigation');

    const openapi = await requestHttp(port, STANDALONE_SAME_ORIGIN_PATHS.openapi);
    assertStatus(openapi, 200, 'standalone OpenAPI');
    assertContentType(openapi, 'application/json', 'standalone OpenAPI');
    const openapiDocument = JSON.parse(openapi.body);
    for (const ownerPath of [
      STANDALONE_SAME_ORIGIN_PATHS.webApplications,
      STANDALONE_SAME_ORIGIN_PATHS.iamSession,
      STANDALONE_SAME_ORIGIN_PATHS.driveAssets,
    ]) {
      if (!openapiDocument.paths?.[ownerPath]) {
        throw new Error(`standalone OpenAPI is missing embedded owner route ${ownerPath}`);
      }
    }

    for (const ownerRoute of [
      {
        label: 'standalone unauthenticated Web applications',
        path: STANDALONE_SAME_ORIGIN_PATHS.webApplications,
        operationId: 'applications.list',
      },
      {
        label: 'standalone unauthenticated IAM current session',
        path: STANDALONE_SAME_ORIGIN_PATHS.iamSession,
        operationId: 'sessions.current.retrieve',
      },
      {
        label: 'standalone unauthenticated Drive assets',
        path: STANDALONE_SAME_ORIGIN_PATHS.driveAssets,
        operationId: 'assets.list',
      },
    ]) {
      const response = await requestHttp(port, ownerRoute.path);
      assertUnauthenticatedOwnerRoute(
        response,
        {
          instance: `GET ${ownerRoute.path}`,
          operationId: ownerRoute.operationId,
        },
        ownerRoute.label,
      );
    }

    const missingApi = await requestHttp(port, STANDALONE_SAME_ORIGIN_PATHS.missingApi, {
      accept: 'text/html',
    });
    assertStatus(missingApi, 404, 'standalone unknown API');
    if (String(missingApi.headers['content-type'] ?? '').toLowerCase().includes('text/html')) {
      throw new Error('standalone unknown API must not return the SPA shell');
    }

    child.kill('SIGTERM');
    const exit = await waitForExit(child, STOP_TIMEOUT_MS);
    if (exit.code !== 0 || exit.signal !== null) {
      throw new Error(
        `packaged standalone ingress exited unexpectedly code=${exit.code} signal=${exit.signal}: ${readOutput()}`,
      );
    }
    return port;
  } catch (error) {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill('SIGKILL');
      await waitForExit(child, 5_000).catch(() => {});
    }
    throw error;
  }
}

function hermeticEnv(runtimeConfigFile) {
  return { ...process.env, SDKWORK_WEBSERVER_CONFIG_FILE: runtimeConfigFile };
}

function writeHermeticRuntimeConfig(temporaryRoot) {
  const directory = path.join(temporaryRoot, 'smoke-config');
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const file = path.join(directory, 'config.toml');
  writeFileSync(file, HERMETIC_RUNTIME_CONFIG, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
  return file;
}

function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve({ code: child.exitCode, signal: child.signalCode });
  }
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('packaged gateway stop timed out')), timeoutMs);
    child.once('error', (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.once('exit', (code, signal) => {
      clearTimeout(timeout);
      resolve({ code, signal });
    });
  });
}

function buildSmokeConfig(example, httpPort, httpsPort, certificateFile, privateKeyFile) {
  return {
    ...example,
    appKey: 'sdkwork-release-smoke',
    limits: {
      ...example.limits,
      maxConnections: 128,
      maxConcurrentRequests: 64,
      maxConcurrentHealthChecks: 4,
      drainTimeoutMs: 1_000,
      maxConnectionAgeMs: 60_000,
    },
    listeners: [
      {
        id: 'smoke-http',
        bind: '127.0.0.1',
        port: httpPort,
        protocols: ['http1'],
        defaultVirtualHostRef: 'smoke-host',
      },
      {
        id: 'smoke-https',
        bind: '127.0.0.1',
        port: httpsPort,
        protocols: ['http1', 'http2'],
        tlsPolicyRef: 'smoke-tls',
        defaultVirtualHostRef: 'smoke-host',
      },
    ],
    certificates: [
      {
        id: 'smoke-certificate',
        serverNames: ['localhost'],
        source: {
          type: 'protected-file',
          certificateFile,
          privateKeyFile,
        },
      },
    ],
    tlsPolicies: [
      {
        id: 'smoke-tls',
        certificateRef: 'smoke-certificate',
        minimumVersion: 'tls1.2',
        maximumVersion: 'tls1.3',
        alpn: ['h2', 'http/1.1'],
      },
    ],
    resolvers: [],
    resources: [
      {
        id: 'smoke-response',
        type: 'respond',
        status: 200,
        contentType: 'text/plain; charset=utf-8',
        body: 'release-smoke\n',
      },
    ],
    upstreams: [],
    virtualHosts: [
      {
        id: 'smoke-host',
        listenerRefs: ['smoke-http', 'smoke-https'],
        serverNames: ['localhost'],
        routes: [
          {
            id: 'health',
            match: { pathType: 'exact', path: '/healthz', methods: ['GET', 'HEAD'] },
            resourceRef: 'smoke-response',
          },
        ],
      },
    ],
    observability: { accessLog: false },
    deployment: {
      drainTimeoutMs: 1_000,
      reload: { mode: 'disabled' },
    },
    metadata: { owner: 'sdkwork-release-smoke' },
  };
}

async function smoke(settings) {
  const resolved = resolveArtifact(settings);
  if (process.platform !== 'linux' || process.arch !== resolved.architecture) {
    throw new Error(
      `Linux ${resolved.architecture} release smoke must run on a linux-${resolved.architecture} host`,
    );
  }
  run(process.execPath, [
    'scripts/webserver-release.mjs',
    'validate',
    '--deployment-profile',
    settings.deploymentProfile,
    '--architecture',
    resolved.architecture,
    '--version',
    resolved.version,
  ]);

  const temporaryRoot = mkdtempSync(path.join(os.tmpdir(), 'sdkwork-webserver-release-smoke-'));
  let child;
  try {
    await extractTar({
      file: resolved.archive,
      cwd: temporaryRoot,
      strict: true,
      preservePaths: false,
    });
    const packageRoot = path.join(temporaryRoot, 'sdkwork-webserver');
    const binRoot = path.join(packageRoot, 'bin');
    for (const binary of EXPECTED_BINARIES) {
      const metadata = statSync(path.join(binRoot, binary));
      if (!metadata.isFile() || (metadata.mode & 0o111) === 0) {
        throw new Error(`packaged binary ${binary} is not an executable regular file`);
      }
    }

    const runtimeConfigFile = writeHermeticRuntimeConfig(temporaryRoot);
    const gateway = path.join(binRoot, 'sdkwork-api-webserver-standalone-gateway');
    const websiteEdgeRuntime = path.join(
      binRoot,
      'sdkwork-webserver-website-delivery-edge-runtime',
    );
    const packagedExample = path.join(packageRoot, 'etc', 'examples', 'sdkwork.webserver.config.json');
    const packagedWebsiteHostConfig = path.join(
      packageRoot,
      'etc',
      'data-plane',
      'website.cloud.config.json',
    );
    run(gateway, ['--help'], { cwd: packageRoot, env: hermeticEnv(runtimeConfigFile) });
    run(websiteEdgeRuntime, ['--help'], { cwd: packageRoot, env: hermeticEnv(runtimeConfigFile) });
    run(gateway, ['validate', packagedExample], {
      cwd: packageRoot,
      env: hermeticEnv(runtimeConfigFile),
    });
    run(websiteEdgeRuntime, ['validate', packagedWebsiteHostConfig], {
      cwd: packageRoot,
      env: hermeticEnv(runtimeConfigFile),
    });
    const pcStaticRoot = path.join(packageRoot, 'share', 'sdkwork', 'webserver-pc');
    const h5StaticRoot = path.join(packageRoot, 'share', 'sdkwork', 'webserver-h5');
    const staticFallbackRoot = path.join(packageRoot, 'share', 'sdkwork', 'webserver-static');
    let sameOriginPort;
    if (settings.deploymentProfile === 'standalone') {
      for (const [label, root] of [
        ['PC', pcStaticRoot],
        ['H5', h5StaticRoot],
      ]) {
        for (const bootstrapFile of ['index.html', 'runtime-env.json']) {
          const metadata = statSync(path.join(root, bootstrapFile));
          if (!metadata.isFile() || metadata.size === 0) {
            throw new Error(`packaged ${label} ${bootstrapFile} is not a non-empty regular file`);
          }
        }
      }
      const staticIndex = statSync(path.join(staticFallbackRoot, 'index.html'));
      if (!staticIndex.isFile() || staticIndex.size === 0) {
        throw new Error('packaged static-fallback index.html is not a non-empty regular file');
      }
      const assets = readdirSync(path.join(pcStaticRoot, 'assets'), { withFileTypes: true });
      if (!assets.some((entry) => entry.isFile())) {
        throw new Error('packaged PC app shell does not contain an assets/ file');
      }
      run(gateway, ['validate-app-shell'], {
        cwd: packageRoot,
        env: {
          ...hermeticEnv(runtimeConfigFile),
          SDKWORK_DEPLOYMENT_PROFILE: 'standalone',
          SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE: 'standalone',
          SDKWORK_WEBSERVER_ENVIRONMENT: 'production',
          SDKWORK_WEBSERVER_PC_STATIC_ROOT: STANDALONE_PC_ROOT,
          SDKWORK_WEBSERVER_H5_STATIC_ROOT: STANDALONE_H5_ROOT,
          SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT: STANDALONE_STATIC_FALLBACK_ROOT,
        },
      });
      sameOriginPort = await verifyStandaloneSameOriginIngress({
        gateway,
        packageRoot,
        temporaryRoot,
        runtimeConfigFile,
      });
    } else {
      throw new Error('--deployment-profile must be standalone (sdkwork-webserver is standalone-only)');
    }

    const certificateFile = path.join(temporaryRoot, 'smoke-cert.pem');
    const privateKeyFile = path.join(temporaryRoot, 'smoke-key.pem');
    run('openssl', [
      'req',
      '-x509',
      '-newkey',
      'rsa:2048',
      '-sha256',
      '-nodes',
      '-days',
      '1',
      '-subj',
      '/CN=localhost',
      '-addext',
      'subjectAltName=DNS:localhost',
      '-keyout',
      privateKeyFile,
      '-out',
      certificateFile,
    ]);

    const httpPort = await reservePort();
    let httpsPort = await reservePort();
    while (httpsPort === httpPort) {
      httpsPort = await reservePort();
    }
    const example = JSON.parse(readFileSync(packagedExample, 'utf8'));
    const smokeConfig = buildSmokeConfig(
      example,
      httpPort,
      httpsPort,
      certificateFile,
      privateKeyFile,
    );
    const smokeConfigPath = path.join(temporaryRoot, 'sdkwork.webserver.release-smoke.json');
    writeFileSync(smokeConfigPath, `${JSON.stringify(smokeConfig, null, 2)}\n`, {
      encoding: 'utf8',
      flag: 'wx',
      mode: 0o600,
    });
    run(gateway, ['validate', smokeConfigPath], {
      cwd: packageRoot,
      env: hermeticEnv(runtimeConfigFile),
    });

    child = spawn(gateway, ['data-plane', smokeConfigPath], {
      cwd: packageRoot,
      env: {
        ...hermeticEnv(runtimeConfigFile),
        RUST_LOG: 'info',
        SDKWORK_WEBSERVER_ENVIRONMENT: 'test',
        SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE: settings.deploymentProfile,
        SDKWORK_WEBSERVER_RUNTIME_TARGET: 'server',
      },
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
    });
    const readStdout = captureBounded(child.stdout);
    const readStderr = captureBounded(child.stderr);
    const readOutput = () => `${readStdout()}\n${readStderr()}`.trim();

    await waitForHealth('http', httpPort, child, readOutput);
    await waitForHealth('https', httpsPort, child, readOutput);
    child.kill('SIGTERM');
    let exit;
    try {
      exit = await waitForExit(child, STOP_TIMEOUT_MS);
    } catch (error) {
      child.kill('SIGKILL');
      throw error;
    }
    if (exit.code !== 0 || exit.signal !== null) {
      throw new Error(
        `packaged gateway exited unexpectedly code=${exit.code} signal=${exit.signal}: ${readOutput()}`,
      );
    }
    child = undefined;
    const sameOrigin = sameOriginPort
      ? ` sameOrigin=http://127.0.0.1:${sameOriginPort}`
      : '';
    console.log(
      `[sdkwork-webserver-release-smoke] passed artifact=${resolved.artifactBase}.tar.gz http=${httpPort} https=${httpsPort}${sameOrigin}`,
    );
  } finally {
    if (child && child.exitCode === null && child.signalCode === null) {
      child.kill('SIGKILL');
      await waitForExit(child, 5_000).catch(() => {});
    }
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
}

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    console.log(
      'Usage: node scripts/webserver-release-smoke.mjs --deployment-profile <standalone|cloud> [--architecture <x64|arm64>] [--version <semver>]',
    );
    return;
  }
  await smoke(settings);
}

main().catch((error) => {
  process.stderr.write(
    `[sdkwork-webserver-release-smoke] ${error instanceof Error ? error.message : String(error)}\n`,
  );
  process.exitCode = 1;
});
