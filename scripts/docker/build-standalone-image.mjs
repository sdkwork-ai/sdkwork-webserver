#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
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
import { extract as extractTar } from 'tar';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const WORKSPACE_ROOT = path.resolve(REPO_ROOT, '..');
const DOCKER_ROOT = path.join(REPO_ROOT, 'deployments', 'docker');
const RELEASE_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const STAGE_ROOT = path.join(REPO_ROOT, '.sdkwork', 'runtime', 'docker-standalone-context');
const CLOUD_GATEWAY_ROOT = path.join(WORKSPACE_ROOT, 'sdkwork-api-cloud-gateway');
const CLOUD_GATEWAY_BUILD_SCRIPT = path.join(
  CLOUD_GATEWAY_ROOT,
  'scripts',
  'build-api-cloud-gateway-container.mjs',
);
const CLOUD_GATEWAY_INSTALL_DIR = path.join(CLOUD_GATEWAY_ROOT, 'dist', 'container-image-build');
const CLOUD_GATEWAY_BINARY_NAME = 'sdkwork-api-cloud-gateway';
// Sibling module databases staged into the image so the standalone image is
// self-contained: compose overlays mount the same directories from workspace
// checkouts, but the image itself must serve every declared module app root.
// Mirrors DEPENDENCY_RUNTIME_ASSETS in scripts/webserver-release.mjs.
const EMBEDDED_MODULE_DATABASES = [
  { repo: 'sdkwork-skills', shareName: 'skills' },
  { repo: 'sdkwork-mcp', shareName: 'mcp' },
  { repo: 'sdkwork-deployments', shareName: 'deploy' },
];
const SUPPORTED_ENVIRONMENTS = ['development', 'test', 'staging', 'production'];

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
    architecture: 'x64',
    // Lifecycle environment of the static bundle packaged into the image.
    // Default is the production (线上) bundle; the container entrypoint
    // rewrites runtime-env.json per active environment at startup, and the
    // environment-agnostic image plus per-environment env files remain the
    // release contract (ENVIRONMENT_SPEC.md §5.1.0.1).
    environment: 'production',
    version: appVersion(),
    tag: undefined,
    skipReleaseBuild: false,
    skipPlatformGateway: false,
    skipPlatformGatewayBuild: false,
    dryRun: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--architecture') {
      settings.architecture = argv[++index];
    } else if (argument === '--environment') {
      settings.environment = argv[++index];
      if (!SUPPORTED_ENVIRONMENTS.includes(settings.environment)) {
        throw new Error(
          `--environment must be ${SUPPORTED_ENVIRONMENTS.join(', ')}`,
        );
      }
    } else if (argument === '--version') {
      settings.version = argv[++index];
    } else if (argument === '--tag') {
      settings.tag = argv[++index];
    } else if (argument === '--skip-release-build') {
      settings.skipReleaseBuild = true;
    } else if (argument === '--skip-platform-gateway') {
      settings.skipPlatformGateway = true;
    } else if (argument === '--skip-platform-gateway-build') {
      settings.skipPlatformGatewayBuild = true;
    } else if (argument === '--dry-run') {
      settings.dryRun = true;
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  settings.tag ??= settings.version;
  return settings;
}

