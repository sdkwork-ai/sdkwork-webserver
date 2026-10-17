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
import { bundleToJSON } from '@sigstore/bundle';
import {
  CIContextProvider,
  DEFAULT_REKOR_URL,
  FulcioSigner,
  MessageSignatureBundleBuilder,
  RekorWitness,
} from '@sigstore/sign';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { verify } from 'sigstore';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const MAX_ARCHIVE_BYTES = 512 * 1024 * 1024;
const MAX_BUNDLE_BYTES = 2 * 1024 * 1024;
// Packaging runs through the pinned reusable workflow
// sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@<sha>,
// so GitHub mints the Sigstore certificate identity from the *reusable*
// workflow path, not from this repository's package.yml. Both are legitimate
// first-party identities for a release artifact and both are accepted here:
//   1. this repository's own workflow, referenced by ref;
//   2. the organization-owned reusable workflow, referenced by full commit SHA
//      (the same SHA this repository pins in package.yml).
// The alternation stays pinned to the sdkwork-ai organization, so a
// certificate from any other repository, owner, or host is still rejected.
// certificateIssuer remains pinned to GitHub's OIDC issuer, which is what
// actually proves the workflow ran inside GitHub Actions.
const OFFICIAL_WORKFLOW_IDENTITIES = [
  '^https://github\\.com/sdkwork-ai/sdkwork-webserver/\\.github/workflows/package\\.yml@refs/(?:heads|tags|pull)/.+$',
  '^https://github\\.com/sdkwork-ai/sdkwork-github-workflow/\\.github/workflows/sdkwork-package\\.yml@[0-9a-f]{40}$',
].join('|');
const GITHUB_OIDC_ISSUER = 'https://token.actions.githubusercontent.com';
// Release archives are built reproducibly (SOURCE_DATE_EPOCH plus a pinned
// tar mtime), so an unchanged commit produces a byte-identical archive and
// therefore an identical digest on every attempt. Rekor deduplicates entries
// by (identity, digest), so a re-run legitimately hits
// `409 an equivalent entry already exists`.
//
// The top-level `sign()` helper from the `sigstore` package hardcodes
// `fetchOnConflict: false` (node_modules/sigstore/dist/config.js) and its
// SignOptions type does not expose the flag, so that entry point always treats
// a re-run as a fatal error. @sigstore/sign exports the same builder with the
// flag settable, so the witness is assembled here instead. `fetchOnConflict`
// makes the client fetch the existing entry by UUID and reuse it, which is the
// correct outcome: the existing tlog entry already attests this exact digest.
const REKOR_FETCH_ON_CONFLICT = true;
// `fetchOnConflict` only settles the 409 "an equivalent entry already exists"
// case, which is deterministic for a reproducible archive. It says nothing
// about Rekor being briefly unavailable: a 502/503/504 from the transparency
// log or the Fulcio CA surfaces as a plain rejection and fails the whole
// platform job, even though nothing about the artifact is wrong. Retry the
// transient classes with exponential backoff and let everything else —
// including the 409, which the witness already handles — fail immediately so a
// real rejection is never masked by six minutes of waiting.
const SIGN_RETRY_ATTEMPTS = 5;
const SIGN_RETRY_BASE_DELAY_MS = 4_000;
const SIGN_RETRY_MAX_DELAY_MS = 60_000;
const TRANSIENT_STATUS_CODES = new Set([408, 425, 429, 500, 502, 503, 504]);

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

// Matches the status code out of whatever the client library wrapped the
// failure in: `fetch` surfaces "502 Bad Gateway" in the message while a
// structured error keeps `status`/`statusCode`/`code`.
function transientStatusOf(error) {
  const candidates = [error?.status, error?.statusCode, error?.code, error?.response?.status];
  for (const candidate of candidates) {
    if (typeof candidate === 'number' && TRANSIENT_STATUS_CODES.has(candidate)) {
      return candidate;
    }
    if (typeof candidate === 'string') {
      const parsed = Number.parseInt(candidate, 10);
      if (Number.isInteger(parsed) && TRANSIENT_STATUS_CODES.has(parsed)) {
        return parsed;
      }
    }
  }
  const message = error instanceof Error ? error.message : String(error ?? '');
  const match = /\b(408|425|429|500|502|503|504)\b/u.exec(message);
  return match ? Number.parseInt(match[1], 10) : null;
}

async function withTransientRetry(operation, describe) {
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      const status = transientStatusOf(error);
      if (status === null || attempt >= SIGN_RETRY_ATTEMPTS) {
        throw error;
      }
      const delay = Math.min(
        SIGN_RETRY_BASE_DELAY_MS * 2 ** (attempt - 1),
        SIGN_RETRY_MAX_DELAY_MS,
      );
      const message = error instanceof Error ? error.message : String(error);
      console.warn(
        `[sdkwork-webserver-sign] ${describe} hit a transient ${status} `
        + `(${attempt}/${SIGN_RETRY_ATTEMPTS}); retrying in ${delay}ms: ${message}`,
      );
      await sleep(delay);
    }
  }
}

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
    /^(?:standalone)$/u,
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
  const identityProvider = process.env.SIGSTORE_ID_TOKEN
    ? { getToken: async () => process.env.SIGSTORE_ID_TOKEN }
    : new CIContextProvider('sigstore');
  const builder = new MessageSignatureBundleBuilder({
    signer: new FulcioSigner({ identityProvider }),
    witnesses: [
      new RekorWitness({
        rekorBaseURL: DEFAULT_REKOR_URL,
        fetchOnConflict: REKOR_FETCH_ON_CONFLICT,
      }),
    ],
  });
  const bundle = bundleToJSON(
    await withTransientRetry(
      () => builder.create({ data: payload }),
      'sigstore signing',
    ),
  );
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
  await withTransientRetry(
    () => verify(bundle, payload, {
      certificateIssuer: GITHUB_OIDC_ISSUER,
      certificateIdentityURI: OFFICIAL_WORKFLOW_IDENTITIES,
      ctLogThreshold: 1,
      tlogThreshold: 1,
    }),
    'sigstore verification',
  );
  console.log(`[sdkwork-webserver-sign] verified ${path.basename(artifact.archive)}`);
}

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    console.log(
      'Usage: node scripts/webserver-sign.mjs <sign|verify> --deployment-profile <standalone> --architecture <x64|arm64> --version <semver>',
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
