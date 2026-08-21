#!/usr/bin/env node
/**
 * Render deployments/webserver/nginx.<profile>.conf sidecars from the layout-v2
 * TOML merge (SDKWORK_WEBSERVER_SPEC.md §4.3 / §13.2). Adaptive Web plan
 * folding applies only when a server already declares Adaptive Web wiring;
 * this module's edge nginx is reverse-proxy only (deploy.yaml mode: api).
 *
 * Stale sidecars are removed before validation so W16 cannot block regeneration.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  renderNginxConf,
  validateWebserverDir,
} from '../../sdkwork-specs/tools/webserver/validate.mjs';
import { parseTomlSubset } from '../../sdkwork-specs/tools/webserver/toml.mjs';
import { mergeConfigs } from '../../sdkwork-specs/tools/webserver/merge.mjs';
import { applyAdaptiveWebFolding } from '../../sdkwork-specs/tools/webserver/adaptive-web.mjs';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const webserverDir = path.join(appRoot, 'deployments', 'webserver');

for (const profile of ['standalone', 'cloud']) {
  const stale = path.join(webserverDir, `nginx.${profile}.conf`);
  if (fs.existsSync(stale)) fs.unlinkSync(stale);
}

const common = parseTomlSubset(
  fs.readFileSync(path.join(webserverDir, 'server.common.toml'), 'utf8'),
  'server.common.toml',
);
const profiles = {
  standalone: parseTomlSubset(
    fs.readFileSync(path.join(webserverDir, 'server.standalone.toml'), 'utf8'),
    'server.standalone.toml',
  ),
  cloud: parseTomlSubset(
    fs.readFileSync(path.join(webserverDir, 'server.cloud.toml'), 'utf8'),
    'server.cloud.toml',
  ),
};

for (const profile of ['standalone', 'cloud']) {
  const profileDoc = { ...profiles[profile] };
  delete profileDoc.profile;
  const merged = mergeConfigs(common, profileDoc);
  const { doc, mode, warnings } = applyAdaptiveWebFolding(merged, {
    moduleRoot: appRoot,
    runtimeCode: 'webserver',
  });
  for (const warning of warnings) {
    console.warn(`warning [${profile}]: ${warning}`);
  }
  const conf = renderNginxConf(doc, { profile });
  const out = path.join(webserverDir, `nginx.${profile}.conf`);
  fs.writeFileSync(out, `${conf.trimEnd()}\n`, 'utf8');
  console.log(`wrote ${path.relative(appRoot, out)} (adaptive mode: ${mode})`);
}

const verify = validateWebserverDir(appRoot);
for (const warning of verify.warnings) console.warn(`warning: ${warning}`);
if (!verify.ok) {
  for (const error of verify.errors) console.error(`error: ${error}`);
  process.exit(1);
}
console.log('check-webserver-toml-standard ok (sidecars refreshed)');
