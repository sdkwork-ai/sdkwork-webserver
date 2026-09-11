#!/usr/bin/env node

// Package the unified sdkwork-webserver Docker install bundle
// (PNPM_SCRIPT_SPEC.md §4.4 / DEPLOYMENT_SPEC.md §6):
//
//   dist/docker-install/sdkwork-webserver-install-<version>.bundle/
//     image.tar.gz / image.sha256 / image.env
//     compose/docker-compose.bundle.yml       environment-neutral multi-instance
//     compose/docker-compose.bundle-edge.yml  instance-1 80/443 edge overlay
//     env/{development,test,production}.env.example
//     deploy.sh / manifest.json / README.md
//
// The image is built once (delegating to build-standalone-image.mjs) and
// carries no environment binding; environments and instance counts are
// deployment inputs resolved by the bundle deploy.sh at container start.

import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFileSync,
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
import { pipeline } from 'node:stream/promises';
import { createReadStream, createWriteStream } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createGzip } from 'node:zlib';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const DOCKER_ROOT = path.join(REPO_ROOT, 'deployments', 'docker');
// bin/ is the single script source root (MODULE_BIN_SPEC.md §1/§2.1): the bundle
// receives copies of the executors and the postgres init scripts the build makes.
const BIN_ROOT = path.join(REPO_ROOT, 'bin');
const DEFAULT_OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'docker-install');
// staging is a first-class deployment environment (DEPLOYMENT_SPEC §2) and
// ships in the bundle so production-like rehearsal uses the same installer.
const DEFAULT_ENVIRONMENTS = ['development', 'test', 'staging', 'demo', 'production'];

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
    version: appVersion(),
    tag: undefined,
    outputRoot: DEFAULT_OUTPUT_ROOT,
    skipImageBuild: false,
    skipPlatformGateway: false,
    dryRun: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--version') {
      settings.version = argv[++index];
    } else if (argument === '--tag') {
      settings.tag = argv[++index];
    } else if (argument === '--out') {
      settings.outputRoot = path.resolve(argv[++index]);
    } else if (argument === '--skip-image-build') {
      settings.skipImageBuild = true;
    } else if (argument === '--skip-platform-gateway') {
      settings.skipPlatformGateway = true;
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
  console.log(`Usage: node scripts/docker/package-install-bundle.mjs [options]

Build the unified install image and package the self-contained install bundle.

Options:
  --version <semver>          Default: sdkwork.app.config.json release.currentVersion
  --tag <image-tag>           Default: release version
  --out <dir>                 Output root (default: dist/docker-install)
  --skip-image-build          Require an already-built image; do not invoke docker build
  --skip-platform-gateway     Forwarded to build-standalone-image.mjs (docker gateway mode)
  --dry-run                   Print the resolved plan only
  --help, -h                  Show this help`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    stdio: options.stdio ?? 'inherit',
    timeout: options.timeoutMs ?? 60 * 60 * 1000,
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message ?? `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result;
}

function imageRef(tag) {
  return `registry.sdkwork.com/apps/sdkwork-webserver-standalone:${tag}`;
}

function imageExists(ref) {
  return spawnSync('docker', ['image', 'inspect', ref], {
    stdio: 'ignore',
    windowsHide: true,
  }).status === 0;
}

async function buildImage(settings) {
  const ref = imageRef(settings.tag);
  if (imageExists(ref)) {
    console.log(`image already present: ${ref}`);
    return ref;
  }
  if (settings.skipImageBuild) {
    throw new Error(`image missing and --skip-image-build set: ${ref}`);
  }
  const args = [
    path.join('scripts', 'docker', 'build-standalone-image.mjs'),
    '--tag',
    settings.tag,
  ];
  if (settings.skipPlatformGateway) {
    args.push('--skip-platform-gateway');
  }
  if (process.env.SDKWORK_DOCKER_NO_PULL === '1') {
    args.push('--no-pull');
  }
  run('node', args);
  if (!imageExists(ref)) {
    throw new Error(`image was not produced: ${ref}`);
  }
  return ref;
}

async function saveImage(ref, destination) {
  // Stream docker save through gzip (cross-platform; no shell pipeline).
  const child = spawn('docker', ['save', ref], {
    stdio: ['ignore', 'pipe', 'inherit'],
    windowsHide: true,
  });
  // Attach the exit listener BEFORE awaiting the pipeline: a fast `docker
  // save` can exit while the gzip pipeline is still flushing, and an exit
  // listener attached afterwards never fires — the await would hang on a
  // dead promise and node would exit 0 with the bundle half-written.
  const childExited = new Promise((resolve, reject) => {
    child.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`docker save exited ${code}`))));
    child.on('error', reject);
  });
  await pipeline(child.stdout, createGzip({ level: 1 }), createWriteStream(destination));
  await childExited;
}

async function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('error', reject);
    stream.on('end', () => resolve(hash.digest('hex')));
  });
}

function copyDir(source, target) {
  mkdirSync(target, { recursive: true });
  for (const entry of readdirSync(source)) {
    const from = path.join(source, entry);
    if (statSync(from).isDirectory()) {
      copyDir(from, path.join(target, entry));
    } else {
      copyFileSync(from, path.join(target, entry));
    }
  }
}

