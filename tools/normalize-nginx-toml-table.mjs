#!/usr/bin/env node
/**
 * Normalize [nginx] tables under sdkwork-space deployments/webserver.
 * Canonical key order: enabled, profile, unknownDirectivePolicy, strict,
 * confFile, exceptionRef. Drops duplicate keys (last value wins only when
 * equal; unequal duplicates abort).
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const CANONICAL = [
  'enabled',
  'profile',
  'unknownDirectivePolicy',
  'strict',
  'confFile',
  'exceptionRef',
];

function listCommonToml() {
  const result = spawnSync(
    'rg',
    [
      '-l',
      'kind = .sdkwork.webserver.server.',
      workspaceRoot,
      '--glob',
      '**/deployments/webserver/server.common.toml',
      '--glob',
      '!**/node_modules/**',
    ],
    { encoding: 'utf8' },
  );
  return (result.stdout || '')
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean);
}

function normalizeNginxBlock(text) {
  const lines = text.split(/\r?\n/u);
  const start = lines.findIndex((line) => line.trim() === '[nginx]');
  if (start < 0) {
    return { text, changed: false, reason: 'no-nginx-table' };
  }

  let end = start + 1;
  while (end < lines.length && !/^\s*\[/.test(lines[end])) {
    end += 1;
  }

  const bodyLines = lines.slice(start + 1, end);
  const values = new Map();
  const comments = [];
  const other = [];

  for (const line of bodyLines) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    if (trimmed.startsWith('#')) {
      comments.push(line);
      continue;
    }
    const match = line.match(/^\s*([A-Za-z][A-Za-z0-9_]*)\s*=\s*(.*)$/u);
    if (!match) {
      other.push(line);
      continue;
    }
    const key = match[1];
    const value = match[2].trim();
    if (values.has(key) && values.get(key) !== value) {
      throw new Error(`conflicting duplicate key ${key}: ${values.get(key)} vs ${value}`);
    }
    values.set(key, value);
  }

  if (other.length) {
    throw new Error(`unsupported [nginx] lines: ${other.join(' | ')}`);
  }

  const rebuilt = ['[nginx]', ...comments];
  for (const key of CANONICAL) {
    if (values.has(key)) {
      rebuilt.push(`${key} = ${values.get(key)}`);
      values.delete(key);
    }
  }
  for (const [key, value] of values) {
    rebuilt.push(`${key} = ${value}`);
  }
  rebuilt.push('');

  const eol = text.includes('\r\n') ? '\r\n' : '\n';
  const next = [...lines.slice(0, start), ...rebuilt, ...lines.slice(end)].join(eol);
  const normalized = next.replace(/(?:\r?\n)+$/u, eol);
  return { text: normalized, changed: normalized !== text };
}

const files = listCommonToml();
let changed = 0;
for (const file of files) {
  const raw = fs.readFileSync(file, 'utf8');
  const result = normalizeNginxBlock(raw);
  if (result.changed) {
    fs.writeFileSync(file, result.text, 'utf8');
    changed += 1;
    console.log(`normalized ${file}`);
  }
}

console.log(JSON.stringify({ files: files.length, changed }, null, 2));
