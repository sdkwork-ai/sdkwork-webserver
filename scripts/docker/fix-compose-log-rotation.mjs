#!/usr/bin/env node
/**
 * fix-compose-log-rotation.mjs — one-shot alignment tool for OPERATIONS_SPEC §2.3.
 *
 * For every docker-compose*.yml under a root: ensure every top-level service
 * under `services:` carries a `logging:` block. When the file already defines
 * the `x-default-logging: &default-logging` anchor, services get
 * `logging: *default-logging`; otherwise a full inline block is inserted.
 *
 * Usage: node fix-compose-log-rotation.mjs <root> [--dry-run]
 */
import { readdirSync, readFileSync, writeFileSync, statSync } from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2);
const dryRun = args.includes('--dry-run');
const rootArg = args.find((a) => !a.startsWith('--'));
if (!rootArg) { console.error('usage: node fix-compose-log-rotation.mjs <root> [--dry-run]'); process.exit(2); }
const root = path.resolve(rootArg);

const files = [];
(function walk(dir, depth) {
  if (depth > 3) return;
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules') continue;
    const p = path.join(dir, name);
    const s = statSync(p, { throwIfNoEntry: false });
    if (!s) continue;
    if (s.isDirectory()) walk(p, depth + 1);
    else if (/^docker-compose.*\.ya?ml$/.test(name)) files.push(p);
  }
})(root, 0);

const ANCHOR = [
  '# Bounded json-file log rotation: unbounded container logs eventually fill',
  '# the host disk on long-lived deployments (OPERATIONS_SPEC.md §2.3).',
  'x-default-logging: &default-logging',
  '  driver: json-file',
  '  options:',
  '    max-size: "50m"',
  '    max-file: "3"',
  '',
].join('\n');
const INLINE_BLOCK = [
  '    logging:',
  '      driver: json-file',
  '      options:',
  '        max-size: "50m"',
  '        max-file: "3"',
].join('\n');

let changed = 0;
for (const file of files) {
  const lines = readFileSync(file, 'utf8').split('\n');
  if (!lines.some((l) => /^services:\s*$/.test(l))) continue;
  const hasAnchor = lines.some((l) => /^x-default-logging:\s*&default-logging\s*$/.test(l));

  // Collect top-level service blocks: name at 2-space indent under services:,
  // block ends at the next 2-space-indent key or a 0/1-space-indent key.
  const services = [];
  let inServices = false;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (/^services:\s*$/.test(line)) { inServices = true; continue; }
    if (inServices && /^(services|volumes|networks|secrets|configs):/.test(line)) { inServices = false; continue; }
    if (!inServices) continue;
    const m = line.match(/^  ([a-zA-Z0-9_-]+):\s*$/);
    if (m) services.push({ name: m[1], start: i });
  }
  // Block end = line index of the next service start or the first line after
  // services: that is not indented by >=2 spaces.
  for (let s = 0; s < services.length; s += 1) {
    const start = services[s].start;
    let end = s + 1 < services.length ? services[s + 1].start : lines.length;
    for (let i = start + 1; i < end; i += 1) {
      if (lines[i].length > 0 && !/^ {2,}/.test(lines[i]) && !/^\t/.test(lines[i])) { end = i; break; }
    }
    services[s].end = end;
    services[s].hasLogging = false;
    for (let i = start; i < end; i += 1) {
      if (/^ {4}logging:\s*(\*default-logging)?$/.test(lines[i])) { services[s].hasLogging = true; break; }
    }
  }
  const missing = services.filter((s) => !s.hasLogging);
  if (missing.length === 0) continue;

  const rel = path.relative(root, file);
  if (dryRun) {
    console.log(`${rel}: would add logging to ${missing.map((s) => s.name).join(', ')}`);
    continue;
  }
  // Insert bottom-up so earlier indices stay valid.
  for (let i = missing.length - 1; i >= 0; i -= 1) {
    const svc = missing[i];
    if (hasAnchor) {
      lines.splice(svc.end, 0, `    logging: *default-logging`);
    } else {
      lines.splice(svc.end, 0, INLINE_BLOCK);
    }
  }
  let text = lines.join('\n');
  if (!hasAnchor) {
    // Add the shared anchor before the first top-level key (or at top).
    const firstKey = text.match(/^(name:|services:|volumes:|networks:)/m);
    if (firstKey) text = text.replace(/^(name:|services:)/m, `${ANCHOR}$1`);
    else text = `${ANCHOR}${text}`;
  }
  writeFileSync(file, text);
  changed += 1;
  console.log(`${rel}: added logging to ${missing.map((s) => s.name).join(', ')}`);
}
console.log(changed === 0 ? 'all compose files conform' : `patched ${changed} file(s)`);
