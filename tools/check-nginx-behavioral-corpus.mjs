#!/usr/bin/env node
/**
 * Validate specs/nginx-behavioral-corpus.manifest.json against on-disk fixtures.
 * Keeps the REQ-linked nginx differential corpus indexed and free of tracked runtime junk.
 */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'specs', 'nginx-behavioral-corpus.manifest.json');
const catalogPath = path.join(root, 'specs', 'nginx-gap.catalog.json');

const FORBIDDEN_TRACKED_SUFFIXES = ['.pid', '.log'];

function gitTrackedUnder(relativeRoot) {
  try {
    const out = execFileSync(
      'git',
      ['-C', root, 'ls-files', '--', relativeRoot],
      { encoding: 'utf8' },
    );
    return out
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

test('nginx behavioral corpus manifest matches fixtures', () => {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.kind, 'sdkwork.webserver.nginx-behavioral-corpus.manifest');
  assert.equal(manifest.pinnedNginx, '1.26.2');
  assert.ok(Array.isArray(manifest.slices));
  assert.ok(manifest.slices.length >= 5);

  const corpusRoot = path.join(root, manifest.root);
  assert.ok(fs.existsSync(corpusRoot), `missing corpus root ${manifest.root}`);
  assert.ok(
    fs.existsSync(path.join(corpusRoot, '.gitignore')),
    'tests/nginx/.gitignore must exist to exclude runtime pid/log junk',
  );

  const ids = new Set();
  for (const slice of manifest.slices) {
    assert.ok(slice.id && !ids.has(slice.id), `duplicate or missing slice id: ${slice.id}`);
    ids.add(slice.id);
    assert.ok(/^REQ-2026-\d{4}$/.test(slice.req), `${slice.id} bad req id`);
    assert.ok(slice.dir);
    assert.ok(slice.nginxConf);
    assert.ok(slice.probe);

    const dir = path.join(corpusRoot, slice.dir);
    assert.ok(fs.existsSync(dir), `missing slice dir ${slice.dir}`);
    assert.ok(
      fs.existsSync(path.join(dir, slice.nginxConf)),
      `${slice.id}: missing ${slice.nginxConf}`,
    );
    assert.ok(
      fs.existsSync(path.join(dir, slice.probe)),
      `${slice.id}: missing ${slice.probe}`,
    );
  }

  const tracked = gitTrackedUnder(manifest.root);
  for (const file of tracked) {
    const base = path.basename(file);
    for (const suffix of FORBIDDEN_TRACKED_SUFFIXES) {
      assert.ok(
        !base.endsWith(suffix),
        `must not track runtime artifact ${file}`,
      );
    }
  }

  const catalog = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));
  const corpus = catalog.capabilities.find((c) => c.id === 'nginx.behavioral-corpus');
  assert.ok(corpus, 'catalog must list nginx.behavioral-corpus');
  assert.equal(
    corpus.status,
    'implemented',
    'catalog nginx.behavioral-corpus must stay implemented while the indexed corpus exists',
  );
  assert.match(
    corpus.summary,
    /manifest|tests\/nginx|REQ/i,
    'catalog summary must point operators at the indexed corpus',
  );
});
