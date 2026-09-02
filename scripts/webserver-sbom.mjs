#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  closeSync,
  existsSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  readSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const ROOT_PACKAGES = [
  'sdkwork-webserver-agent',
  'sdkwork-api-webserver-standalone-gateway',
  'sdkwork-webserver-website-delivery-edge-runtime',
  'sdkwork-webserver-certificate-worker',
];
const SUPPORTED_ARCHITECTURES = new Set(['x64', 'arm64']);
const MAX_ARCHIVE_BYTES = 512 * 1024 * 1024;
const MAX_SBOM_BYTES = 16 * 1024 * 1024;
const MAX_CHECKSUM_BYTES = 256;
const MAX_METADATA_BYTES = 64 * 1024 * 1024;
const MAX_COMPONENTS = 20_000;
const MAX_DEPENDENCY_EDGES = 100_000;
const HASH_BUFFER_BYTES = 64 * 1024;
const COMMAND_TIMEOUT_MS = 2 * 60 * 1000;

function parseArgs(argv) {
  const settings = {
    operation: argv[0],
    deploymentProfile: process.env.SDKWORK_DEPLOYMENT_PROFILE,
    architecture: process.env.SDKWORK_PACKAGE_ARCHITECTURE,
    version: undefined,
  };
  for (let index = 1; index < argv.length; index += 1) {
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

function inspectRegularFile(filePath, label, maxBytes) {
  if (!existsSync(filePath)) {
    throw new Error(`${label} does not exist: ${filePath}`);
  }
  const linkStat = lstatSync(filePath);
  const stat = statSync(filePath);
  if (linkStat.isSymbolicLink() || !linkStat.isFile() || !stat.isFile()) {
    throw new Error(`${label} must be a regular non-symlink file`);
  }
  if (!Number.isSafeInteger(stat.size) || stat.size <= 0 || stat.size > maxBytes) {
    throw new Error(`${label} must contain 1..=${maxBytes} bytes`);
  }
  return stat;
}

function assertSafeOwnedPath(candidate, owner, label) {
  const relative = path.relative(owner, candidate);
  if (relative === '' || relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new Error(`${label} is outside its owned directory`);
  }
}

function readSmallText(filePath, maxBytes, label) {
  inspectRegularFile(filePath, label, maxBytes);
  return readFileSync(filePath, 'utf8');
}

function sha256File(filePath) {
  const stat = inspectRegularFile(filePath, 'hash input', MAX_ARCHIVE_BYTES);
  const descriptor = openSync(filePath, 'r');
  const buffer = Buffer.allocUnsafe(HASH_BUFFER_BYTES);
  const hash = createHash('sha256');
  let total = 0;
  try {
    while (total < stat.size) {
      const bytesRead = readSync(descriptor, buffer, 0, buffer.length, null);
      if (bytesRead <= 0) {
        throw new Error(`unexpected EOF while hashing ${filePath}`);
      }
      total += bytesRead;
      if (total > stat.size) {
        throw new Error(`hash input grew while reading ${filePath}`);
      }
      hash.update(buffer.subarray(0, bytesRead));
    }
  } finally {
    closeSync(descriptor);
  }
  if (statSync(filePath).size !== stat.size) {
    throw new Error(`hash input changed size while reading ${filePath}`);
  }
  return hash.digest('hex');
}

function resolveVersion(settings) {
  const manifest = JSON.parse(
    readSmallText(
      path.join(REPO_ROOT, 'sdkwork.app.config.json'),
      MAX_CHECKSUM_BYTES * 1024,
      'application manifest',
    ),
  );
  const packageVersion = process.env.SDKWORK_PACKAGE_VERSION?.trim();
  const compatibilityVersion = process.env.SDKWORK_RELEASE_VERSION?.trim();
  if (packageVersion && compatibilityVersion && packageVersion !== compatibilityVersion) {
    throw new Error('SDKWORK_PACKAGE_VERSION conflicts with SDKWORK_RELEASE_VERSION');
  }
  const version = settings.version || packageVersion || compatibilityVersion || manifest.release?.defaultVersion;
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/u.test(version ?? '')) {
    throw new Error('release version must be an explicit semantic version');
  }
  return version;
}

function resolveArtifact(settings) {
  if (settings.deploymentProfile !== 'standalone') {
    // sdkwork-webserver is standalone-only (SDKWORK_WEBSERVER_SPEC.md §17.4).
    throw new Error('--deployment-profile must be standalone');
  }
  const architecture = settings.architecture?.trim() || process.arch;
  if (!SUPPORTED_ARCHITECTURES.has(architecture)) {
    throw new Error('release architecture must be x64 or arm64');
  }
  const version = resolveVersion(settings);
  const artifactBase = `sdkwork-webserver-linux-${architecture}-${settings.deploymentProfile}-server-${version}`;
  const archive = path.join(OUTPUT_ROOT, `${artifactBase}.tar.gz`);
  const sbom = `${archive}.cdx.json`;
  assertSafeOwnedPath(archive, OUTPUT_ROOT, 'release archive');
  assertSafeOwnedPath(sbom, OUTPUT_ROOT, 'release SBOM');
  return {
    version,
    architecture,
    artifactBase,
    archive,
    sbom,
    sbomChecksum: `${sbom}.sha256`,
  };
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    env: options.env ?? process.env,
    stdio: 'pipe',
    timeout: options.timeoutMs ?? COMMAND_TIMEOUT_MS,
    maxBuffer: options.maxBuffer ?? MAX_METADATA_BYTES,
    killSignal: 'SIGKILL',
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message || result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result.stdout;
}

function validateArchive(resolved) {
  run(process.execPath, [
    'scripts/webserver-release.mjs',
    'validate',
    '--deployment-profile',
    resolved.deploymentProfile,
    '--architecture',
    resolved.architecture,
    '--version',
    resolved.version,
  ]);
}

function sourceKind(source) {
  if (!source) {
    return 'workspace';
  }
  if (source.startsWith('registry+')) {
    return 'registry';
  }
  if (source.startsWith('git+')) {
    return 'git';
  }
  return 'other';
}

function packageRef(pkg) {
  const name = encodeURIComponent(pkg.name);
  const version = encodeURIComponent(pkg.version);
  return `pkg:cargo/${name}@${version}?source=${sourceKind(pkg.source)}`;
}

function deterministicUuid(hexDigest) {
  const bytes = Buffer.from(hexDigest.slice(0, 32), 'hex');
  bytes[6] = (bytes[6] & 0x0f) | 0x50;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytes.toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function rustTargetForArchitecture(architecture) {
  return architecture === 'arm64'
    ? 'aarch64-unknown-linux-gnu'
    : 'x86_64-unknown-linux-gnu';
}

function loadCargoMetadata(architecture) {
  const rustTarget = rustTargetForArchitecture(architecture);
  const stdout = run('cargo', [
    'metadata',
    '--locked',
    '--format-version',
    '1',
    '--filter-platform',
    rustTarget,
  ]);
  let metadata;
  try {
    metadata = JSON.parse(stdout);
  } catch {
    throw new Error('cargo metadata output must be valid JSON');
  }
  if (
    !Array.isArray(metadata.packages) ||
    metadata.packages.length === 0 ||
    metadata.packages.length > MAX_COMPONENTS ||
    !metadata.resolve ||
    !Array.isArray(metadata.resolve.nodes)
  ) {
    throw new Error('cargo metadata package/resolve graph is missing or exceeds its bound');
  }
  return metadata;
}

function isReleaseDependency(dependency) {
  return !Array.isArray(dependency.dep_kinds) ||
    dependency.dep_kinds.some((kind) => kind.kind !== 'dev');
}

function selectReleaseClosure(metadata) {
  const packageById = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const nodeById = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const rootIds = ROOT_PACKAGES.map((name) => {
    const candidates = metadata.packages.filter((pkg) => pkg.name === name);
    if (candidates.length !== 1) {
      throw new Error(`cargo metadata must contain exactly one release root package ${name}`);
    }
    return candidates[0].id;
  });
  const selected = new Set();
  const pending = [...rootIds];
  let edgeCount = 0;
  while (pending.length > 0) {
    const packageId = pending.pop();
    if (selected.has(packageId)) {
      continue;
    }
    if (!packageById.has(packageId)) {
      throw new Error(`cargo dependency package is missing: ${packageId}`);
    }
    selected.add(packageId);
    if (selected.size > MAX_COMPONENTS) {
      throw new Error(`release dependency closure exceeds ${MAX_COMPONENTS} components`);
    }
    const node = nodeById.get(packageId);
    if (!node || !Array.isArray(node.deps)) {
      throw new Error(`cargo resolve node is missing: ${packageId}`);
    }
    for (const dependency of node.deps.filter(isReleaseDependency)) {
      edgeCount += 1;
      if (edgeCount > MAX_DEPENDENCY_EDGES) {
        throw new Error(`release dependency closure exceeds ${MAX_DEPENDENCY_EDGES} edges`);
      }
      pending.push(dependency.pkg);
    }
  }
  return { packageById, nodeById, rootIds, selected };
}

function componentForPackage(pkg) {
  const component = {
    type: 'library',
    'bom-ref': packageRef(pkg),
    name: pkg.name,
    version: pkg.version,
    purl: packageRef(pkg),
    properties: [
      { name: 'sdkwork:cargo:source-kind', value: sourceKind(pkg.source) },
    ],
  };
  if (typeof pkg.license === 'string' && pkg.license.trim()) {
    component.licenses = [{ license: { name: pkg.license.trim() } }];
  }
  return component;
}

function buildSbom(resolved) {
  const archiveDigest = sha256File(resolved.archive);
  const metadata = loadCargoMetadata(resolved.architecture);
  const closure = selectReleaseClosure(metadata);
  const refById = new Map();
  const usedRefs = new Set();
  for (const packageId of closure.selected) {
    const ref = packageRef(closure.packageById.get(packageId));
    if (usedRefs.has(ref)) {
      throw new Error(`cargo dependency identity is ambiguous: ${ref}`);
    }
    usedRefs.add(ref);
    refById.set(packageId, ref);
  }
  const components = [...closure.selected]
    .map((packageId) => componentForPackage(closure.packageById.get(packageId)))
    .sort((left, right) => left['bom-ref'].localeCompare(right['bom-ref'], 'en'));
  const artifactRef = `pkg:generic/sdkwork-webserver@${encodeURIComponent(resolved.version)}?arch=${resolved.architecture}&profile=${resolved.deploymentProfile}`;
  const dependencies = [...closure.selected]
    .map((packageId) => {
      const node = closure.nodeById.get(packageId);
      const dependsOn = [...new Set(node.deps
        .filter(isReleaseDependency)
        .map((dependency) => dependency.pkg)
        .filter((dependencyId) => closure.selected.has(dependencyId))
        .map((dependencyId) => refById.get(dependencyId)))]
        .sort((left, right) => left.localeCompare(right, 'en'));
      return { ref: refById.get(packageId), dependsOn };
    });
  dependencies.push({
    ref: artifactRef,
    dependsOn: closure.rootIds.map((packageId) => refById.get(packageId)).sort((left, right) => left.localeCompare(right, 'en')),
  });
  dependencies.sort((left, right) => left.ref.localeCompare(right.ref, 'en'));
  const sbom = {
    bomFormat: 'CycloneDX',
    specVersion: '1.6',
    serialNumber: `urn:uuid:${deterministicUuid(archiveDigest)}`,
    version: 1,
    metadata: {
      component: {
        type: 'application',
        'bom-ref': artifactRef,
        group: 'sdkwork',
        name: 'sdkwork-webserver',
        version: resolved.version,
        hashes: [{ alg: 'SHA-256', content: archiveDigest }],
        properties: [
          { name: 'sdkwork:artifact:name', value: path.basename(resolved.archive) },
          { name: 'sdkwork:deployment-profile', value: resolved.deploymentProfile },
          { name: 'sdkwork:runtime-target', value: 'server' },
          { name: 'sdkwork:platform', value: 'linux' },
          { name: 'sdkwork:architecture', value: resolved.architecture },
          { name: 'sdkwork:rust-target', value: rustTargetForArchitecture(resolved.architecture) },
        ],
      },
    },
    components,
    dependencies,
  };
  return `${JSON.stringify(sbom, null, 2)}\n`;
}

function writeAtomic(filePath, text) {
  mkdirSync(path.dirname(filePath), { recursive: true, mode: 0o755 });
  const temporaryPath = `${filePath}.tmp-${process.pid}`;
  rmSync(temporaryPath, { force: true });
  try {
    writeFileSync(temporaryPath, text, { encoding: 'utf8', flag: 'wx', mode: 0o644 });
    const descriptor = openSync(temporaryPath, process.platform === 'win32' ? 'r+' : 'r');
    try {
      fsyncSync(descriptor);
    } finally {
      closeSync(descriptor);
    }
    renameSync(temporaryPath, filePath);
  } finally {
    rmSync(temporaryPath, { force: true });
  }
}

function generate(resolved) {
  validateArchive(resolved);
  const sbomText = buildSbom(resolved);
  if (Buffer.byteLength(sbomText) > MAX_SBOM_BYTES) {
    throw new Error(`release SBOM exceeds ${MAX_SBOM_BYTES} bytes`);
  }
  writeAtomic(resolved.sbom, sbomText);
  const checksum = sha256File(resolved.sbom);
  writeAtomic(resolved.sbomChecksum, `${checksum}  ${path.basename(resolved.sbom)}\n`);
  console.log(
    `[sdkwork-webserver-sbom] wrote artifact=${resolved.artifactBase}.tar.gz components=${JSON.parse(sbomText).components.length}`,
  );
}

function validate(resolved) {
  validateArchive(resolved);
  const sbomText = readSmallText(resolved.sbom, MAX_SBOM_BYTES, 'release SBOM');
  if (!sbomText.endsWith('\n')) {
    throw new Error('release SBOM must end with a newline');
  }
  const checksumText = readSmallText(resolved.sbomChecksum, MAX_CHECKSUM_BYTES, 'release SBOM checksum');
  const checksumMatch = checksumText.match(/^([a-f0-9]{64})  ([^\r\n]+)\r?\n$/u);
  if (!checksumMatch || checksumMatch[2] !== path.basename(resolved.sbom)) {
    throw new Error('release SBOM checksum must contain one canonical SHA-256 record');
  }
  if (sha256File(resolved.sbom) !== checksumMatch[1]) {
    throw new Error('release SBOM SHA-256 does not match its sidecar');
  }
  let parsed;
  try {
    parsed = JSON.parse(sbomText);
  } catch {
    throw new Error('release SBOM must be valid JSON');
  }
  if (
    parsed.bomFormat !== 'CycloneDX' ||
    parsed.specVersion !== '1.6' ||
    parsed.version !== 1 ||
    !Array.isArray(parsed.components) ||
    parsed.components.length === 0 ||
    parsed.components.length > MAX_COMPONENTS ||
    !Array.isArray(parsed.dependencies)
  ) {
    throw new Error('release SBOM does not satisfy the bounded CycloneDX 1.6 contract');
  }
  const expected = buildSbom(resolved);
  if (sbomText !== expected) {
    throw new Error('release SBOM does not match the artifact and locked Cargo dependency closure');
  }
  console.log(
    `[sdkwork-webserver-sbom] validated artifact=${resolved.artifactBase}.tar.gz components=${parsed.components.length}`,
  );
}

function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    console.log(
      'Usage: node scripts/webserver-sbom.mjs <generate|validate> --deployment-profile <standalone> [--architecture <x64|arm64>] [--version <semver>]',
    );
    return;
  }
  if (!['generate', 'validate'].includes(settings.operation)) {
    throw new Error('operation must be generate or validate');
  }
  const resolved = {
    ...resolveArtifact(settings),
    deploymentProfile: settings.deploymentProfile,
  };
  if (settings.operation === 'generate') {
    generate(resolved);
  } else {
    validate(resolved);
  }
}

try {
  main();
} catch (error) {
  process.stderr.write(`[sdkwork-webserver-sbom] ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}
