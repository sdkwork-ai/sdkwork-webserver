#!/usr/bin/env node

/**
 * Switch the active webserver import set between standalone and cloud.
 * Authority: SDKWORK_WEBSERVER_SPEC.md §17.3 (imports.d dual configuration).
 *
 * The webserver runtime config loads imports.d/import.conf and
 * imports.d/layout-imports.toml through `[webserver] include`. Both import
 * sets (import.conf.standalone / import.conf.cloud and their layout TOML
 * siblings) are materialized by the entrypoint; this script atomically
 * re-copies the selected set onto the active files so the startup mode can
 * switch freely without rebuilding module configs.
 *
 * Usage:
 *   node scripts/webserver-import-profile.mjs <standalone|cloud>
 *   node scripts/webserver-import-profile.mjs status
 */

import { copyFileSync, existsSync, readFileSync, rmSync, statSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const CONFIG_ROOT = process.env.SDKWORK_WEBSERVER_CONFIG_ROOT || '/etc/sdkwork/webserver';
const IMPORTS_ROOT = path.join(CONFIG_ROOT, 'imports.d');
const PROFILES = Object.freeze(['standalone', 'cloud']);
const ACTIVE_IMPORT_CONF = 'import.conf';
const ACTIVE_LAYOUT_TOML = 'layout-imports.toml';

function readActiveProfile() {
  for (const profile of PROFILES) {
    const importConf = path.join(IMPORTS_ROOT, `import.conf.${profile}`);
    const active = path.join(IMPORTS_ROOT, ACTIVE_IMPORT_CONF);
    if (existsSync(importConf) && existsSync(active) && statSync(active).isFile()) {
      let activeContent;
      try {
        activeContent = readFileSync(active, 'utf8');
      } catch {
        continue;
      }
      let profileContent;
      try {
        profileContent = readFileSync(importConf, 'utf8');
      } catch {
        continue;
      }
      if (activeContent === profileContent) {
        return profile;
      }
    }
  }
  return null;
}

function activate(profile) {
  if (!PROFILES.includes(profile)) {
    throw new Error(`import profile must be ${PROFILES.join(' or ')}`);
  }
  if (!existsSync(path.join(IMPORTS_ROOT, `import.conf.${profile}`))) {
    throw new Error(
      `import set ${profile} is not materialized under ${IMPORTS_ROOT}; run the entrypoint first`,
    );
  }
  const importConf = path.join(IMPORTS_ROOT, `import.conf.${profile}`);
  const layoutToml = path.join(IMPORTS_ROOT, `layout-imports.${profile}.toml`);
  const activeImportConf = path.join(IMPORTS_ROOT, ACTIVE_IMPORT_CONF);
  const activeLayoutToml = path.join(IMPORTS_ROOT, ACTIVE_LAYOUT_TOML);
  copyFileSync(importConf, activeImportConf);
  if (existsSync(layoutToml)) {
    copyFileSync(layoutToml, activeLayoutToml);
  } else {
    rmSync(activeLayoutToml, { force: true });
  }
  console.log(`[webserver-import-profile] activated import profile ${profile} under ${IMPORTS_ROOT}`);
  return profile;
}

function main() {
  const operation = process.argv[2];
  if (operation === 'status' || operation === undefined) {
    const active = readActiveProfile();
    if (active) {
      console.log(`[webserver-import-profile] active import profile: ${active}`);
      return;
    }
    console.log('[webserver-import-profile] no active import set detected; default activation is cloud');
    return;
  }
  activate(operation);
}

main();
