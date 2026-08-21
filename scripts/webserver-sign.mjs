#!/usr/bin/env node

import {
  closeSync,
  existsSync,
  fsyncSync,
  lstatSync,
  openSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { sign, verify } from 'sigstore';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const MAX_ARCHIVE_BYTES = 512 * 1024 * 1024;
const MAX_BUNDLE_BYTES = 2 * 1024 * 1024;
const OFFICIAL_WORKFLOW_IDENTITY =
  '^https://github\\.com/sdkwork-ai/sdkwork-webserver/\\.github/workflows/package\\.yml@refs/(?:heads|tags|pull)/.+$';
const GITHUB_OIDC_ISSUER = 'https://token.actions.githubusercontent.com';

function parseArgs(argv) {
  const settings = {
    operation: argv[0],
    help: argv[0] === '--help' || argv[0] === '-h',
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

function requiredToken(value, pattern, label) {
  const normalized = value?.trim();
  if (!normalized || !pattern.test(normalized)) {
    throw new Error(`${label} is missing or invalid`);
  }
  return normalized;
}

function resolveArtifact(settings) {
  const deploymentProfile = requiredToken(
    settings.deploymentProfile ?? process.env.SDKWORK_DEPLOYMENT_PROFILE,
    /^(?:standalone|cloud)$/u,
    'deployment profile',
  );
  const architecture = requiredToken(
    settings.architecture ?? process.env.SDKWORK_PACKAGE_ARCHITECTURE,
    /^(?:x64|arm64)$/u,
    'package architecture',
  );
  const version = requiredToken(
    settings.version ?? process.env.SDKWORK_PACKAGE_VERSION,
    /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/u,
    'package version',
  );
  const basename = `sdkwork-webserver-linux-${architecture}-${deploymentProfile}-server-${version}.tar.gz`;
  return {
    archive: path.join(OUTPUT_ROOT, basename),
    bundle: path.join(OUTPUT_ROOT, `${basename}.sigstore.json`),
  };
}

function readRegularFile(filePath, maxBytes, label) {
  if (!existsSync(filePath)) {
    throw new Error(`${label} does not exist: ${path.relative(REPO_ROOT, filePath)}`);
  }
  const stat = lstatSync(filePath);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size < 1 || stat.size > maxBytes) {
    throw new Error(`${label} must be a non-empty regular non-symlink file within ${maxBytes} bytes`);
  }
  return readFileSync(filePath);
}

function writeAtomic(filePath, content) {
  if (existsSync(filePath)) {
    throw new Error(`signature bundle already exists: ${path.relative(REPO_ROOT, filePath)}`);
  }
  const temporary = `${filePath}.tmp-${process.pid}`;
  rmSync(temporary, { force: true });
  try {
    writeFileSync(temporary, content, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
    const descriptor = openSync(temporary, 'r');
    try {
      fsyncSync(descriptor);
    } finally {
      closeSync(descriptor);
    }
    renameSync(temporary, filePath);
  } finally {
    rmSync(temporary, { force: true });
  }
}

async function signArtifact(artifact) {
  if (
    !process.env.SIGSTORE_ID_TOKEN &&
    (!process.env.ACTIONS_ID_TOKEN_REQUEST_URL || !process.env.ACTIONS_ID_TOKEN_REQUEST_TOKEN)
  ) {
    throw new Error('Sigstore signing requires GitHub Actions OIDC or SIGSTORE_ID_TOKEN');
  }
  const payload = readRegularFile(artifact.archive, MAX_ARCHIVE_BYTES, 'release archive');
  const bundle = await sign(payload, { tlogUpload: true });
  writeAtomic(artifact.bundle, `${JSON.stringify(bundle)}\n`);
  console.log(`[sdkwork-webserver-sign] signed ${path.basename(artifact.archive)}`);
}

async function verifyArtifact(artifact) {
  const payload = readRegularFile(artifact.archive, MAX_ARCHIVE_BYTES, 'release archive');
  const bundleBytes = readRegularFile(artifact.bundle, MAX_BUNDLE_BYTES, 'Sigstore bundle');
  let bundle;
  try {
    bundle = JSON.parse(bundleBytes.toString('utf8'));
  } catch (error) {
    throw new Error(`Sigstore bundle is not valid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
  await verify(bundle, payload, {
    certificateIssuer: GITHUB_OIDC_ISSUER,
    certificateIdentityURI: OFFICIAL_WORKFLOW_IDENTITY,
    ctLogThreshold: 1,
    tlogThreshold: 1,
  });
  console.log(`[sdkwork-webserver-sign] verified ${path.basename(artifact.archive)}`);
}

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    console.log(
      'Usage: node scripts/webserver-sign.mjs <sign|verify> --deployment-profile <standalone|cloud> --architecture <x64|arm64> --version <semver>',
    );
    return;
  }
  if (!['sign', 'verify'].includes(settings.operation)) {
    throw new Error('operation must be sign or verify');
  }
  const artifact = resolveArtifact(settings);
  if (settings.operation === 'sign') {
    await signArtifact(artifact);
  } else {
    await verifyArtifact(artifact);
  }
}

main().catch((error) => {
  process.stderr.write(`[sdkwork-webserver-sign] ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
