#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  closeSync,
  copyFileSync,
  existsSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  readdirSync,
  readSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { list as listTar } from 'tar';

import { ensureTrackedBuildSources } from './lib/build-source-integrity.mjs';
import { resolveBrowserDistOutDir } from '../../sdkwork-specs/tools/browser-dist-layout.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// The staging directory must live on a filesystem with real permission bits
// (Linux ext4). On shared/Windows-mounted filesystems (WSL /mnt) the mode
// bits are not preserved; override with SDKWORK_RELEASE_STAGE_PARENT.
const STAGE_PARENT = process.env.SDKWORK_RELEASE_STAGE_PARENT
  ? path.resolve(process.env.SDKWORK_RELEASE_STAGE_PARENT)
  : path.join(REPO_ROOT, '.sdkwork', 'runtime', 'release-stage');const OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const MAX_ARCHIVE_BYTES = 512 * 1024 * 1024;
const MAX_PACKAGE_FILE_BYTES = 256 * 1024 * 1024;
const MAX_PACKAGE_CONTENT_BYTES = 1024 * 1024 * 1024;
const MAX_PACKAGE_ENTRIES = 2048;
const MAX_PC_STATIC_FILES = 256;
const MAX_DEPENDENCY_RUNTIME_FILES = 256;
const MAX_PC_BOOTSTRAP_FILE_BYTES = 4 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 256 * 1024;
const MAX_CHECKSUM_BYTES = 256;
const HASH_BUFFER_BYTES = 64 * 1024;
const PROCESS_OUTPUT_BYTES = 1024 * 1024;
const GIT_TIMEOUT_MS = 30 * 1000;
const TAR_TIMEOUT_MS = 5 * 60 * 1000;
const CARGO_BUILD_TIMEOUT_MS = 30 * 60 * 1000;
const PC_BUILD_TIMEOUT_MS = 15 * 60 * 1000;
const SBOM_TIMEOUT_MS = 3 * 60 * 1000;
const SUPPORTED_ARCHITECTURES = new Set(['x64', 'arm64']);

/** Lifecycle environment of the packaged frontend (test -> dist/test). */
function resolvedEnvironment(env = process.env) {
  const value = env.SDKWORK_WEBSERVER_ENVIRONMENT ?? env.SDKWORK_ENVIRONMENT ?? 'production';
  return ['development', 'test', 'staging', 'production'].includes(value) ? value : 'production';
}

const PC_APP_RELATIVE_ROOT = 'apps/sdkwork-webserver-pc';
const PC_APP_ROOT = path.join(REPO_ROOT, 'apps', 'sdkwork-webserver-pc');
const H5_APP_RELATIVE_ROOT = 'apps/sdkwork-webserver-h5';
const H5_APP_ROOT = path.join(REPO_ROOT, 'apps', 'sdkwork-webserver-h5');
const PC_PACKAGE_PREFIX = 'share/sdkwork/webserver-pc';
const PC_PACKAGE_INDEX = `${PC_PACKAGE_PREFIX}/index.html`;
const PC_PACKAGE_RUNTIME_ENV = `${PC_PACKAGE_PREFIX}/runtime-env.json`;
const PC_PACKAGE_ASSETS_PREFIX = `${PC_PACKAGE_PREFIX}/assets/`;
const H5_PACKAGE_PREFIX = 'share/sdkwork/webserver-h5';
const H5_PACKAGE_INDEX = `${H5_PACKAGE_PREFIX}/index.html`;
const H5_PACKAGE_RUNTIME_ENV = `${H5_PACKAGE_PREFIX}/runtime-env.json`;
const H5_PACKAGE_ASSETS_PREFIX = `${H5_PACKAGE_PREFIX}/assets/`;

/**
 * Browser build output root for one surface, deployment profile, and
 * environment (FRONTEND_CODE_SPEC.md §7): dist/<profile>/<envAlias>.
 * Resolved lazily so CLI --environment / --deployment-profile flags always win.
 */
function resolveBrowserBuildOutput(appRoot, settings) {
  const environment = settings.environment ?? resolvedEnvironment(process.env);
  const deploymentProfile = settings.deploymentProfile ?? 'standalone';
  return path.join(appRoot, resolveBrowserDistOutDir(environment, deploymentProfile));
}
const STATIC_FALLBACK_SOURCE = path.join(REPO_ROOT, 'deployments', 'webserver', 'static');
const STATIC_FALLBACK_PACKAGE_PREFIX = 'share/sdkwork/webserver-static';
const STATIC_FALLBACK_PACKAGE_INDEX = `${STATIC_FALLBACK_PACKAGE_PREFIX}/index.html`;
const DEPENDENCY_RUNTIME_ASSETS = Object.freeze([
  {
    id: 'iam',
    sourceRoot: path.resolve(REPO_ROOT, '..', 'sdkwork-iam'),
    sourceDirectories: ['database', 'iam'],
    packagePrefix: 'share/sdkwork/iam',
    requiredPaths: [
      'database/database.manifest.json',
      'iam/registry/iam-registry.config.json',
    ],
    requiredPrefix: 'iam/modules/',
    requiredSuffix: '/iam.module.manifest.json',
  },
  {
    id: 'drive',
    sourceRoot: path.resolve(REPO_ROOT, '..', 'sdkwork-drive'),
    sourceDirectories: ['database'],
    packagePrefix: 'share/sdkwork/drive',
    requiredPaths: ['database/database.manifest.json'],
  },
  {
    id: 'skills',
    sourceRoot: path.resolve(REPO_ROOT, '..', 'sdkwork-skills'),
    sourceDirectories: ['database', 'skills'],
    packagePrefix: 'share/sdkwork/skills',
    requiredPaths: ['database/database.manifest.json'],
  },
  {
    id: 'mcp',
    sourceRoot: path.resolve(REPO_ROOT, '..', 'sdkwork-mcp'),
    sourceDirectories: ['database'],
    packagePrefix: 'share/sdkwork/mcp',
    requiredPaths: ['database/database.manifest.json'],
  },
  {
    id: 'deploy',
    sourceRoot: path.resolve(REPO_ROOT, '..', 'sdkwork-deployments'),
    sourceDirectories: ['database'],
    packagePrefix: 'share/sdkwork/deploy',
    requiredPaths: ['database/database.manifest.json'],
  },
  {
    id: 'webstore',
    sourceRoot: path.resolve(REPO_ROOT, '..', 'sdkwork-web-framework'),
    sourceDirectories: ['database'],
    packagePrefix: 'share/sdkwork/webstore',
    requiredPaths: ['database/database.manifest.json'],
  },
]);
const DEPENDENCY_PACKAGE_PREFIXES = DEPENDENCY_RUNTIME_ASSETS.map(
  (dependency) => `${dependency.packagePrefix}/`,
);
const SDK_BASE_URL_FIELDS = [
  'appApiBaseUrl',
  'backendApiBaseUrl',
  'driveAppApiBaseUrl',
  'appbaseAppApiBaseUrl',
];
const BINARIES = [
  'sdkwork-api-webserver-standalone-gateway',
  'sdkwork-webserver-website-delivery-edge-runtime',
  'sdkwork-webserver-node-daemon',
  'sdkwork-webserver-agent',
  'sdkwork-webserver-certificate-worker',
];
const PACKAGE_ASSETS = [
  { source: 'sdkwork.app.config.json', target: 'sdkwork.app.config.json' },
  {
    source: 'specs/iam.module.manifest.json',
    target: 'specs/iam.module.manifest.json',
  },
  {
    source: 'specs/sdkwork.webserver.config.schema.json',
    target: 'specs/sdkwork.webserver.config.schema.json',
  },
  {
    source: 'etc/examples/sdkwork.webserver.config.json',
    target: 'etc/examples/sdkwork.webserver.config.json',
  },
  {
    source: 'etc/examples/public/index.html',
    target: 'etc/examples/public/index.html',
  },
  {
    source: 'etc/data-plane/website.cloud.config.json',
    target: 'etc/data-plane/website.cloud.config.json',
  },
  {
    source: 'etc/node-daemon/development.env.example',
    target: 'etc/node-daemon/development.env.example',
  },
  { source: 'database/README.md', target: 'database/README.md' },
  {
    source: 'database/database.manifest.json',
    target: 'database/database.manifest.json',
  },
  {
    source: 'database/contract/prefix-registry.json',
    target: 'database/contract/prefix-registry.json',
  },
  {
    source: 'database/contract/schema.yaml',
    target: 'database/contract/schema.yaml',
  },
  {
    source: 'database/contract/table-registry.json',
    target: 'database/contract/table-registry.json',
  },
  {
    source: 'database/ddl/baseline/postgres/0001_web_baseline.sql',
    target: 'database/ddl/baseline/postgres/0001_web_baseline.sql',
  },
  {
    source: 'database/migrations/postgres/0005_web_application.up.sql',
    target: 'database/migrations/postgres/0005_web_application.up.sql',
  },
  {
    source: 'database/migrations/postgres/0006_organization_id_not_null.up.sql',
    target: 'database/migrations/postgres/0006_organization_id_not_null.up.sql',
  },
  {
    source: 'database/drift/policy.yaml',
    target: 'database/drift/policy.yaml',
  },
  {
    source: 'database/seeds/seed.manifest.json',
    target: 'database/seeds/seed.manifest.json',
  },
  {
    source: 'database/seeds/common/001_bootstrap.sql',
    target: 'database/seeds/common/001_bootstrap.sql',
  },
];
const EXPECTED_FIXED_CONTENT_PATHS = [
  ...BINARIES.map((binary) => `bin/${binary}`),
  ...PACKAGE_ASSETS.map((asset) => asset.target),
].sort();

