#!/usr/bin/env node
/**
 * Validate specs/nginx-gap.catalog.json shape and summarize status.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const catalogPath = path.join(root, 'specs', 'nginx-gap.catalog.json');

test('nginx gap catalog is present and well-formed', () => {
  const catalog = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));
  assert.equal(catalog.schemaVersion, 1);
  assert.equal(catalog.kind, 'sdkwork.webserver.nginx-gap.catalog');
  assert.equal(catalog.profile, 'http-core-v1');
  assert.ok(Array.isArray(catalog.capabilities));
  assert.ok(catalog.capabilities.length >= 10);
  const statuses = new Set(['implemented', 'partial', 'missing', 'excluded']);
  const ids = new Set();
  for (const item of catalog.capabilities) {
    assert.ok(item.id && !ids.has(item.id), `duplicate or missing id: ${item.id}`);
    ids.add(item.id);
    assert.ok(statuses.has(item.status), `${item.id} bad status ${item.status}`);
    assert.ok(item.summary);
    assert.ok(!item.id.startsWith('compat.'), `${item.id} must not use retired compat.* prefix`);
  }
  assert.ok(ids.has('nginx.toggle'));
  assert.ok(ids.has('nginx.conf-import'));
  assert.ok(ids.has('nginx.behavioral-corpus'));
  const corpus = catalog.capabilities.find((item) => item.id === 'nginx.behavioral-corpus');
  assert.equal(corpus.status, 'implemented');
  assert.ok(
    fs.existsSync(path.join(root, 'specs', 'nginx-behavioral-corpus.manifest.json')),
    'behavioral corpus must be indexed by specs/nginx-behavioral-corpus.manifest.json',
  );
});