function printHelp() {
  console.log(`Usage: node scripts/docker/build-standalone-image.mjs [options]

Build the standalone gateway container image from the verified release archive.

Options:
  --architecture <x64|arm64>   Default: x64
  --environment <env>          Lifecycle environment of the packaged static
                               bundle (development|test|staging|production).
                               Default: production
  --version <semver>           Default: sdkwork.app.config.json release.currentVersion
  --tag <image-tag>            Default: release version
  --skip-release-build         Do not invoke release packaging when the archive is missing
  --skip-platform-gateway      Omit bundled sdkwork-api-cloud-gateway from the image (use docker mode)
  --skip-platform-gateway-build  Require prebuilt cloud-gateway artifacts; do not invoke build:container
  --dry-run                    Print the resolved build plan only
  --help, -h                   Show this help`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    env: options.env ?? process.env,
    stdio: options.capture ? 'pipe' : 'inherit',
    timeout: options.timeoutMs ?? 30 * 60 * 1000,
    maxBuffer: 16 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message ?? result.stderr?.trim() ?? `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result;
}

function archivePath(settings) {
  return path.join(
    RELEASE_OUTPUT_ROOT,
    `sdkwork-webserver-linux-${settings.architecture}-standalone-server-${settings.version}.tar.gz`,
  );
}

function copyTree(source, target) {
  const stack = [{ from: source, to: target }];
  while (stack.length > 0) {
    const current = stack.pop();
    const stat = statSync(current.from);
    if (stat.isDirectory()) {
      mkdirSync(current.to, { recursive: true });
      for (const entry of readdirSync(current.from)) {
        stack.push({ from: path.join(current.from, entry), to: path.join(current.to, entry) });
      }
    } else {
      mkdirSync(path.dirname(current.to), { recursive: true });
      writeFileSync(current.to, readFileSync(current.from));
      // Preserve POSIX modes (executable bits on release binaries); plain
      // writeFileSync would strip them and break container startup.
      chmodSync(current.to, statSync(current.from).mode & 0o777);
    }
  }
}

function cloudGatewayBinaryPath() {
  return path.join(CLOUD_GATEWAY_INSTALL_DIR, 'bin', CLOUD_GATEWAY_BINARY_NAME);
}

function ensurePlatformGatewayInstallTree(settings) {
  if (settings.skipPlatformGateway) {
    console.log('skipping bundled sdkwork-api-cloud-gateway (SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker expected)');
    return null;
  }
  const binary = cloudGatewayBinaryPath();
  const installRoot = CLOUD_GATEWAY_INSTALL_DIR;
  if (!existsSync(binary) || !existsSync(path.join(installRoot, 'database-modules'))) {
    if (settings.skipPlatformGatewayBuild) {
      throw new Error(
        `bundled platform gateway artifacts missing at ${installRoot}; ` +
          'build sdkwork-api-cloud-gateway (pnpm build:container) or pass --skip-platform-gateway',
      );
    }
    if (!existsSync(CLOUD_GATEWAY_BUILD_SCRIPT)) {
      throw new Error(
        `missing ${CLOUD_GATEWAY_BUILD_SCRIPT}; clone sdkwork-api-cloud-gateway beside sdkwork-webserver`,
      );
    }
    console.log('building sdkwork-api-cloud-gateway container install tree (pnpm build:container --skip-build when possible)');
    const buildArgs = [CLOUD_GATEWAY_BUILD_SCRIPT];
    if (existsSync(binary)) {
      buildArgs.push('--skip-build');
    }
    run('node', buildArgs, { cwd: CLOUD_GATEWAY_ROOT });
  }
  if (!existsSync(binary)) {
    throw new Error(`platform gateway binary missing after build: ${binary}`);
  }
  if (!existsSync(path.join(installRoot, 'database-modules'))) {
    throw new Error(`platform gateway install root missing database-modules: ${installRoot}`);
  }
  return { binary, installRoot };
}

function stagePlatformGatewayInstall(settings) {
  const stagedInstallRoot = path.join(STAGE_ROOT, 'opt', 'sdkwork', 'api-gateway');
  const artifacts = ensurePlatformGatewayInstallTree(settings);
  if (!artifacts) {
    mkdirSync(stagedInstallRoot, { recursive: true });
    writeFileSync(
      path.join(stagedInstallRoot, '.bundled-gateway-omitted'),
      'SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker or mount artifacts for bundled mode\n',
    );
    return null;
  }
  const stagedBinary = path.join(STAGE_ROOT, 'bin', CLOUD_GATEWAY_BINARY_NAME);
  copyTree(artifacts.binary, stagedBinary);
  copyTree(artifacts.installRoot, stagedInstallRoot);
  console.log(`staged bundled platform gateway: ${stagedBinary} + ${stagedInstallRoot}`);
  return { stagedBinary, stagedInstallRoot };
}

async function ensureReleaseArchive(settings) {
  const archive = archivePath(settings);
  if (existsSync(archive)) {
    return archive;
  }
  if (settings.skipReleaseBuild) {
    throw new Error(`release archive is missing: ${archive}`);
  }
  run('node', [
    path.join(REPO_ROOT, 'scripts', 'webserver-release.mjs'),
    'package',
    '--deployment-profile',
    'standalone',
    '--architecture',
    settings.architecture,
    '--environment',
    settings.environment,
    '--version',
    settings.version,
  ]);
  if (!existsSync(archive)) {
    throw new Error(`release archive was not produced: ${archive}`);
  }
  return archive;
}

async function stageContext(settings) {
  const archive = await ensureReleaseArchive(settings);
  rmSync(STAGE_ROOT, { recursive: true, force: true });
  mkdirSync(STAGE_ROOT, { recursive: true });
  const extractRoot = path.join(STAGE_ROOT, '_extract');
  mkdirSync(extractRoot, { recursive: true });
  await extractTar({ file: archive, cwd: extractRoot });
  const bundleRoot = path.join(extractRoot, 'sdkwork-webserver');
  if (!existsSync(bundleRoot)) {
    throw new Error(`release archive did not contain sdkwork-webserver/: ${archive}`);
  }
  copyTree(bundleRoot, STAGE_ROOT);
  const repoDatabase = path.join(REPO_ROOT, 'database');
  if (existsSync(repoDatabase)) {
    copyTree(repoDatabase, path.join(STAGE_ROOT, 'database'));
  }
  for (const module of EMBEDDED_MODULE_DATABASES) {
    const databaseSource = path.join(WORKSPACE_ROOT, module.repo, 'database');
    if (!existsSync(databaseSource)) {
      throw new Error(
        `missing ${module.repo} database module at ${databaseSource}; clone sibling repo before building the standalone image`,
      );
    }
    copyTree(
      databaseSource,
      path.join(STAGE_ROOT, 'share', 'sdkwork', module.shareName, 'database'),
    );
  }
  writeFileSync(
    path.join(STAGE_ROOT, 'entrypoint-standalone.sh'),
    readFileSync(path.join(DOCKER_ROOT, 'scripts', 'entrypoint-standalone.sh')),
    { mode: 0o755 },
  );
  const platformGateway = stagePlatformGatewayInstall(settings);
  rmSync(extractRoot, { recursive: true, force: true });
  return {
    archive,
    dockerfile: path.join(DOCKER_ROOT, 'Dockerfile.standalone'),
    image: `registry.sdkwork.com/apps/sdkwork-webserver-standalone:${settings.tag}`,
    platformGateway,
  };
}

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    return;
  }
  const plan = await stageContext(settings);
  const buildArgs = [
    'build',
    '--pull',
    '--file',
    plan.dockerfile,
    '--tag',
    plan.image,
    STAGE_ROOT,
  ];
  console.log(`release archive: ${plan.archive}`);
  console.log(`static bundle environment: ${settings.environment}`);
  console.log(`docker context: ${STAGE_ROOT}`);
  console.log(`image tag: ${plan.image}`);
  if (plan.platformGateway) {
    console.log(`bundled platform gateway: ${plan.platformGateway.stagedBinary}`);
  } else {
    console.log('bundled platform gateway: omitted (--skip-platform-gateway or build skipped)');
  }
  if (settings.dryRun) {
    console.log(`docker ${buildArgs.join(' ')}`);
    return;
  }
  // The executed command must match the printed plan exactly: `--pull` stays
  // in buildArgs so dry-run output and the real build can never diverge.
  run('docker', buildArgs);
  console.log(`built ${plan.image}`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