function archiveDirectoriesFor(contentPaths) {
  return Array.from(
    new Set(
      contentPaths.flatMap((contentPath) => {
        const segments = contentPath.split('/');
        return segments.slice(0, -1).map((_, index) =>
          ['sdkwork-webserver', ...segments.slice(0, index + 1)].join('/'),
        );
      }).concat('sdkwork-webserver'),
    ),
  ).sort();
}

function parseArgs(argv) {
  const settings = {
    operation: argv[0],
    deploymentProfile: process.env.SDKWORK_DEPLOYMENT_PROFILE,
    environment: process.env.SDKWORK_WEBSERVER_ENVIRONMENT ?? process.env.SDKWORK_ENVIRONMENT ?? 'production',
    architecture: process.env.SDKWORK_PACKAGE_ARCHITECTURE,
    version: undefined,
    dryRun: false,
    skipPcBuild: false,
    skipH5Build: false,
  };
  for (let index = 1; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--deployment-profile') {
      settings.deploymentProfile = argv[++index];
    } else if (argument === '--environment') {
      settings.environment = argv[++index];
      process.env.SDKWORK_WEBSERVER_ENVIRONMENT = settings.environment;
      process.env.SDKWORK_ENVIRONMENT = settings.environment;
    } else if (argument === '--architecture') {
      settings.architecture = argv[++index];
    } else if (argument === '--version') {
      settings.version = argv[++index];
    } else if (argument === '--dry-run') {
      settings.dryRun = true;
    } else if (argument === '--skip-pc-build') {
      // Reuse an existing PC static build (apps/sdkwork-webserver-pc/dist/<profile>/prod)
      // instead of rebuilding it on this runner. The dist output is
      // platform-independent; used when the runner has no matching Node
      // toolchain (for example a WSL packaging runner).
      settings.skipPcBuild = true;
    } else if (argument === '--skip-h5-build') {
      // Reuse an existing H5 static build (apps/sdkwork-webserver-h5/dist/<profile>/prod).
      settings.skipH5Build = true;
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  return settings;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    env: options.env ?? process.env,
    stdio: options.capture ? 'pipe' : 'inherit',
    timeout: options.timeoutMs,
    maxBuffer: PROCESS_OUTPUT_BYTES,
    killSignal: 'SIGKILL',
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message ?? result.stderr?.trim() ?? `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result;
}

function assertExactKeys(value, keys, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw new Error(`${label} must contain exactly: ${expected.join(', ')}`);
  }
}

function assertSafeOwnedPath(candidate, owner, label) {
  const relative = path.relative(owner, candidate);
  if (relative.startsWith('..') || path.isAbsolute(relative) || relative === '') {
    throw new Error(`${label} is outside its owned directory`);
  }
}

function assertRegularOrMissing(filePath, label) {
  if (!existsSync(filePath)) {
    return;
  }
  const stat = lstatSync(filePath);
  if (stat.isSymbolicLink() || !stat.isFile()) {
    throw new Error(`${label} must be a regular non-symlink file`);
  }
}

function inspectRegularFile(filePath, label, maxBytes = MAX_PACKAGE_FILE_BYTES) {
  const linkStat = lstatSync(filePath);
  const stat = statSync(filePath);
  if (linkStat.isSymbolicLink() || !linkStat.isFile() || !stat.isFile()) {
    throw new Error(`${label} must be a regular non-symlink file`);
  }
  if (!Number.isSafeInteger(stat.size) || stat.size < 0 || stat.size > maxBytes) {
    throw new Error(`${label} must be within 0..=${maxBytes} bytes`);
  }
  return stat;
}

function syncFile(filePath) {
  const descriptor = openSync(filePath, 'r');
  try {
    fsyncSync(descriptor);
  } finally {
    closeSync(descriptor);
  }
}

function syncDirectory(directoryPath) {
  if (process.platform === 'win32') {
    return;
  }
  const descriptor = openSync(directoryPath, 'r');
  try {
    fsyncSync(descriptor);
  } finally {
    closeSync(descriptor);
  }
}

function writeAtomicText(filePath, content) {
  assertRegularOrMissing(filePath, `output ${filePath}`);
  const temporaryPath = `${filePath}.tmp-${process.pid}`;
  rmSync(temporaryPath, { force: true });
  try {
    writeFileSync(temporaryPath, content, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
    syncFile(temporaryPath);
    renameSync(temporaryPath, filePath);
    syncDirectory(path.dirname(filePath));
  } finally {
    rmSync(temporaryPath, { force: true });
  }
}

function sha256File(filePath) {
  const descriptor = openSync(filePath, 'r');
  const buffer = Buffer.allocUnsafe(HASH_BUFFER_BYTES);
  const hash = createHash('sha256');
  try {
    while (true) {
      const bytesRead = readSync(descriptor, buffer, 0, buffer.length, null);
      if (bytesRead === 0) {
        break;
      }
      hash.update(buffer.subarray(0, bytesRead));
    }
  } finally {
    closeSync(descriptor);
  }
  return hash.digest('hex');
}

function readSmallText(filePath, maxBytes, label) {
  const stat = inspectRegularFile(filePath, label, maxBytes);
  if (stat.size === 0) {
    throw new Error(`${label} must not be empty`);
  }
  return readFileSync(filePath, 'utf8');
}

function ensureCriticalSources() {
  const relativePaths = [
    'Cargo.toml',
    'scripts/webserver-sbom.mjs',
    ...PACKAGE_ASSETS.map((asset) => asset.source),
  ];
  ensureTrackedBuildSources({ repoRoot: REPO_ROOT, relativePaths });
  for (const relativePath of relativePaths) {
    const absolutePath = path.join(REPO_ROOT, relativePath);
    inspectRegularFile(absolutePath, `package source ${relativePath}`);
  }
}

function resolveVersion(settings) {
  const manifestPath = path.join(REPO_ROOT, 'sdkwork.app.config.json');
  const manifestText = readSmallText(manifestPath, MAX_MANIFEST_BYTES, 'application manifest');
  const manifest = JSON.parse(manifestText);
  const packageVersion = process.env.SDKWORK_PACKAGE_VERSION?.trim();
  const compatibilityVersion = process.env.SDKWORK_RELEASE_VERSION?.trim();
  if (packageVersion && compatibilityVersion && packageVersion !== compatibilityVersion) {
    throw new Error('SDKWORK_PACKAGE_VERSION conflicts with SDKWORK_RELEASE_VERSION');
  }
  const version = settings.version || packageVersion || compatibilityVersion || manifest.release?.currentVersion;
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/u.test(version ?? '')) {
    throw new Error('release version must be an explicit semantic version');
  }
  return version;
}

function resolveArchitecture(settings) {
  const architecture = settings.architecture?.trim() || process.arch;
  if (!SUPPORTED_ARCHITECTURES.has(architecture)) {
    throw new Error('release architecture must be x64 or arm64');
  }
  return architecture;
}

function resolveArtifact(settings) {
  if (settings.deploymentProfile !== 'standalone') {
    // sdkwork-webserver is standalone-only (SDKWORK_WEBSERVER_SPEC.md §17.4).
    throw new Error('--deployment-profile must be standalone');
  }
  const version = resolveVersion(settings);
  const architecture = resolveArchitecture(settings);
  const artifactBase = `sdkwork-webserver-linux-${architecture}-${settings.deploymentProfile}-server-${version}`;
  const archive = path.join(OUTPUT_ROOT, `${artifactBase}.tar.gz`);
  assertSafeOwnedPath(archive, OUTPUT_ROOT, 'release archive');
  return { version, architecture, artifactBase, archive };
}

function resolveCargoTargetRoot() {
  const configured = process.env.CARGO_TARGET_DIR?.trim();
  if (!configured) {
    return path.join(REPO_ROOT, 'target');
  }
  return path.isAbsolute(configured)
    ? path.normalize(configured)
    : path.resolve(REPO_ROOT, configured);
}

function copyPackageAsset(asset, stageRoot) {
  const source = path.join(REPO_ROOT, asset.source);
  inspectRegularFile(source, `package source ${asset.source}`);
  const target = path.join(stageRoot, asset.target);
  assertSafeOwnedPath(target, stageRoot, `package target ${asset.target}`);
  mkdirSync(path.dirname(target), { recursive: true, mode: 0o755 });
  copyFileSync(source, target);
  chmodSync(target, 0o644);
  return target;
}

function normalizePackageContentPath(value, label) {
  if (typeof value !== 'string' || value.length === 0 || value.length > 500) {
    throw new Error(`${label} must contain 1..=500 characters`);
  }
  if (value.includes('\\') || value.includes('\0') || value.startsWith('/')) {
    throw new Error(`${label} is unsafe: ${JSON.stringify(value)}`);
  }
  const segments = value.split('/');
  if (
    [...value].some((character) => character.charCodeAt(0) <= 0x1f || character === '\u007f') ||
    segments.some((segment) => segment === '' || segment === '.' || segment === '..') ||
    path.posix.normalize(value) !== value
  ) {
    throw new Error(`${label} is unsafe: ${JSON.stringify(value)}`);
  }
  return value;
}

function resolveCloudApiBaseUrl() {
  const deploymentIndex = path.join(REPO_ROOT, 'etc', 'sdkwork.deployment.config.json');
  const deployment = JSON.parse(readFileSync(deploymentIndex, 'utf8'));
  const environments = deployment.environments ?? {};
  const result = {};
  for (const environment of ['development', 'test', 'staging', 'production']) {
    const value = environments[environment]?.cloudApiBaseUrl;
    if (typeof value !== 'string' || value.length === 0) {
      throw new Error(
        `etc/sdkwork.deployment.config.json must declare environments.${environment}.cloudApiBaseUrl`,
      );
    }
    result[environment] = new URL(value).origin;
  }
  return result;
}

function validateBrowserRuntimeEnv(value, label, deploymentProfile) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} must contain a JSON object`);
  }
  const environment = resolvedEnvironment(process.env);
  const profileId = `${deploymentProfile}.${environment}`;
  for (const [field, expected] of [
    ['environment', environment],
    ['deploymentProfile', deploymentProfile],
    ['profileId', profileId],
    ['runtimeTarget', 'browser'],
    ['browserOriginMode', deploymentProfile === 'standalone' ? 'same-origin' : 'cross-origin'],
  ]) {
    if (value[field] !== expected) {
      throw new Error(`${label}.${field} must equal ${expected}`);
    }
  }
  if (deploymentProfile === 'standalone') {
    for (const field of SDK_BASE_URL_FIELDS) {
      if (value[field] !== '/') {
        throw new Error(`${label}.${field} must use the canonical same-origin root /`);
      }
    }
    return;
  }
  const cloudApiBaseUrl = resolveCloudApiBaseUrl()[environment];
  for (const field of SDK_BASE_URL_FIELDS) {
    const raw = String(value[field] ?? '').trim();
    if (!raw) {
      throw new Error(`${label}.${field} must be an absolute HTTP(S) URL`);
    }
    let origin;
    try {
      origin = new URL(raw).origin;
    } catch {
      throw new Error(`${label}.${field} must be an absolute HTTP(S) URL`);
    }
    if (origin !== cloudApiBaseUrl) {
      throw new Error(
        `${label}.${field} must equal the unified cloud API edge ${cloudApiBaseUrl} (ENVIRONMENT_SPEC §5.1.0.1), not ${origin}`,
      );
    }
  }
}

