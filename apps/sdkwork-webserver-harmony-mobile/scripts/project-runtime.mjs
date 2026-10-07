#!/usr/bin/env node
// Projects exactly one runtime-env profile into the Harmony HAP rawfile.
//
// Authority: `config/app/runtime-env.<profileId>.json` (`CONFIG_SPEC.md`,
// `ENVIRONMENT_SPEC.md`, `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §9). The
// entry module reads `resources/rawfile/runtime-env.json` at bootstrap; this
// script is the build step that selects which committed profile is baked in —
// the same shape the mini-program root uses (`build-runtime.mjs`), without a
// second host-writing fallback in ArkTS.
//
// Usage: node scripts/project-runtime.mjs --profile standalone.production

import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const RAWFILE_TARGET = path.join(
  APP_ROOT,
  'entry',
  'src',
  'main',
  'resources',
  'rawfile',
  'runtime-env.json',
);
const PROFILE_ID_PATTERN = /^[a-z][a-z0-9]*(?:\.[a-z][a-z0-9]*)+$/;

function parseArgs(argv) {
  let profileId;
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === '--profile') {
      profileId = argv[++index];
    } else {
      throw new Error(`unsupported argument: ${argv[index]}`);
    }
  }
  if (typeof profileId !== 'string' || !PROFILE_ID_PATTERN.test(profileId)) {
    throw new Error(
      `--profile must be a dotted lowercase profile id (e.g. standalone.production), got ${profileId}`,
    );
  }
  return { profileId };
}

function main() {
  const { profileId } = parseArgs(process.argv.slice(2));
  const source = path.join(APP_ROOT, 'config', 'app', `runtime-env.${profileId}.json`);
  if (!existsSync(source)) {
    throw new Error(`runtime-env source is missing: ${path.relative(APP_ROOT, source)}`);
  }
  // Containment: the profile id is pattern-checked above and the resolved
  // source must stay inside config/app.
  if (!path.resolve(source).startsWith(path.resolve(APP_ROOT, 'config', 'app') + path.sep)) {
    throw new Error(`runtime-env source escapes config/app: ${source}`);
  }
  const parsed = JSON.parse(readFileSync(source, 'utf8'));
  if (parsed.profileId !== profileId) {
    throw new Error(
      `runtime-env profileId mismatch: file declares ${parsed.profileId}, requested ${profileId}`,
    );
  }
  mkdirSync(path.dirname(RAWFILE_TARGET), { recursive: true });
  copyFileSync(source, RAWFILE_TARGET);
  console.log(
    `[project-runtime] projected ${path.relative(APP_ROOT, source)} -> ${path.relative(APP_ROOT, RAWFILE_TARGET)}`,
  );
}

main();
