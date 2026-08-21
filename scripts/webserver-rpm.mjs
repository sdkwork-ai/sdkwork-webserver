#!/usr/bin/env node

// SDKWork Web Server RHEL-family (.rpm) installer builder.
//
// Symmetric to scripts/webserver-deb.mjs: packages the standalone release
// archive into an RPM following RUNTIME_DIRECTORY_SPEC.md section 4.1 and
// PACKAGING_SPEC.md section 5.5 (the .deb/.rpm installer layout matrix).
// The SPEC %post/%preun/%postun scripts mirror the deb maintainer scripts;
// RPM declares runtime dependencies through Requires instead of installing
// them from %post.
//
// Usage:
//   node scripts/webserver-rpm.mjs package --environment test|production
//       [--architecture x64|arm64] [--version <semver>] [--dry-run]
//   node scripts/webserver-rpm.mjs validate --environment test|production
//       [--architecture x64|arm64] [--version <semver>]
//
// rpmbuild runs natively on Linux; on Windows it is invoked through WSL.

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const RELEASE_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const INSTALLER_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'installers');
// The staging directory must live on a filesystem with real permission bits
// (Linux ext4); override with SDKWORK_RPM_STAGE_PARENT (WSL /mnt does not
// preserve mode bits).
const STAGE_PARENT = process.env.SDKWORK_RPM_STAGE_PARENT
  ? path.resolve(process.env.SDKWORK_RPM_STAGE_PARENT)
  : path.join(REPO_ROOT, '.sdkwork', 'runtime', 'rpm-stage');const RPM_TEMPLATE_ROOT = path.join(REPO_ROOT, 'scripts', 'rpm');
const DEB_TEMPLATE_ROOT = path.join(REPO_ROOT, 'scripts', 'deb');
const PC_RUNTIME_ENV_SOURCES = Object.freeze({
  test: path.join(
    REPO_ROOT,
    'apps',
    'sdkwork-webserver-pc',
    'etc',
    'browser',
    'runtime-env.test.json',
  ),
});
const MAX_RPM_BYTES = 768 * 1024 * 1024;

const ENVIRONMENTS = Object.freeze({
  test: {
    packageName: 'sdkwork-webserver-test',
    serviceName: 'sdkwork-webserver-test',
    domain: 'server-test.sdkwork.com',
    ingressPort: 8888,
    publicUrl: 'http://server-test.sdkwork.com:8888',
    databaseName: 'sdkwork_ai_test',
    databaseUser: 'sdkwork_ai_test',
    acmeProfile: 'staging',
    acmeDirectoryUrl: 'https://acme-staging-v02.api.letsencrypt.org/directory',
    certificateWorker: false,
    nginxRequires: '',
    description: 'SDKWork Web Server standalone gateway installer (test environment)',
  },
  production: {
    packageName: 'sdkwork-webserver',
    serviceName: 'sdkwork-webserver',
    domain: 'server.sdkwork.com',
    ingressPort: 8080,
    publicUrl: 'https://server.sdkwork.com',
    databaseName: 'sdkwork_ai_prod',
    databaseUser: 'sdkwork_ai_prod',
    acmeProfile: 'production',
    acmeDirectoryUrl: 'https://acme-v02.api.letsencrypt.org/directory',
    certificateWorker: true,
    nginxRequires: 'Requires:       nginx\n',
    description: 'SDKWork Web Server standalone gateway installer (production environment)',
  },
});

const RPM_ARCHITECTURES = Object.freeze({ x64: 'x86_64', arm64: 'aarch64' });
const RPM_PACKAGE_IDS = Object.freeze({
  test: 'linux-rhel-x64-standalone-test-server-rpm',
  production: 'linux-rhel-x64-standalone-server-rpm',
});

function appVersion() {
  const manifest = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'sdkwork.app.config.json'), 'utf8'),
  );
  const version = manifest?.release?.currentVersion;
  if (typeof version !== 'string' || version.length === 0) {
    throw new Error('sdkwork.app.config.json release.currentVersion is missing');
  }
  return version;
}