function decodeUtf8(bytes, label) {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    throw new Error(`${label} must be valid UTF-8`);
  }
}

function validateBrowserBootstrapFiles(indexBytes, runtimeEnvBytes, label, deploymentProfile) {
  if (!indexBytes || indexBytes.length === 0) {
    throw new Error(`${label} is missing index.html`);
  }
  if (!runtimeEnvBytes || runtimeEnvBytes.length === 0) {
    throw new Error(`${label} is missing runtime-env.json`);
  }
  if (
    indexBytes.length > MAX_PC_BOOTSTRAP_FILE_BYTES ||
    runtimeEnvBytes.length > MAX_PC_BOOTSTRAP_FILE_BYTES
  ) {
    throw new Error(`${label} bootstrap files exceed ${MAX_PC_BOOTSTRAP_FILE_BYTES} bytes`);
  }
  const index = decodeUtf8(indexBytes, `${label} index.html`);
  const lowerIndex = index.toLowerCase();
  if (!index.trim() || (!lowerIndex.includes('<!doctype html') && !lowerIndex.includes('<html'))) {
    throw new Error(`${label} index.html must contain an HTML document`);
  }
  let runtimeEnv;
  try {
    runtimeEnv = JSON.parse(decodeUtf8(runtimeEnvBytes, `${label} runtime-env.json`));
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(`${label} runtime-env.json must contain valid JSON`);
    }
    throw error;
  }
  validateBrowserRuntimeEnv(runtimeEnv, `${label} runtime-env.json`, deploymentProfile);
}