const BUNDLE_README = `# sdkwork-webserver install bundle

Self-contained deployment bundle for the unified install image
(DEPLOYMENT_SPEC.md §6 / PNPM_SCRIPT_SPEC.md §4.4).

Quick start on any Docker host:

    bash deploy.sh --environment development            # external host-system postgres/redis (default)
    bash deploy.sh --environment production --replicas 3
    bash deploy.sh --environment production --embedded --replicas 2
    bash deploy.sh --environment test --down --purge

1. Copy env/<environment>.env.example to env/<environment>.env and fill secrets.
2. Default dependency mode is EXTERNAL: the webserver connects to the docker
   host's own PostgreSQL (host.docker.internal:5432) and Redis
   (host.docker.internal:6379). Provision host identities with
   bin/host/setup-host-external-deps.sh (WSL/Ubuntu).
3. deploy.sh loads image.tar.gz, then starts instances 1..N (instance 1 owns
   the 80/443 edge and runs migrations first). Use --embedded to run built-in
   postgres/redis containers instead.
4. Management ports are base..base+N-1 -> 3800; load-balance across instances
   on those ports.

Full guide: docs/guides/operator/docker-install.md (repo) / docker-install.en.md (English).
`;

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    return;
  }
  // --dry-run reports what would be produced. It must not build the image:
  // an expensive multi-minute docker build behind a "dry" flag defeats the
  // point of being able to inspect the bundle plan offline.
  const bundleDir = path.join(
    settings.outputRoot,
    `sdkwork-webserver-install-${settings.version}.bundle`,
  );
  if (settings.dryRun) {
    const ref = imageRef(settings.tag);
    console.log(`bundle directory: ${bundleDir}`);
    console.log(`docker save ${ref} | gzip -> ${path.join(bundleDir, 'image.tar.gz')}`);
    console.log(`compose templates: ${DOCKER_ROOT}/docker-compose.bundle*.yml`);
    console.log(`env templates: ${DOCKER_ROOT}/env/*.env.example`);
    console.log(`deploy script: ${BIN_ROOT}/docker-bundle-deploy.sh -> deploy.sh`);
    console.log(`release script: ${BIN_ROOT}/docker-bundle-release.sh -> release.sh`);
    console.log(`postgres init: ${BIN_ROOT}/container/postgres-init`);
    return;
  }

  const ref = await buildImage(settings);
  console.log(`bundle directory: ${bundleDir}`);

  rmSync(bundleDir, { recursive: true, force: true });
  mkdirSync(path.join(bundleDir, 'compose'), { recursive: true });
  mkdirSync(path.join(bundleDir, 'env'), { recursive: true });
  copyDir(path.join(BIN_ROOT, 'container', 'postgres-init'), path.join(bundleDir, 'postgres', 'init'));

  console.log(`saving image ${ref} (gzip, this can take several minutes)...`);
  const imageTgz = path.join(bundleDir, 'image.tar.gz');
  await saveImage(ref, imageTgz);
  const digest = await sha256File(imageTgz);
  writeFileSync(path.join(bundleDir, 'image.sha256'), `${digest}  image.tar.gz\n`);

  writeFileSync(
    path.join(bundleDir, 'image.env'),
    `SDKWORK_WEBSERVER_IMAGE_TAG=${settings.tag}\n`,
  );

  copyFileSync(
    path.join(DOCKER_ROOT, 'docker-compose.bundle.yml'),
    path.join(bundleDir, 'compose', 'docker-compose.bundle.yml'),
  );
  copyFileSync(
    path.join(DOCKER_ROOT, 'docker-compose.bundle-edge.yml'),
    path.join(bundleDir, 'compose', 'docker-compose.bundle-edge.yml'),
  );
  copyFileSync(
    path.join(DOCKER_ROOT, 'docker-compose.bundle-gateway.yml'),
    path.join(bundleDir, 'compose', 'docker-compose.bundle-gateway.yml'),
  );
  for (const environment of DEFAULT_ENVIRONMENTS) {
    copyFileSync(
      path.join(DOCKER_ROOT, 'env', `${environment}.env.example`),
      path.join(bundleDir, 'env', `${environment}.env.example`),
    );
  }
  // Source names are flat (MODULE_BIN_SPEC.md §2.2); the bundle root names are
  // fixed by DOCKER_SPEC.md §4.1 / APPLICATION_DEPLOY_LAYOUT_SPEC.md §9.1.
  const deployScript = path.join(bundleDir, 'deploy.sh');
  copyFileSync(path.join(BIN_ROOT, 'docker-bundle-deploy.sh'), deployScript);
  // Bundle-owned release channel (OPERATIONS_SPEC.md §1.2): versioned
  // deploy/rollback with a health gate, ledger, and release lock.
  const releaseScript = path.join(bundleDir, 'release.sh');
  copyFileSync(path.join(BIN_ROOT, 'docker-bundle-release.sh'), releaseScript);
  for (const script of [deployScript, releaseScript]) {
    const mode = statSync(script).mode | 0o755;
    const chmod = spawnSync('chmod', ['0755', script], { stdio: 'ignore' });
    if (chmod.status !== 0) {
      // Windows filesystems ignore the POSIX mode; harmless.
      void mode;
    }
  }

  writeFileSync(
    path.join(bundleDir, 'manifest.json'),
    `${JSON.stringify(
      {
        kind: 'sdkwork.component-deployment-bundle',
        application: 'sdkwork-webserver',
        deploymentProfile: 'standalone',
        version: settings.version,
        image: ref,
        imageTag: settings.tag,
        imageSha256: digest,
        environments: DEFAULT_ENVIRONMENTS,
        multiInstance: true,
        createdAt: new Date().toISOString(),
        spec: ['DEPLOYMENT_SPEC.md#6', 'OPERATIONS_SPEC.md#1.2', 'PNPM_SCRIPT_SPEC.md#4.4'],
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(path.join(bundleDir, 'README.md'), BUNDLE_README);

  console.log(`packaged ${bundleDir}`);
  console.log(`  image:    ${ref}`);
  console.log(`  sha256:   ${digest}`);
  console.log(`  deploy:   bash deploy.sh --environment <env> [--replicas N]`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