function parseArgs(argv) {
  const settings = {
    operation: argv[0],
    environment: undefined,
    architecture: 'x64',
    version: appVersion(),
    dryRun: false,
    help: false,
  };
  for (let index = 1; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--environment') {
      settings.environment = argv[++index];
    } else if (argument === '--architecture') {
      settings.architecture = argv[++index];
    } else if (argument === '--version') {
      settings.version = argv[++index];
    } else if (argument === '--dry-run') {
      settings.dryRun = true;
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  if (settings.help) {
    return settings;
  }
  if (!['package', 'validate'].includes(settings.operation)) {
    throw new Error(`operation must be package or validate, got ${settings.operation}`);
  }
  if (!ENVIRONMENTS[settings.environment]) {
    throw new Error(`--environment must be test or production, got ${settings.environment}`);
  }
  if (!RPM_ARCHITECTURES[settings.architecture]) {
    throw new Error(`--architecture must be x64 or arm64, got ${settings.architecture}`);
  }
  return settings;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    env: options.env ?? process.env,
    stdio: options.capture ? 'pipe' : 'inherit',
    timeout: options.timeoutMs ?? 10 * 60 * 1000,
    maxBuffer: 16 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message ?? result.stderr?.trim() ?? `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result;
}

function archiveBaseName(settings) {
  return (
    `sdkwork-webserver-linux-${settings.architecture}-standalone-server-${settings.version}`
  );
}

function archivePath(settings) {
  return path.join(RELEASE_OUTPUT_ROOT, `${archiveBaseName(settings)}.tar.gz`);
}

function wslPath(windowsPath) {
  const match = windowsPath.match(/^([A-Za-z]):\\(.*)$/);
  if (!match) {
    throw new Error(`cannot convert Windows path to WSL: ${windowsPath}`);
  }
  return `/mnt/${match[1].toLowerCase()}/${match[2].replace(/\\/g, '/')}`;
}

function wslOrNative(filePath) {
  return process.platform === 'win32' ? wslPath(filePath) : filePath;
}

function runRpmBuild(args, env = {}) {
  if (process.platform === 'win32') {
    const shellArgs = args.map((arg) => (arg.includes(' ') ? `'${arg}'` : arg)).join(' ');
    return run('wsl.exe', ['-d', 'Ubuntu-22.04', '-e', 'bash', '-lc', `rpmbuild ${shellArgs}`], {
      capture: true,
      env: { ...process.env, ...env },
    });
  }
  return run('rpmbuild', args, { capture: true, env: { ...process.env, ...env } });
}

function runRpm(args) {
  if (process.platform === 'win32') {
    const shellArgs = args.map((arg) => (arg.includes(' ') ? `'${arg}'` : arg)).join(' ');
    return run('wsl.exe', ['-d', 'Ubuntu-22.04', '-e', 'bash', '-lc', `rpm ${shellArgs}`], {
      capture: true,
    });
  }
  return run('rpm', args, { capture: true });
}

function sha256File(filePath) {
  const hash = createHash('sha256');
  hash.update(readFileSync(filePath));
  return hash.digest('hex');
}

function renderTemplate(templatePath, values) {
  let text = readFileSync(templatePath, 'utf8');
  for (const [key, value] of Object.entries(values)) {
    text = text.split(`__${key}__`).join(value);
  }
  return text;
}

async function ensureReleaseArchive(settings) {
  const archive = archivePath(settings);
  if (existsSync(archive)) {
    return archive;
  }
  if (process.platform !== 'linux') {
    throw new Error(
      `release archive is missing: ${archive}\n`
        + 'Build it on a Linux runner first: '
        + 'node scripts/webserver-release.mjs package --deployment-profile standalone '
        + `--architecture ${settings.architecture} --version ${settings.version}`,
    );
  }
  const args = [
    'package',
    '--deployment-profile',
    'standalone',
    '--architecture',
    settings.architecture,
    '--version',
    settings.version,
  ];
  if (settings.dryRun) {
    args.push('--dry-run');
  }
  run(process.execPath, ['scripts/webserver-release.mjs', ...args]);
  if (!existsSync(archive)) {
    throw new Error(`release archive was not produced: ${archive}`);
  }
  return archive;
}

async function assembleRpmStage(settings) {
  const environment = ENVIRONMENTS[settings.environment];
  const rpmArchitecture = RPM_ARCHITECTURES[settings.architecture];
  const stageContainer = path.join(STAGE_PARENT, `${environment.packageName}-${process.pid}`);
  rmSync(stageContainer, { recursive: true, force: true });
  mkdirSync(stageContainer, { recursive: true, mode: 0o700 });
  for (const sub of ['SPECS', 'SOURCES', 'BUILD', 'BUILDROOT', 'RPMS', 'SRPMS']) {
    mkdirSync(path.join(stageContainer, sub), { recursive: true, mode: 0o755 });
  }

  const archive = await ensureReleaseArchive(settings);
  writeFileSync(path.join(stageContainer, 'SOURCES', path.basename(archive)), readFileSync(archive), {
    mode: 0o644,
  });

  // systemd units rendered from the shared templates (TOML-loaded runtime
  // config; no EnvironmentFile).
  const unitValues = { ENVIRONMENT: settings.environment };
  writeFileSync(
    path.join(stageContainer, 'SOURCES', `${environment.serviceName}.service`),
    renderTemplate(path.join(DEB_TEMPLATE_ROOT, 'sdkwork-webserver.service.template'), unitValues),
    { mode: 0o644 },
  );
  if (environment.certificateWorker) {
    writeFileSync(
      path.join(stageContainer, 'SOURCES', 'sdkwork-webserver-certificate-worker.service'),
      renderTemplate(
        path.join(DEB_TEMPLATE_ROOT, 'sdkwork-webserver-certificate-worker.service.template'),
        unitValues,
      ),
      { mode: 0o644 },
    );
  }

  // Test package: PC runtime env override source (the archive is materialized
  // for standalone.production).
  let pcSource1 = '';
  let pcRuntimeEnvOverride = '';
  if (settings.environment === 'test') {
    const source = PC_RUNTIME_ENV_SOURCES.test;
    if (!existsSync(source)) {
      throw new Error(`test PC runtime env source is missing: ${source}`);
    }
    writeFileSync(
      path.join(stageContainer, 'SOURCES', 'runtime-env.test.json'),
      `${JSON.stringify(JSON.parse(readFileSync(source, 'utf8')), null, 2)}\n`,
      { mode: 0o644 },
    );
    pcSource1 = 'Source1:        runtime-env.test.json';
    pcRuntimeEnvOverride = 'install -m 0644 %{_sourcedir}/runtime-env.test.json \\\n'
      + '  %{buildroot}/usr/share/sdkwork/webserver/web/pc/runtime-env.json';
  }

  const conflictingPackage = settings.environment === 'test'
    ? 'sdkwork-webserver'
    : 'sdkwork-webserver-test';
  const certWorkerUnit = environment.certificateWorker
    ? 'install -m 0644 %{_sourcedir}/sdkwork-webserver-certificate-worker.service \\\n'
      + '  %{buildroot}/usr/lib/systemd/system/sdkwork-webserver-certificate-worker.service'
    : '';
  const certWorkerFile = environment.certificateWorker
    ? '/usr/lib/systemd/system/sdkwork-webserver-certificate-worker.service'
    : '';

  const specValues = {
    RPM_NAME: environment.packageName,
    ENVIRONMENT: settings.environment,
    DOMAIN: environment.domain,
    DB_NAME: environment.databaseName,
    DB_USER: environment.databaseUser,
    SERVICE_NAME: environment.serviceName,
    VERSION: settings.version,
    PUBLIC_URL: environment.publicUrl,
    INGRESS_BIND: `0.0.0.0:${environment.ingressPort}`,
    INGRESS_PORT: String(environment.ingressPort),
    INTERNAL_API_URL: `http://127.0.0.1:${environment.ingressPort}`,
    ACME_PROFILE: environment.acmeProfile,
    ACME_DIRECTORY_URL: environment.acmeDirectoryUrl,
    CONFLICTS: conflictingPackage,
    NGINX_REQUIRES: environment.nginxRequires,
    PC_SOURCE1: pcSource1,
    PC_RUNTIME_ENV_OVERRIDE: pcRuntimeEnvOverride,
    CERT_WORKER_UNIT: certWorkerUnit,
    CERT_WORKER_FILE: certWorkerFile,
  };
  const specPath = path.join(stageContainer, 'SPECS', `${environment.packageName}.spec`);
  writeFileSync(
    specPath,
    renderTemplate(path.join(RPM_TEMPLATE_ROOT, 'sdkwork-webserver.spec.template'), specValues),
    { mode: 0o644 },
  );

  return { stageContainer, specPath, rpmArchitecture };
}