function inspectSpaBuildOutput({
  buildOutput,
  packagePrefix,
  label,
  maxFiles = MAX_PC_STATIC_FILES,
  deploymentProfile = 'standalone',
}) {
  const rootMetadata = lstatSync(buildOutput);
  if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) {
    throw new Error(`${label} ${deploymentProfile} build output must be a non-symlink directory`);
  }
  const files = [];
  let inspectedEntries = 0;
  const walk = (directory, relativeDirectory) => {
    const entries = readdirSync(directory, { withFileTypes: true }).sort((left, right) =>
      left.name < right.name ? -1 : left.name > right.name ? 1 : 0,
    );
    for (const entry of entries) {
      inspectedEntries += 1;
      if (inspectedEntries > MAX_PACKAGE_ENTRIES) {
        throw new Error(`${label} build contains more than ${MAX_PACKAGE_ENTRIES} filesystem entries`);
      }
      const source = path.join(directory, entry.name);
      assertSafeOwnedPath(source, buildOutput, `${label} build entry ${entry.name}`);
      const metadata = lstatSync(source);
      if (metadata.isSymbolicLink()) {
        throw new Error(`${label} build entry ${source} must not be a symbolic link`);
      }
      const relative = relativeDirectory ? `${relativeDirectory}/${entry.name}` : entry.name;
      normalizePackageContentPath(relative, `${label} build path ${relative}`);
      if (metadata.isDirectory()) {
        walk(source, relative);
      } else if (metadata.isFile()) {
        inspectRegularFile(source, `${label} build file ${relative}`);
        files.push({
          source,
          target: `${packagePrefix}/${relative}`,
          relative,
        });
        if (files.length > maxFiles) {
          throw new Error(`${label} build contains more than ${maxFiles} files`);
        }
      } else {
        throw new Error(`${label} build entry ${source} must be a regular file or directory`);
      }
    }
  };
  walk(buildOutput, '');

  const index = files.find((file) => file.relative === 'index.html');
  const runtimeEnv = files.find((file) => file.relative === 'runtime-env.json');
  validateBrowserBootstrapFiles(
    index ? readFileSync(index.source) : undefined,
    runtimeEnv ? readFileSync(runtimeEnv.source) : undefined,
    `${label} ${deploymentProfile} build`,
    deploymentProfile,
  );
  if (!files.some((file) => file.relative.startsWith('assets/'))) {
    throw new Error(`${label} ${deploymentProfile} build must contain at least one assets/ file`);
  }
  return files;
}

/** Browser build source and packaged target for one surface + profile. */
function resolveBrowserSurface({ appRoot, relativeRoot, packagePrefix, label, settings }) {
  const buildOutput = resolveBrowserBuildOutput(appRoot, settings);
  return {
    buildOutput,
    label,
    packagePrefix,
    relativeRoot,
  };
}

function inspectPcBuildOutput(settings) {
  return inspectSpaBuildOutput({
    ...resolveBrowserSurface({
      appRoot: PC_APP_ROOT,
      packagePrefix: PC_PACKAGE_PREFIX,
      label: 'PC',
      settings,
    }),
    deploymentProfile: settings.deploymentProfile,
  });
}

function inspectH5BuildOutput(settings) {
  return inspectSpaBuildOutput({
    ...resolveBrowserSurface({
      appRoot: H5_APP_ROOT,
      packagePrefix: H5_PACKAGE_PREFIX,
      label: 'H5',
      settings,
    }),
    deploymentProfile: settings.deploymentProfile,
  });
}

function inspectStaticFallbackAssets() {
  const rootMetadata = lstatSync(STATIC_FALLBACK_SOURCE);
  if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) {
    throw new Error('static-fallback source must be a non-symlink directory');
  }
  const files = [];
  const walk = (directory, relativeDirectory) => {
    const entries = readdirSync(directory, { withFileTypes: true }).sort((left, right) =>
      left.name < right.name ? -1 : left.name > right.name ? 1 : 0,
    );
    for (const entry of entries) {
      if (entry.name === 'README.md') continue;
      const source = path.join(directory, entry.name);
      assertSafeOwnedPath(source, STATIC_FALLBACK_SOURCE, `static-fallback entry ${entry.name}`);
      const metadata = lstatSync(source);
      if (metadata.isSymbolicLink()) {
        throw new Error(`static-fallback entry ${source} must not be a symbolic link`);
      }
      const relative = relativeDirectory ? `${relativeDirectory}/${entry.name}` : entry.name;
      if (metadata.isDirectory()) {
        walk(source, relative);
      } else if (metadata.isFile()) {
        inspectRegularFile(source, `static-fallback file ${relative}`);
        files.push({
          source,
          target: `${STATIC_FALLBACK_PACKAGE_PREFIX}/${relative}`,
          relative,
        });
      } else {
        throw new Error(`static-fallback entry ${source} must be a regular file or directory`);
      }
    }
  };
  walk(STATIC_FALLBACK_SOURCE, '');
  if (!files.some((file) => file.relative === 'index.html')) {
    throw new Error('static-fallback source must contain index.html');
  }
  return files;
}

function copyPackagedStaticFile(file, stageRoot, label) {
  const target = path.join(stageRoot, ...file.target.split('/'));
  assertSafeOwnedPath(target, stageRoot, `${label} package target ${file.target}`);
  mkdirSync(path.dirname(target), { recursive: true, mode: 0o755 });
  copyFileSync(file.source, target);
  chmodSync(target, 0o644);
  return target;
}

function copyPcStaticFile(file, stageRoot) {
  const target = path.join(stageRoot, ...file.target.split('/'));
  assertSafeOwnedPath(target, stageRoot, `PC package target ${file.target}`);
  mkdirSync(path.dirname(target), { recursive: true, mode: 0o755 });
  copyFileSync(file.source, target);
  chmodSync(target, 0o644);
  return target;
}

