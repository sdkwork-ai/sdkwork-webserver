#!/usr/bin/env node
/**
 * Web server materialization drift check (SDKWORK_WEBSERVER_SPEC.md §2.2 / §2.4).
 *
 * `deployments/webserver/` is a MATERIALIZED tree, not hand-authored source:
 *   1. `align-webserver-workspace.mjs` derives the layout-v3 TOMLs (+ snippets,
 *      + README, + app-roots example) from `specs/topology.spec.json`;
 *   2. `render-nginx-sidecars.mjs` renders `nginx.<profile>.<environment>.conf`
 *      from the effective merge and validates it against that layout.
 *
 * Commit 75c68448 hand-edited the generated TOMLs and never re-ran the chain.
 * That silently dropped every non-production virtual host (W5 + W26) and left
 * all eight non-production sidecars stale (W16) — three gate failures whose
 * only correct fix is to re-run generation, not to patch the output.
 *
 * This gate re-runs the chain into a throw-away sandbox and compares the
 * result byte-for-byte against what is committed, so that class of drift
 * cannot land again. It also fails when the chain cannot run at all, and when
 * the sandbox renderer reports validation problems.
 *
 * Human-owned files under `deployments/webserver/static/` are excluded: those
 * are packaged fallback assets, not generation output.
 *
 * Usage:
 *   node tools/check-webserver-materialization.mjs [--root <moduleRoot>] [--json]
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const MODULE_NAME = 'sdkwork-webserver';
const WEBSERVER_REL = path.join('deployments', 'webserver');
const HUMAN_OWNED_PREFIX = path.join('static') + path.sep;
const SPECS_TOOLS = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'sdkwork-specs',
  'tools',
  'webserver',
);

const argv = process.argv.slice(2);
const jsonMode = argv.includes('--json');
const rootIndex = argv.indexOf('--root');
const root = path.resolve(rootIndex >= 0 ? argv[rootIndex + 1] : process.cwd());

const failures = [];
const warnings = [];

function fail(message) {
  failures.push(message);
}

/** Recursively list files relative to `base`, excluding human-owned assets. */
function listArtifacts(base) {
  const out = [];
  const walk = (dir, rel) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const next = rel ? path.join(rel, entry.name) : entry.name;
      if (entry.isDirectory()) {
        if (`${next}${path.sep}` === HUMAN_OWNED_PREFIX) continue;
        walk(path.join(dir, entry.name), next);
      } else {
        out.push(next);
      }
    }
  };
  if (fs.existsSync(base)) walk(base, '');
  return out.sort();
}

const normalize = (text) => text.replace(/\r\n/g, '\n').trimEnd();

function firstDifference(expected, actual) {
  const e = normalize(expected).split('\n');
  const a = normalize(actual).split('\n');
  for (let i = 0; i < Math.max(e.length, a.length); i += 1) {
    if (e[i] === a[i]) continue;
    if (i >= a.length) {
      return `committed is truncated: missing ${e.length - a.length} trailing line(s), first missing ${JSON.stringify(e[i])}`;
    }
    if (i >= e.length) {
      return `committed has ${a.length - e.length} extra trailing line(s), first extra ${JSON.stringify(a[i])}`;
    }
    return `line ${i + 1}: committed=${JSON.stringify(a[i])} regenerated=${JSON.stringify(e[i])}`;
  }
  return 'no line difference (whitespace-only?)';
}

async function loadTools() {
  const build = await import(pathToFileURL(path.join(SPECS_TOOLS, 'build-from-topology.mjs')).href);
  const render = await import(pathToFileURL(path.join(SPECS_TOOLS, 'render-nginx-sidecars.mjs')).href);
  return { build, render };
}

async function main() {
  const topologyPath = path.join(root, 'specs', 'topology.spec.json');
  if (!fs.existsSync(topologyPath)) {
    fail(`missing ${topologyPath} — cannot materialize without a topology source`);
    return;
  }
  const committedDir = path.join(root, WEBSERVER_REL);
  if (!fs.existsSync(committedDir)) {
    fail(`missing ${committedDir}`);
    return;
  }

  const { build, render } = await loadTools();
  // The sandbox directory must carry the module name: buildAppRootsExample()
  // and buildWebserverReadme() embed path.basename(moduleRoot) in their output.
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'ws-mat-check-'));
  const sandbox = path.join(scratch, MODULE_NAME);

  try {
    fs.mkdirSync(path.join(sandbox, 'specs'), { recursive: true });
    fs.copyFileSync(topologyPath, path.join(sandbox, 'specs', 'topology.spec.json'));
    // Empty app roots are enough: only the directory names feed app-roots.example.toml.
    for (const appDir of fs.readdirSync(path.join(root, 'apps'), { withFileTypes: true })) {
      if (appDir.isDirectory() && /-pc$|-h5$/u.test(appDir.name)) {
        fs.mkdirSync(path.join(sandbox, 'apps', appDir.name), { recursive: true });
      }
    }

    const topology = JSON.parse(fs.readFileSync(topologyPath, 'utf8'));
    const docs = build.buildWebserverDocs({ appId: MODULE_NAME, topology, moduleRoot: sandbox });
    if (!docs.enabled) {
      fail('topology-derived docs report the module as disabled — refusing to compare');
      return;
    }
    build.writeWebserverLayout(sandbox, docs, { appId: MODULE_NAME, topology });

    const rendered = render.renderModuleNginxSidecars(sandbox, { validate: true, quiet: true });
    if (rendered.skipped) {
      fail(`sandbox sidecar render skipped: ${rendered.reason}`);
      return;
    }
    for (const warning of rendered.warnings ?? []) warnings.push(`generation warning: ${warning}`);

    const generated = listArtifacts(path.join(sandbox, WEBSERVER_REL));
    const committed = listArtifacts(committedDir);
    if (generated.length === 0) {
      fail('generation produced zero artifacts — treating as failure, not as a clean tree');
      return;
    }

    const generatedSet = new Set(generated);
    const committedSet = new Set(committed);
    for (const rel of generated) {
      if (!committedSet.has(rel)) {
        fail(`${rel}: generated by the chain but NOT committed`);
        continue;
      }
      const expected = fs.readFileSync(path.join(sandbox, WEBSERVER_REL, rel), 'utf8');
      const actual = fs.readFileSync(path.join(committedDir, rel), 'utf8');
      if (normalize(expected) !== normalize(actual)) {
        fail(`${rel}: diverges from specs/topology.spec.json — ${firstDifference(expected, actual)}`);
      }
    }
    for (const rel of committed) {
      if (!generatedSet.has(rel)) fail(`${rel}: committed but NOT produced by the chain (stray file)`);
    }

    if (!jsonMode) {
      const checked = generated.length;
      console.log(`webserver materialization: ${checked} artifact(s) regenerated and compared`);
    }
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

await main();

if (jsonMode) {
  process.stdout.write(`${JSON.stringify({ root, failures, warnings }, null, 2)}\n`);
} else {
  for (const warning of warnings) console.log(`  ${warning}`);
  for (const failure of failures) console.error(`error: ${failure}`);
}

if (failures.length > 0) {
  if (!jsonMode) {
    console.error(
      '\nThe committed tree drifted from specs/topology.spec.json.\n'
        + 'Never hand-edit deployments/webserver — regenerate it instead:\n'
        + '  pnpm run align:webserver\n'
        + 'If generation itself is wrong, fix specs/tools/webserver (shared), not the output.',
    );
  }
  process.exit(1);
}
if (!jsonMode) console.log('webserver materialization check passed');