function rpmFileName(settings) {
  const environment = ENVIRONMENTS[settings.environment];
  const rpmArchitecture = RPM_ARCHITECTURES[settings.architecture];
  return `${environment.packageName}-${settings.version}-1.${rpmArchitecture}.rpm`;
}

function buildRpm(settings, stageContainer, specPath, rpmArchitecture) {
  const debFileName = rpmFileName(settings);
  const rpmPath = path.join(INSTALLER_OUTPUT_ROOT, debFileName);
  mkdirSync(INSTALLER_OUTPUT_ROOT, { recursive: true, mode: 0o755 });
  const sourceDateEpoch = Number.parseInt(process.env.SOURCE_DATE_EPOCH ?? '0', 10);
  const defines = [
    '--define',
    `_topdir ${wslOrNative(stageContainer)}`,
    '--define',
    `_sourcedir ${wslOrNative(path.join(stageContainer, 'SOURCES'))}`,
    '--define',
    `_specdir ${wslOrNative(path.join(stageContainer, 'SPECS'))}`,
    '--define',
    `_builddir ${wslOrNative(path.join(stageContainer, 'BUILD'))}`,
    '--define',
    `_buildrootdir ${wslOrNative(path.join(stageContainer, 'BUILDROOT'))}`,
    '--define',
    `_rpmdir ${wslOrNative(path.join(stageContainer, 'RPMS'))}`,
    '--define',
    `_srcrpmdir ${wslOrNative(path.join(stageContainer, 'SRPMS'))}`,
  ];
  runRpmBuild(
    ['-bb', wslOrNative(specPath), ...defines],
    { SOURCE_DATE_EPOCH: String(sourceDateEpoch) },
  );
  const built = path.join(
    stageContainer,
    'RPMS',
    rpmArchitecture,
    `${ENVIRONMENTS[settings.environment].packageName}-${settings.version}-1.${rpmArchitecture}.rpm`,
  );
  if (!existsSync(built)) {
    throw new Error(`rpmbuild did not produce ${built}`);
  }
  const stat = statSync(built);
  if (stat.size === 0 || stat.size > MAX_RPM_BYTES) {
    throw new Error(`rpm is outside the size bounds: ${stat.size}`);
  }
  rmSync(rpmPath, { force: true });
  // rpmbuild output lives on the stage filesystem (possibly a different
  // device than dist/); copy instead of rename.
  writeFileSync(rpmPath, readFileSync(built), { mode: 0o644 });
  rmSync(built, { force: true });
  writeFileSync(`${rpmPath}.sha256`, `${sha256File(rpmPath)}  ${path.basename(rpmPath)}\n`, {
    mode: 0o644,
  });
  return rpmPath;
}