function inspectDependencyRuntimeAssets() {
  const files = [];
  for (const dependency of DEPENDENCY_RUNTIME_ASSETS) {
    for (const sourceDirectory of dependency.sourceDirectories) {
      const sourceRoot = path.join(dependency.sourceRoot, sourceDirectory);
      const rootMetadata = lstatSync(sourceRoot);
      if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) {
        throw new Error(
          `${dependency.id} runtime source ${sourceDirectory} must be a non-symlink directory`,
        );
      }
      const walk = (directory, relativeDirectory) => {
        const entries = readdirSync(directory, { withFileTypes: true }).sort((left, right) =>
          left.name < right.name ? -1 : left.name > right.name ? 1 : 0,
        );
        for (const entry of entries) {
          const source = path.join(directory, entry.name);
          assertSafeOwnedPath(
            source,
            dependency.sourceRoot,
            `${dependency.id} runtime source ${entry.name}`,
          );
          const metadata = lstatSync(source);
          if (metadata.isSymbolicLink()) {
            throw new Error(`${dependency.id} runtime source ${source} must not be a symbolic link`);
          }
          const relative = relativeDirectory
            ? `${relativeDirectory}/${entry.name}`
            : `${sourceDirectory}/${entry.name}`;
          normalizePackageContentPath(relative, `${dependency.id} runtime path ${relative}`);
          if (metadata.isDirectory()) {
            walk(source, relative);
          } else if (metadata.isFile()) {
            inspectRegularFile(source, `${dependency.id} runtime file ${relative}`);
            files.push({
              dependencyId: dependency.id,
              source,
              target: `${dependency.packagePrefix}/${relative}`,
              relative,
            });
            if (files.length > MAX_DEPENDENCY_RUNTIME_FILES) {
              throw new Error(
                `dependency runtime assets contain more than ${MAX_DEPENDENCY_RUNTIME_FILES} files`,
              );
            }
          } else {
            throw new Error(
              `${dependency.id} runtime source ${source} must be a regular file or directory`,
            );
          }
        }
      };
      walk(sourceRoot, '');
    }

    const dependencyFiles = files.filter((file) => file.dependencyId === dependency.id);
    for (const requiredPath of dependency.requiredPaths) {
      if (!dependencyFiles.some((file) => file.relative === requiredPath)) {
        throw new Error(`${dependency.id} runtime assets are missing ${requiredPath}`);
      }
    }
    if (
      dependency.requiredPrefix
      && !dependencyFiles.some((file) => (
        file.relative.startsWith(dependency.requiredPrefix)
          && file.relative.endsWith(dependency.requiredSuffix)
      ))
    ) {
      throw new Error(`${dependency.id} runtime assets are missing module manifests`);
    }
  }
  return files;
}

function copyDependencyRuntimeFile(file, stageRoot) {
  const target = path.join(stageRoot, ...file.target.split('/'));
  assertSafeOwnedPath(target, stageRoot, `dependency runtime package target ${file.target}`);
  mkdirSync(path.dirname(target), { recursive: true, mode: 0o755 });
  copyFileSync(file.source, target);
  chmodSync(target, 0o644);
  return target;
}

function normalizeArchivePath(value) {
  if (typeof value !== 'string' || value.length === 0 || value.length > 512) {
    throw new Error('archive entry path must contain 1..=512 characters');
  }
  if (value.includes('\\') || value.includes('\0') || value.startsWith('/')) {
    throw new Error(`unsafe archive entry path: ${JSON.stringify(value)}`);
  }
  const normalized = value.endsWith('/') ? value.slice(0, -1) : value;
  const segments = normalized.split('/');
  if (
    normalized.length === 0 ||
    segments.some((segment) => segment === '' || segment === '.' || segment === '..') ||
    path.posix.normalize(normalized) !== normalized ||
    !normalized.startsWith('sdkwork-webserver') ||
    (normalized !== 'sdkwork-webserver' && !normalized.startsWith('sdkwork-webserver/'))
  ) {
    throw new Error(`unsafe archive entry path: ${JSON.stringify(value)}`);
  }
  return normalized;
}

async function inspectArchiveEntries(archive) {
  const records = new Map();
  const capturedBuffers = new Map();
  const order = [];
  const entryCompletions = [];
  let manifestBuffer;
  let declaredBytes = 0;
  let validationError;
  const fail = (error) => {
    if (!validationError) {
      validationError = error instanceof Error ? error : new Error(String(error));
    }
  };

  await listTar({
    file: archive,
    strict: true,
    noResume: true,
    maxReadSize: HASH_BUFFER_BYTES,
    onReadEntry(entry) {
      try {
        if (records.size >= MAX_PACKAGE_ENTRIES) {
          throw new Error(`archive contains more than ${MAX_PACKAGE_ENTRIES} entries`);
        }
        const entryPath = normalizeArchivePath(entry.path);
        if (records.has(entryPath)) {
          throw new Error(`archive contains duplicate entry ${entryPath}`);
        }
        if (entry.meta || entry.invalid || entry.unsupported || entry.linkpath) {
          throw new Error(`archive entry ${entryPath} uses unsupported metadata or links`);
        }
        if (!['File', 'Directory'].includes(entry.type)) {
          throw new Error(`archive entry ${entryPath} has unsupported type ${entry.type}`);
        }
        if (entry.uid !== 0 || entry.gid !== 0) {
          throw new Error(`archive entry ${entryPath} must use uid/gid 0`);
        }
        const mode = (entry.mode ?? 0) & 0o7777;
        if ((mode & 0o022) !== 0) {
          throw new Error(`archive entry ${entryPath} must not be group/world writable`);
        }
        if (!(entry.mtime instanceof Date) || !Number.isFinite(entry.mtime.getTime())) {
          throw new Error(`archive entry ${entryPath} must have a valid mtime`);
        }
        if (!Number.isSafeInteger(entry.size) || entry.size < 0) {
          throw new Error(`archive entry ${entryPath} has an invalid size`);
        }
        if (entry.type === 'Directory' && entry.size !== 0) {
          throw new Error(`archive directory ${entryPath} must be empty metadata`);
        }
        if (entry.type === 'File' && entry.size > MAX_PACKAGE_FILE_BYTES) {
          throw new Error(`archive file ${entryPath} exceeds ${MAX_PACKAGE_FILE_BYTES} bytes`);
        }
        declaredBytes += entry.size;
        if (declaredBytes > MAX_PACKAGE_CONTENT_BYTES) {
          throw new Error(`archive content exceeds ${MAX_PACKAGE_CONTENT_BYTES} bytes`);
        }

        const record = {
          path: entryPath,
          type: entry.type,
          size: entry.size,
          mode,
          uid: entry.uid,
          gid: entry.gid,
          mtimeSeconds: Math.floor(entry.mtime.getTime() / 1000),
        };
        records.set(entryPath, record);
        order.push(entryPath);
        if (entry.type === 'Directory') {
          entry.resume();
          return;
        }

        const hash = createHash('sha256');
        let actualBytes = 0;
        const captureLimit = entryPath === 'sdkwork-webserver/package.manifest.json'
          ? MAX_MANIFEST_BYTES
          : [PC_PACKAGE_INDEX, PC_PACKAGE_RUNTIME_ENV, H5_PACKAGE_INDEX, H5_PACKAGE_RUNTIME_ENV]
              .map((item) => `sdkwork-webserver/${item}`)
              .includes(entryPath)
            ? MAX_PC_BOOTSTRAP_FILE_BYTES
            : undefined;
        const capturedChunks = [];
        const completion = new Promise((resolve, reject) => {
          entry.on('data', (chunk) => {
            actualBytes += chunk.length;
            if (actualBytes > entry.size || actualBytes > MAX_PACKAGE_FILE_BYTES) {
              fail(new Error(`archive file ${entryPath} exceeds its declared bound`));
              return;
            }
            hash.update(chunk);
            if (captureLimit !== undefined) {
              if (actualBytes > captureLimit) {
                fail(new Error(`archive file ${entryPath} exceeds ${captureLimit} bytes`));
                return;
              }
              capturedChunks.push(Buffer.from(chunk));
            }
          });
          entry.once('error', reject);
          entry.once('end', () => {
            record.actualBytes = actualBytes;
            record.sha256 = hash.digest('hex');
            if (captureLimit !== undefined) {
              const captured = Buffer.concat(capturedChunks, actualBytes);
              capturedBuffers.set(entryPath, captured);
              if (entryPath === 'sdkwork-webserver/package.manifest.json') {
                manifestBuffer = captured;
              }
            }
            resolve();
          });
        });
        entryCompletions.push(completion);
      } catch (error) {
        fail(error);
        entry.resume();
      }
    },
  });
  await Promise.all(entryCompletions);
  if (validationError) {
    throw validationError;
  }
  return { records, order, manifestBuffer, capturedBuffers };
}

