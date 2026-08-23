#!/usr/bin/env node
/**
 * Verify the nginx sidecar renderer (nginx.sidecar-render): running the
 * script must regenerate deployments/webserver/nginx.<profile>.conf and the
 * rendered sidecars must stay byte-identical to the committed ones
 * (deterministic rendering, W16-equivalent).
 */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const renderer = path.join(root, 'scripts', 'render-webserver-nginx-sidecars.mjs');
const webserverDir = path.join(root, 'deployments', 'webserver');
const sidecars = ['nginx.standalone.conf', 'nginx.cloud.conf'];

test('sidecar renderer runs and stays deterministic', () => {
  const before = new Map(
    sidecars.map((name) => [name, fs.readFileSync(path.join(webserverDir, name), 'utf8')]),
  );

  execFileSync(process.execPath, [renderer], { cwd: root, stdio: 'pipe' });

  for (const name of sidecars) {
    const rendered = fs.readFileSync(path.join(webserverDir, name), 'utf8');
    assert.ok(rendered.length > 0, `${name} must be rendered`);
    assert.equal(
      rendered,
      before.get(name),
      `${name} must render byte-identically to the committed sidecar`,
    );
  }
});

test('rendered sidecars carry the declared surface', () => {
  const standalone = fs.readFileSync(path.join(webserverDir, 'nginx.standalone.conf'), 'utf8');
  const cloud = fs.readFileSync(path.join(webserverDir, 'nginx.cloud.conf'), 'utf8');
  for (const [name, content] of [
    ['nginx.standalone.conf', standalone],
    ['nginx.cloud.conf', cloud],
  ]) {
    assert.ok(content.includes('server_name server.sdkwork.com;'), `${name} server_name`);
    assert.ok(content.includes('upstream gateway'), `${name} upstream`);
    assert.ok(content.includes('listen 443 ssl'), `${name} https listener`);
    assert.ok(content.includes('proxy_pass'), `${name} proxy_pass`);
  }
});