function validateRpm(settings) {
  const environment = ENVIRONMENTS[settings.environment];
  const rpmPath = path.join(INSTALLER_OUTPUT_ROOT, rpmFileName(settings));
  if (!existsSync(rpmPath)) {
    throw new Error(`installer is missing: ${rpmPath}`);
  }
  const checksumText = readFileSync(`${rpmPath}.sha256`, 'utf8').trim();
  const checksumMatch = checksumText.match(/^([a-f0-9]{64})  (.+)$/);
  if (!checksumMatch || checksumMatch[2] !== path.basename(rpmPath)) {
    throw new Error('installer checksum sidecar is invalid');
  }
  if (sha256File(rpmPath) !== checksumMatch[1]) {
    throw new Error('installer SHA-256 does not match its sidecar');
  }
  const info = runRpm(['-qip', wslOrNative(rpmPath)]).stdout;
  if (!info.includes(`Name        : ${environment.packageName}`)
    || !info.includes(`Version     : ${settings.version}`)) {
    throw new Error('installer RPM metadata does not match the selected environment');
  }
  const contents = runRpm(['-qpl', wslOrNative(rpmPath)]).stdout;
  const requiredPaths = [
    '/usr/lib/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway',
    `/usr/lib/systemd/system/${environment.serviceName}.service`,
    '/usr/share/sdkwork/webserver/web/pc/index.html',
    '/usr/share/sdkwork/webserver/web/h5/index.html',
    '/usr/share/sdkwork/webserver/web/static/index.html',
    '/var/lib/sdkwork/webserver/iam',
  ];
  for (const required of requiredPaths) {
    if (!contents.includes(required)) {
      throw new Error(`installer is missing required path ${required}`);
    }
  }
  if (environment.certificateWorker
    && !contents.includes('/usr/lib/systemd/system/sdkwork-webserver-certificate-worker.service')) {
    throw new Error('installer is missing the certificate worker unit');
  }
  const scripts = runRpm(['-qp', '--scripts', wslOrNative(rpmPath)]).stdout;
  for (const script of ['postinstall scriptlet', 'preuninstall scriptlet', 'postuninstall scriptlet']) {
    if (!scripts.includes(script)) {
      throw new Error(`installer is missing ${script}`);
    }
  }
  console.log(
    `[sdkwork-webserver-rpm] validated package=${environment.packageName} version=${settings.version} bytes=${statSync(rpmPath).size}`,
  );
  return rpmPath;
}