function validatePackageManifest(manifestBuffer, records, order, capturedBuffers, expected) {
  if (!manifestBuffer || manifestBuffer.length === 0) {
    throw new Error('archive is missing package.manifest.json');
  }
  let manifestText;
  try {
    manifestText = new TextDecoder('utf-8', { fatal: true }).decode(manifestBuffer);
  } catch {
    throw new Error('package manifest must be valid UTF-8');
  }
  if (!manifestText.endsWith('\n')) {
    throw new Error('package manifest must end with a newline');
  }
  const manifest = JSON.parse(manifestText);
  assertExactKeys(
    manifest,
    [
      'schemaVersion',
      'kind',
      'application',
      'version',
      'deploymentProfile',
      'runtimeTarget',
      'platform',
      'architecture',
      'sourceDateEpoch',
      'content',
    ],
    'package manifest',
  );
  if (
    manifest.schemaVersion !== 1 ||
    manifest.kind !== 'sdkwork.server-package' ||
    manifest.application !== 'sdkwork-web' ||
    manifest.version !== expected.version ||
    manifest.deploymentProfile !== expected.deploymentProfile ||
    manifest.runtimeTarget !== 'server' ||
    manifest.platform !== 'linux' ||
    manifest.architecture !== expected.architecture
  ) {
    throw new Error('package manifest identity does not match the selected artifact');
  }
  if (!Number.isSafeInteger(manifest.sourceDateEpoch) || manifest.sourceDateEpoch < 0) {
    throw new Error('package manifest sourceDateEpoch must be a non-negative safe integer');
  }
  if (
    !Array.isArray(manifest.content) ||
    manifest.content.length < EXPECTED_FIXED_CONTENT_PATHS.length ||
    manifest.content.length
      > EXPECTED_FIXED_CONTENT_PATHS.length
        + MAX_PC_STATIC_FILES
        + MAX_DEPENDENCY_RUNTIME_FILES
  ) {
    throw new Error('package manifest file count is outside the deployment-profile contract');
  }

  const manifestPaths = [];
  for (const [index, item] of manifest.content.entries()) {
    assertExactKeys(item, ['path', 'bytes', 'sha256'], `package manifest content[${index}]`);
    if (
      typeof item.path !== 'string' ||
      !Number.isSafeInteger(item.bytes) ||
      item.bytes < 0 ||
      item.bytes > MAX_PACKAGE_FILE_BYTES ||
      !/^[a-f0-9]{64}$/u.test(item.sha256)
    ) {
      throw new Error(`package manifest content[${index}] is invalid`);
    }
    normalizePackageContentPath(item.path, `package manifest content[${index}].path`);
    manifestPaths.push(item.path);
    const record = records.get(`sdkwork-webserver/${item.path}`);
    if (
      !record ||
      record.type !== 'File' ||
      record.size !== item.bytes ||
      record.actualBytes !== item.bytes ||
      record.sha256 !== item.sha256
    ) {
      throw new Error(`package content does not match manifest for ${item.path}`);
    }
  }
  const fixedPaths = new Set(EXPECTED_FIXED_CONTENT_PATHS);
  const pcPaths = manifestPaths.filter((item) => item.startsWith(`${PC_PACKAGE_PREFIX}/`));
  const h5Paths = manifestPaths.filter((item) => item.startsWith(`${H5_PACKAGE_PREFIX}/`));
  const staticFallbackPaths = manifestPaths.filter((item) => (
    item.startsWith(`${STATIC_FALLBACK_PACKAGE_PREFIX}/`)
  ));
  const dependencyPaths = manifestPaths.filter((item) => (
    DEPENDENCY_PACKAGE_PREFIXES.some((prefix) => item.startsWith(prefix))
  ));
  const unexpectedPaths = manifestPaths.filter(
    (item) => (
      !fixedPaths.has(item)
        && !item.startsWith(`${PC_PACKAGE_PREFIX}/`)
        && !item.startsWith(`${H5_PACKAGE_PREFIX}/`)
        && !item.startsWith(`${STATIC_FALLBACK_PACKAGE_PREFIX}/`)
        && !DEPENDENCY_PACKAGE_PREFIXES.some((prefix) => item.startsWith(prefix))
    ),
  );
  if (unexpectedPaths.length > 0) {
    throw new Error(`package manifest contains unsupported files: ${unexpectedPaths.join(', ')}`);
  }
  // sdkwork-webserver is standalone-only (SDKWORK_WEBSERVER_SPEC.md §17.4):
  // every release packages the same-origin PC/H5 bundles.
  {
    if (pcPaths.length === 0 || pcPaths.length > MAX_PC_STATIC_FILES) {
      throw new Error(`standalone package must contain 1..=${MAX_PC_STATIC_FILES} PC files`);
    }
    if (!pcPaths.includes(PC_PACKAGE_INDEX) || !pcPaths.includes(PC_PACKAGE_RUNTIME_ENV)) {
      throw new Error('standalone package must contain PC index.html and runtime-env.json');
    }
    if (!pcPaths.some((item) => item.startsWith(PC_PACKAGE_ASSETS_PREFIX))) {
      throw new Error('standalone package must contain at least one PC assets/ file');
    }
    validateBrowserBootstrapFiles(
      capturedBuffers.get(`sdkwork-webserver/${PC_PACKAGE_INDEX}`),
      capturedBuffers.get(`sdkwork-webserver/${PC_PACKAGE_RUNTIME_ENV}`),
      'standalone package PC',
      'standalone',
    );
    if (h5Paths.length === 0 || h5Paths.length > MAX_PC_STATIC_FILES) {
      throw new Error(`standalone package must contain 1..=${MAX_PC_STATIC_FILES} H5 files`);
    }
    if (!h5Paths.includes(H5_PACKAGE_INDEX) || !h5Paths.includes(H5_PACKAGE_RUNTIME_ENV)) {
      throw new Error('standalone package must contain H5 index.html and runtime-env.json');
    }
    if (!h5Paths.some((item) => item.startsWith(H5_PACKAGE_ASSETS_PREFIX))) {
      throw new Error('standalone package must contain at least one H5 assets/ file');
    }
    validateBrowserBootstrapFiles(
      capturedBuffers.get(`sdkwork-webserver/${H5_PACKAGE_INDEX}`),
      capturedBuffers.get(`sdkwork-webserver/${H5_PACKAGE_RUNTIME_ENV}`),
      'standalone package H5',
      'standalone',
    );
    if (staticFallbackPaths.length === 0) {
      throw new Error('standalone package must contain static-fallback assets');
    }
    if (!staticFallbackPaths.includes(STATIC_FALLBACK_PACKAGE_INDEX)) {
      throw new Error('standalone package must contain static-fallback index.html');
    }
    if (
      dependencyPaths.length === 0
      || dependencyPaths.length > MAX_DEPENDENCY_RUNTIME_FILES
    ) {
      throw new Error(
        `standalone package must contain 1..=${MAX_DEPENDENCY_RUNTIME_FILES} dependency runtime files`,
      );
    }
    for (const dependency of DEPENDENCY_RUNTIME_ASSETS) {
      const packagePaths = dependencyPaths.filter((item) => (
        item.startsWith(`${dependency.packagePrefix}/`)
      ));
      for (const requiredPath of dependency.requiredPaths) {
        const packagePath = `${dependency.packagePrefix}/${requiredPath}`;
        const record = records.get(`sdkwork-webserver/${packagePath}`);
        if (!packagePaths.includes(packagePath) || !record || record.actualBytes === 0) {
          throw new Error(
            `standalone package ${dependency.id} runtime assets require non-empty ${requiredPath}`,
          );
        }
      }
      if (
        dependency.requiredPrefix
        && !packagePaths.some((item) => (
          item.startsWith(`${dependency.packagePrefix}/${dependency.requiredPrefix}`)
            && item.endsWith(dependency.requiredSuffix)
            && records.get(`sdkwork-webserver/${item}`)?.actualBytes > 0
        ))
      ) {
        throw new Error(
          `standalone package ${dependency.id} runtime assets require module manifests`,
        );
      }
    }
  }
  const expectedContentPaths = [
    ...EXPECTED_FIXED_CONTENT_PATHS,
    ...(expected.deploymentProfile === 'standalone' ? pcPaths : []),
    ...(expected.deploymentProfile === 'standalone' ? h5Paths : []),
    ...(expected.deploymentProfile === 'standalone' ? staticFallbackPaths : []),
    ...(expected.deploymentProfile === 'standalone' ? dependencyPaths : []),
  ].sort();
  if (JSON.stringify(manifestPaths) !== JSON.stringify(expectedContentPaths)) {
    throw new Error('package manifest content paths are missing, unexpected, duplicated, or unsorted');
  }

  const expectedFiles = [
    'sdkwork-webserver/package.manifest.json',
    ...expectedContentPaths.map((item) => `sdkwork-webserver/${item}`),
  ].sort();
  const expectedDirectories = archiveDirectoriesFor(expectedContentPaths);
  const actualFiles = [...records.values()]
    .filter((record) => record.type === 'File')
    .map((record) => record.path)
    .sort();
  const actualDirectories = [...records.values()]
    .filter((record) => record.type === 'Directory')
    .map((record) => record.path)
    .sort();
  if (JSON.stringify(actualFiles) !== JSON.stringify(expectedFiles)) {
    throw new Error('archive file inventory does not match the package contract');
  }
  if (JSON.stringify(actualDirectories) !== JSON.stringify(expectedDirectories)) {
    throw new Error('archive directory inventory does not match the package contract');
  }
  const expectedOrder = [...expectedFiles, ...expectedDirectories].sort();
  if (JSON.stringify(order) !== JSON.stringify(expectedOrder)) {
    throw new Error('archive entries are not in deterministic path order');
  }

  for (const record of records.values()) {
    if (record.mtimeSeconds !== manifest.sourceDateEpoch) {
      throw new Error(`archive entry ${record.path} has a non-deterministic mtime`);
    }
    if (record.type === 'Directory') {
      if ((record.mode & 0o700) !== 0o700) {
        throw new Error(`archive directory ${record.path} must be owner accessible`);
      }
      continue;
    }
    if ((record.mode & 0o400) === 0) {
      throw new Error(`archive file ${record.path} must be owner readable`);
    }
    const isBinary = record.path.startsWith('sdkwork-webserver/bin/');
    if (isBinary && (record.mode & 0o111) === 0) {
      throw new Error(`archive binary ${record.path} must be executable`);
    }
    if (!isBinary && (record.mode & 0o111) !== 0) {
      throw new Error(`archive data file ${record.path} must not be executable`);
    }
  }
}

