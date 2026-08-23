#!/usr/bin/env node
/**
 * Render deployments/webserver/nginx.<profile>.<environment>.conf sidecars from
 * layout v3 merge (SDKWORK_WEBSERVER_SPEC.md §4.3 / §13.2).
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  DEPLOYMENT_PROFILES,
  LIFECYCLE_ENVIRONMENTS,
  ENVIRONMENT_FILE_NAMES,
  mergeEffective,
  sidecarFileName,
} from '../../sdkwork-specs/tools/webserver/layout-v3.mjs';
import {
  renderNginxConf,
  validateWebserverDir,
} from '../../sdkwork-specs/tools/webserver/validate.mjs';
import { parseTomlSubset } from '../../sdkwork-specs/tools/webserver/toml.mjs';
import { applyAdaptiveWebFolding } from '../../sdkwork-specs/tools/webserver/adaptive-web.mjs';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const webserverDir = path.join(appRoot, 'deployments', 'webserver');
const confBase = 'nginx.conf';

for (const entry of fs.readdirSync(webserverDir)) {
  if (/^nginx\.(standalone|cloud)(\..*)?\.conf$/u.test(entry)) {
    fs.unlinkSync(path.join(webserverDir, entry));
  }
}

const common = parseTomlSubset(
  fs.readFileSync(path.join(webserverDir, 'server.common.toml'), 'utf8'),
  'server.common.toml',
);
const environmentDocs = Object.fromEntries(
  LIFECYCLE_ENVIRONMENTS.map((environment) => [
    environment,
    parseTomlSubset(
      fs.readFileSync(path.join(webserverDir, ENVIRONMENT_FILE_NAMES[environment]), 'utf8'),
      ENVIRONMENT_FILE_NAMES[environment],
    ),
  ]),
);
const profileDocs = Object.fromEntries(
  DEPLOYMENT_PROFILES.map((profile) => [
    profile,
    parseTomlSubset(
      fs.readFileSync(path.join(webserverDir, `server.${profile}.toml`), 'utf8'),
      `server.${profile}.toml`,
    ),
  ]),
);

for (const profile of DEPLOYMENT_PROFILES) {
  for (const environment of LIFECYCLE_ENVIRONMENTS) {
    const merged = mergeEffective(common, environmentDocs[environment], profileDocs[profile]);
    const { doc, mode, warnings } = applyAdaptiveWebFolding(merged, {
      moduleRoot: appRoot,
      runtimeCode: 'webserver',
    });
    for (const warning of warnings) {
      console.warn(`warning [${profile}.${environment}]: ${warning}`);
    }
    const conf = renderNginxConf(doc, { profile, environment });
    const out = path.join(webserverDir, sidecarFileName(confBase, profile, environment));
    fs.writeFileSync(out, `${conf.trimEnd()}\n`, 'utf8');
    console.log(`wrote ${path.relative(appRoot, out)} (adaptive mode: ${mode})`);
  }
}

const verify = validateWebserverDir(appRoot);
for (const warning of verify.warnings) console.warn(`warning: ${warning}`);
if (!verify.ok) {
  for (const error of verify.errors) console.error(`error: ${error}`);
  process.exit(1);
}
console.log('check-webserver-toml-standard ok (sidecars refreshed)');
