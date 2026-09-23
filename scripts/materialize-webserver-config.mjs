#!/usr/bin/env node
/**
 * Materialize deployments/webserver from specs/topology.spec.json (layout v3).
 *
 * Two ordered steps, both owned by sdkwork-specs:
 *   1. align-webserver-workspace  -> layout-v3 TOMLs, snippets, README, app-roots example
 *   2. render-webserver-nginx-sidecars -> nginx.<profile>.<environment>.conf + validation
 *
 * They must run as one unit. Step 1 validates the sidecars it did not write, so
 * it exits non-zero whenever the sidecars are stale — chaining the two with `&&`
 * in a shell therefore stops before step 2 can repair them. This script runs
 * both steps unconditionally and reports the post-render validation as the
 * authority.
 *
 * `deployments/webserver` is generated output: never hand-edit it. If the
 * generated result is wrong, fix specs/tools/webserver instead.
 */
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const specsTools = path.resolve(appRoot, '..', 'sdkwork-specs', 'tools', 'webserver');
const moduleName = path.basename(appRoot);

const { buildWebserverDocs, writeWebserverLayout } = await import(
  pathToFileURL(path.join(specsTools, 'build-from-topology.mjs')).href
);
const { renderModuleNginxSidecars } = await import(
  pathToFileURL(path.join(specsTools, 'render-nginx-sidecars.mjs')).href
);
const { validateWebserverDir } = await import(
  pathToFileURL(path.join(specsTools, 'validate.mjs')).href
);

const topologyPath = path.join(appRoot, 'specs', 'topology.spec.json');
const fs = await import('node:fs');
if (!fs.existsSync(topologyPath)) {
  console.error(`align:webserver — missing ${topologyPath}`);
  process.exit(1);
}
const topology = JSON.parse(fs.readFileSync(topologyPath, 'utf8'));

console.log(`align:webserver — ${moduleName}`);
const docs = buildWebserverDocs({ appId: moduleName, topology, moduleRoot: appRoot });
if (!docs.enabled) {
  console.error('align:webserver — topology-derived docs report the module as disabled');
  process.exit(1);
}
writeWebserverLayout(appRoot, docs, { appId: moduleName, topology });
console.log(
  `  step 1/2 align-webserver-workspace: layout-v3 TOMLs, snippets, README, app-roots example`,
);

const rendered = renderModuleNginxSidecars(appRoot, { validate: false, quiet: true });
if (rendered.skipped) {
  console.error(`align:webserver — sidecar render skipped: ${rendered.reason}`);
  process.exit(1);
}
console.log(`  step 2/2 render-nginx-sidecars: wrote ${rendered.written.length} sidecar(s)`);

const validation = validateWebserverDir(appRoot);
for (const warning of validation.warnings ?? []) console.warn(`  warning: ${warning}`);
if (!validation.ok) {
  for (const error of validation.errors ?? []) console.error(`  error: ${error}`);
  console.error('align:webserver — validation failed after materialization');
  process.exit(1);
}
console.log('align:webserver — ok (validation clean)');