async function validateReleaseArchive(settings, resolved = resolveArtifact(settings)) {
  const { archive, artifactBase, version } = resolved;
  const archiveStat = inspectRegularFile(archive, 'release archive', MAX_ARCHIVE_BYTES);
  if (archiveStat.size === 0) {
    throw new Error('release archive must not be empty');
  }
  const checksumPath = `${archive}.sha256`;
  const checksumText = readSmallText(checksumPath, MAX_CHECKSUM_BYTES, 'release checksum');
  const checksumMatch = checksumText.match(/^([a-f0-9]{64})  ([^\r\n]+)\r?\n$/u);
  if (!checksumMatch || checksumMatch[2] !== path.basename(archive)) {
    throw new Error('release checksum must contain one canonical SHA-256 record');
  }
  if (sha256File(archive) !== checksumMatch[1]) {
    throw new Error('release archive SHA-256 does not match its sidecar');
  }
  const inspected = await inspectArchiveEntries(archive);
  validatePackageManifest(
    inspected.manifestBuffer,
    inspected.records,
    inspected.order,
    inspected.capturedBuffers,
    {
      deploymentProfile: settings.deploymentProfile,
      architecture: resolved.architecture,
      version,
    },
  );
  console.log(
    `[sdkwork-webserver-release] validated artifact=${artifactBase}.tar.gz bytes=${archiveStat.size} entries=${inspected.records.size}`,
  );
}