// Keep sdkwork.app.config.json installConfig checksums in sync with the built
// artifact (security.checksumRequired for enabled packages).
function updateAppManifestChecksum(settings, rpmPath, { skip = false } = {}) {
  if (skip) {
    return;
  }
  const manifestPath = path.join(REPO_ROOT, 'sdkwork.app.config.json');
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const packageId = RPM_PACKAGE_IDS[settings.environment];
  const pkg = manifest?.artifacts?.installConfig?.packages?.find(
    (candidate) => candidate.id === packageId,
  );
  if (!pkg) {
    throw new Error(`sdkwork.app.config.json is missing package ${packageId}`);
  }
  pkg.checksumAlgorithm = 'sha256';
  pkg.checksum = sha256File(rpmPath);
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o644 });
  console.log(`[sdkwork-webserver-rpm] updated app manifest checksum package=${packageId}`);
}

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    console.log(
      'Usage: node scripts/webserver-rpm.mjs <package|validate> --environment <test|production> [--architecture <x64|arm64>] [--version <semver>] [--dry-run]',
    );
    return;
  }
  console.log(
    `[sdkwork-webserver-rpm] operation=${settings.operation} environment=${settings.environment} architecture=${settings.architecture} version=${settings.version}`,
  );
  if (settings.operation === 'package') {
    const { stageContainer, specPath, rpmArchitecture } = await assembleRpmStage(settings);
    try {
      const rpmPath = buildRpm(settings, stageContainer, specPath, rpmArchitecture);
      console.log(`[sdkwork-webserver-rpm] wrote ${path.relative(REPO_ROOT, rpmPath)}`);
      if (!settings.dryRun) {
        validateRpm(settings);
        updateAppManifestChecksum(settings, rpmPath);
      }
    } finally {
      rmSync(stageContainer, { recursive: true, force: true });
    }
  } else {
    validateRpm(settings);
  }
}

main().catch((error) => {
  process.stderr.write(
    `[sdkwork-webserver-rpm] ${error instanceof Error ? error.message : String(error)}\n`,
  );
  process.exitCode = 1;
});
