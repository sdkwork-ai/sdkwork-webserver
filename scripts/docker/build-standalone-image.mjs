#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import {
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
const DOCKER_ROOT = path.join(REPO_ROOT, 'deployments', 'docker');
const RELEASE_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const STAGE_ROOT = path.join(REPO_ROOT, '.sdkwork', 'runtime', 'docker-standalone-context');

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
    version: appVersion(),
    tag: undefined,
    skipReleaseBuild: false,
    dryRun: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--architecture') {
      settings.architecture = argv[++index];
    } else if (argument === '--version') {
      settings.version = argv[++index];
    } else if (argument === '--tag') {
      settings.tag = argv[++index];
    } else if (argument === '--skip-release-build') {
      settings.skipReleaseBuild = true;
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
  --version <semver>           Default: sdkwork.app.config.json release.currentVersion
  --tag <image-tag>            Default: release version
  --skip-release-build         Do not invoke release packaging when the archive is missing
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
    }
  }
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
  writeFileSync(
    path.join(STAGE_ROOT, 'entrypoint-standalone.sh'),
    readFileSync(path.join(DOCKER_ROOT, 'scripts', 'entrypoint-standalone.sh')),
    { mode: 0o755 },
  );
  rmSync(extractRoot, { recursive: true, force: true });
  return {
    archive,
    dockerfile: path.join(DOCKER_ROOT, 'Dockerfile.standalone'),
    image: `registry.sdkwork.com/apps/sdkwork-webserver-standalone:${settings.tag}`,
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
  console.log(`docker context: ${STAGE_ROOT}`);
  console.log(`image tag: ${plan.image}`);
  if (settings.dryRun) {
    console.log(`docker ${buildArgs.join(' ')}`);
    return;
  }
  run('docker', buildArgs);
  console.log(`built ${plan.image}`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