async function packageArchive(settings) {
  const resolved = resolveArtifact(settings);
  const { version, architecture, artifactBase, archive } = resolved;
  console.log(
    `[sdkwork-webserver-release] operation=package deploymentProfile=${settings.deploymentProfile} runtimeTarget=server architecture=${architecture} version=${version}`,
  );
  console.log(`[sdkwork-webserver-release] artifact=${artifactBase}.tar.gz`);
  if (settings.dryRun) {
    return;
  }
  if (process.platform !== 'linux' || process.arch !== architecture) {
    throw new Error(
      `linux-${architecture} server archives must be packaged on a linux-${architecture} runner`,
    );
  }

  ensureCriticalSources();
  run('cargo', ['build', '--workspace', '--release'], { timeoutMs: CARGO_BUILD_TIMEOUT_MS });
  let pcStaticFiles = [];
  let h5StaticFiles = [];
  let staticFallbackFiles = [];
  let dependencyRuntimeFiles = [];

  // Canonical Adaptive Web runner (PNPM_SCRIPT_SPEC.md §4.2): builds the
  // selected deploymentProfile × environment bundle to
  // apps/*-{pc,h5}/dist/<profile>/<envAlias>/.
  const runBrowserBuild = (architecture) => {
    const runner = path.join(REPO_ROOT, '..', 'sdkwork-specs', 'tools', 'build-browser-client.mjs');
    run(process.execPath, [
      runner,
      '--root',
      REPO_ROOT,
      '--architecture',
      architecture,
      '--environment',
      settings.environment,
      '--deployment-profile',
      settings.deploymentProfile,
    ], { timeoutMs: PC_BUILD_TIMEOUT_MS });
  };

  // sdkwork-webserver is standalone-only (SDKWORK_WEBSERVER_SPEC.md §17.4):
  // the same-origin PC/H5 bundles are always packaged into the server tar.
  if (!settings.skipPcBuild) {
    runBrowserBuild('pc');
  }
  if (!settings.skipH5Build) {
    runBrowserBuild('h5');
  }
  pcStaticFiles = inspectPcBuildOutput(settings);
  h5StaticFiles = inspectH5BuildOutput(settings);
  staticFallbackFiles = inspectStaticFallbackAssets();
  dependencyRuntimeFiles = inspectDependencyRuntimeAssets();
  const cargoTargetRoot = resolveCargoTargetRoot();
  const stageContainer = path.join(STAGE_PARENT, `${artifactBase}-${process.pid}`);
  const stageRoot = path.join(stageContainer, 'sdkwork-webserver');
  assertSafeOwnedPath(stageContainer, STAGE_PARENT, 'release stage');
  rmSync(stageContainer, { recursive: true, force: true });
  mkdirSync(path.join(stageRoot, 'bin'), { recursive: true, mode: 0o755 });
  mkdirSync(OUTPUT_ROOT, { recursive: true, mode: 0o755 });

  try {
    const packagedFiles = [];
    let packageContentBytes = 0;
    for (const binary of BINARIES) {
      const source = path.join(cargoTargetRoot, 'release', binary);
      const stat = inspectRegularFile(source, `release binary ${binary}`);
      packageContentBytes += stat.size;
      const target = path.join(stageRoot, 'bin', binary);
      copyFileSync(source, target);
      chmodSync(target, 0o755);
      packagedFiles.push(target);
    }
    for (const asset of PACKAGE_ASSETS) {
      const target = copyPackageAsset(asset, stageRoot);
      packageContentBytes += statSync(target).size;
      packagedFiles.push(target);
    }
    for (const file of pcStaticFiles) {
      const target = copyPcStaticFile(file, stageRoot);
      packageContentBytes += statSync(target).size;
      packagedFiles.push(target);
    }
    for (const file of h5StaticFiles) {
      const target = copyPackagedStaticFile(file, stageRoot, 'H5');
      packageContentBytes += statSync(target).size;
      packagedFiles.push(target);
    }
    for (const file of staticFallbackFiles) {
      const target = copyPackagedStaticFile(file, stageRoot, 'static-fallback');
      packageContentBytes += statSync(target).size;
      packagedFiles.push(target);
    }
    for (const file of dependencyRuntimeFiles) {
      const target = copyDependencyRuntimeFile(file, stageRoot);
      packageContentBytes += statSync(target).size;
      packagedFiles.push(target);
    }
    if (packageContentBytes > MAX_PACKAGE_CONTENT_BYTES) {
      throw new Error(`package content exceeds ${MAX_PACKAGE_CONTENT_BYTES} bytes`);
    }

    const sourceDateEpoch = Number.parseInt(process.env.SOURCE_DATE_EPOCH ?? '0', 10);
    if (!Number.isSafeInteger(sourceDateEpoch) || sourceDateEpoch < 0) {
      throw new Error('SOURCE_DATE_EPOCH must be a non-negative safe integer');
    }
    const content = packagedFiles
      .map((filePath) => ({
        path: path.relative(stageRoot, filePath).split(path.sep).join('/'),
        bytes: statSync(filePath).size,
        sha256: sha256File(filePath),
      }))
      .sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));
    const packageManifest = {
      schemaVersion: 1,
      kind: 'sdkwork.server-package',
      application: 'sdkwork-web',
      version,
      deploymentProfile: settings.deploymentProfile,
      runtimeTarget: 'server',
      platform: 'linux',
      architecture,
      sourceDateEpoch,
      content,
    };
    const packageManifestPath = path.join(stageRoot, 'package.manifest.json');
    writeFileSync(packageManifestPath, `${JSON.stringify(packageManifest, null, 2)}\n`, {
      encoding: 'utf8',
      flag: 'wx',
      mode: 0o644,
    });
    chmodSync(packageManifestPath, 0o644);

    const temporaryArchive = `${archive}.tmp-${process.pid}`;
    rmSync(temporaryArchive, { force: true });
    try {
      run(
        'tar',
        [
          '--sort=name',
          `--mtime=@${sourceDateEpoch}`,
          '--owner=0',
          '--group=0',
          '--numeric-owner',
          '-czf',
          temporaryArchive,
          'sdkwork-webserver',
        ],
        {
          cwd: stageContainer,
          env: { ...process.env, LC_ALL: 'C' },
          timeoutMs: TAR_TIMEOUT_MS,
        },
      );
      const archiveBytes = inspectRegularFile(
        temporaryArchive,
        'temporary release archive',
        MAX_ARCHIVE_BYTES,
      ).size;
      if (archiveBytes === 0) {
        throw new Error('release archive must not be empty');
      }
      syncFile(temporaryArchive);
      assertRegularOrMissing(archive, 'release archive');
      renameSync(temporaryArchive, archive);
      syncDirectory(OUTPUT_ROOT);
    } finally {
      rmSync(temporaryArchive, { force: true });
    }
    writeAtomicText(
      `${archive}.sha256`,
      `${sha256File(archive)}  ${path.basename(archive)}\n`,
    );
    await validateReleaseArchive(settings, resolved);
    run(
      process.execPath,
      [
        'scripts/webserver-sbom.mjs',
        'generate',
        '--deployment-profile',
        settings.deploymentProfile,
        '--architecture',
        architecture,
        '--version',
        version,
      ],
      { timeoutMs: SBOM_TIMEOUT_MS },
    );
    console.log(`[sdkwork-webserver-release] wrote ${path.relative(REPO_ROOT, archive)}`);
  } finally {
    rmSync(stageContainer, { recursive: true, force: true });
  }
}

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    console.log(
      'Usage: node scripts/webserver-release.mjs <package|validate> --deployment-profile <standalone> [--architecture <x64|arm64>] [--version <semver>] [--skip-pc-build] [--skip-h5-build] [--dry-run]',
    );
    return;
  }
  if (!['package', 'validate'].includes(settings.operation)) {
    throw new Error('operation must be package or validate');
  }
  if (settings.operation === 'package') {
    await packageArchive(settings);
    return;
  }
  const resolved = resolveArtifact(settings);
  console.log(
    `[sdkwork-webserver-release] operation=validate deploymentProfile=${settings.deploymentProfile} runtimeTarget=server architecture=${resolved.architecture} version=${resolved.version}`,
  );
  console.log(`[sdkwork-webserver-release] artifact=${resolved.artifactBase}.tar.gz`);
  if (!settings.dryRun) {
    await validateReleaseArchive(settings, resolved);
  }
}

main().catch((error) => {
  process.stderr.write(`[sdkwork-webserver-release] ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
