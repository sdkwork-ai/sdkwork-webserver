#!/usr/bin/env node
/**
 * One-time bootstrap for sdkwork-webserver standards-aligned scaffold.
 * Run: node tools/bootstrap_scaffold.mjs
 */
import { mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function write(relativePath, content) {
  const full = join(root, relativePath);
  mkdirSync(dirname(full), { recursive: true });
  if (existsSync(full)) {
    return;
  }
  writeFileSync(full, content, 'utf8');
  process.stdout.write(`created ${relativePath}\n`);
}

// Root manifests are created by the agent directly; this script creates placeholders only.
const placeholders = [
  'apis/README.md',
  'apps/README.md',
  'crates/README.md',
  'sdks/README.md',
  'jobs/README.md',
  'tools/README.md',
  'plugins/README.md',
  'examples/README.md',
  'scripts/README.md',
  'docs/README.md',
  'docs/architecture/TECH_ARCHITECTURE.md',
  'docs/product/PRD.md',
  'deployments/README.md',
  'deployments/docker/README.md',
  'database/migrations/postgres/README.md',
  'database/seeds/common/README.md',
  'database/seeds/locales/README.md',
  'database/seeds/locales/en-US/README.md',
  'database/seeds/locales/zh-CN/README.md',
  'database/ddl/generated/README.md',
  'database/fixtures/README.md',
  '.sdkwork/README.md',
  '.sdkwork/skills/README.md',
  '.sdkwork/plugins/README.md',
];

for (const path of placeholders) {
  write(path, `# ${path.split('/').pop()?.replace('.md', '')}\n\nReserved per SDKWORK_WORKSPACE_SPEC.md.\n`);
}

process.stdout.write('bootstrap_scaffold.mjs done\n');
