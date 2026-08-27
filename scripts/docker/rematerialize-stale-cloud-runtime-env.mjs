#!/usr/bin/env node
/**
 * Rematerialize stale public/runtime-env.json for cloud profiles when the
 * checked-in document is incomplete relative to etc/browser sources.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const SPECS_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..', 'sdkwork-specs');
const { materializeBrowserRuntimeEnv } = await import(
  pathToFileURL(path.join(SPECS_ROOT, 'tools', 'build-browser-client.mjs')).href
);

const workspaceRoot = path.resolve(process.argv[2] ?? path.join(SPECS_ROOT, '..'));
let fixed = 0;
let skipped = 0;
const errors = [];

for (const name of fs.readdirSync(workspaceRoot)) {
  if (!name.startsWith('sdkwork-')) continue;
  const repositoryRoot = path.join(workspaceRoot, name);
  const apps = path.join(repositoryRoot, 'apps');
  if (!fs.existsSync(apps)) continue;
  for (const app of fs.readdirSync(apps)) {
    const appRoot = path.join(apps, app);
    const pub = path.join(appRoot, 'public', 'runtime-env.json');
    const deployment = path.join(appRoot, 'etc', 'sdkwork.deployment.config.json');
    if (!fs.existsSync(pub) || !fs.existsSync(deployment)) continue;
    let doc;
    try {
      doc = JSON.parse(fs.readFileSync(pub, 'utf8'));
    } catch {
      continue;
    }
    if (doc.deploymentProfile !== 'cloud') {
      skipped += 1;
      continue;
    }
    const base = String(doc.appApiBaseUrl ?? '');
    const urls = Array.isArray(doc.cloudApiBaseUrls) ? doc.cloudApiBaseUrls : [];
    const incomplete = !base.includes(';') || urls.length < 5;
    if (!incomplete) {
      skipped += 1;
      continue;
    }
    try {
      materializeBrowserRuntimeEnv({
        appRoot,
        deploymentProfile: 'cloud',
        environment: doc.environment || 'production',
        repositoryRoot,
      });
      fixed += 1;
      console.log(`fixed ${name}/${app}`);
    } catch (error) {
      errors.push(`${name}/${app}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

console.log(JSON.stringify({ fixed, skipped, errorCount: errors.length, errors: errors.slice(0, 20) }, null, 2));
if (errors.length > 0) process.exitCode = 1;
