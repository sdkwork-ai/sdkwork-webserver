import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { parse as parseYaml } from 'yaml';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function filesBelow(relativeRoot, predicate = () => true) {
  const root = path.join(ROOT, relativeRoot);
  const files = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const absolute = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        if (!['generated', 'node_modules', 'target'].includes(entry.name)) {
          visit(absolute);
        }
      } else if (predicate(absolute)) {
        files.push(absolute);
      }
    }
  };
  visit(root);
  return files;
}

function relative(file) {
  return path.relative(ROOT, file).replaceAll('\\', '/');
}

test('application-owned APIs and dependencies cannot bypass SDKWork Drive', () => {
  const apiFiles = filesBelow('apis', (file) => /\.ya?ml$/u.test(file));
  const forbiddenRoutes = [];
  for (const file of apiFiles) {
    const document = parseYaml(readFileSync(file, 'utf8'));
    for (const route of Object.keys(document?.paths ?? {})) {
      if (/\/(?:uploads?|upload_sessions?|presign|multipart|file_parts?)(?:\/|\{|$)/iu.test(route)) {
        forbiddenRoutes.push(`${relative(file)}:${route}`);
      }
    }
  }
  assert.deepEqual(
    forbiddenRoutes,
    [],
    `business upload lifecycle routes must be owned by sdkwork-drive: ${forbiddenRoutes.join(', ')}`,
  );

  const manifestFiles = [
    path.join(ROOT, 'Cargo.toml'),
    path.join(ROOT, 'package.json'),
    ...filesBelow('crates', (file) => path.basename(file) === 'Cargo.toml'),
    ...filesBelow('apps', (file) => ['Cargo.toml', 'package.json'].includes(path.basename(file))),
  ];
  const directProviderPattern = /(?:aws-sdk-s3|aws_sdk_s3|rusoto_s3|@aws-sdk\/client-s3|aliyun[-_].*oss|minio)/iu;
  const providerDependencies = manifestFiles
    .filter((file) => directProviderPattern.test(readFileSync(file, 'utf8')))
    .map(relative);
  assert.deepEqual(
    providerDependencies,
    [],
    `direct storage provider dependencies are forbidden; integrate sdkwork-drive: ${providerDependencies.join(', ')}`,
  );

  const rustSources = filesBelow('crates', (file) => file.endsWith('.rs') && !relative(file).includes('/tests/'));
  const rawDriveCalls = rustSources
    .filter((file) => /\/app\/v3\/api\/drive\/(?:uploader|upload_sessions?)/u.test(readFileSync(file, 'utf8')))
    .map(relative);
  assert.deepEqual(
    rawDriveCalls,
    [],
    `trusted Rust backends must use DriveUploaderService instead of Drive App API HTTP: ${rawDriveCalls.join(', ')}`,
  );
});

test('introducing Rust RPC requires SDKWork RPC framework and discovery together', () => {
  const cargoFiles = [
    path.join(ROOT, 'Cargo.toml'),
    ...filesBelow('crates', (file) => path.basename(file) === 'Cargo.toml'),
  ];
  const cargoText = cargoFiles.map((file) => readFileSync(file, 'utf8')).join('\n');
  const hasRpcTransport = /^(?:tonic|prost|grpcio)\s*(?:=|\.)/mu.test(cargoText);

  if (!hasRpcTransport) {
    assert.doesNotMatch(cargoText, /sdkwork-rpc-discovery/u);
    return;
  }

  assert.match(
    cargoText,
    /sdkwork-rpc-(?:server|client)/u,
    'RPC transport requires sdkwork-rpc-framework server/client integration',
  );
  assert.match(
    cargoText,
    /sdkwork-rpc-discovery/u,
    'RPC transport requires sdkwork-discovery integration through sdkwork-rpc-discovery',
  );
});

test('Web Server runtime configuration does not retain cross-application IM ownership', () => {
  const activeFiles = [
    path.join(ROOT, 'package.json'),
    path.join(ROOT, 'etc', 'sdkwork.deployment.config.json'),
    ...filesBelow('scripts', (file) => /\.(?:mjs|js|ts)$/u.test(file)),
  ];
  const stale = activeFiles
    .filter((file) => /sdkwork-im|im-dev|dev-im-ingress/iu.test(readFileSync(file, 'utf8')))
    .map(relative);
  assert.deepEqual(stale, [], `stale sdkwork-im development ownership remains: ${stale.join(', ')}`);
});

// Comments are stripped before any source scan below: a quoted signature inside
// a doc comment is not an implementation, and a commented-out wiring line is not
// an injection. Both were live false negatives before this gate existed.
function withoutRustComments(source) {
  return source.replaceAll(/\/\*[\s\S]*?\*\//gu, '').replaceAll(/\/\/[^\n]*/gu, '');
}

// `crates/<crate>/src/…` -> `crates/<crate>`. Crate identity is compared by this
// directory, never by a substring of the file path: `sdkwork-webserver-service/`
// is *not* part of `crates/sdkwork-intelligence-webserver-service/src/lib.rs`, so
// a substring test silently fails to exclude the declaring crate and the gate
// passes on the definition it was supposed to police.
function crateOf(relativePath) {
  return relativePath.split('/').slice(0, 2).join('/');
}

test('every capability the service consumes optionally is implemented and injected', () => {
  // The service consumes cross-module capabilities as optional ports. An
  // optional port is only honest when a composition root fills it: a declared
  // port with no implementation reaches the operator as a permanent 503 on a
  // surface the application advertises, and no shared validator catches it —
  // `component.spec.json` port lists are checked for shape, never for `impl`.
  const servicePath = 'crates/sdkwork-intelligence-webserver-service/src/lib.rs';
  const serviceLib = withoutRustComments(readFileSync(path.join(ROOT, servicePath), 'utf8'));
  const optionalPorts = [...serviceLib.matchAll(/pub\(crate\)\s+(\w+)\s*:\s*Option<Arc<dyn\s+(\w+)>>/gu)]
    .map((match) => ({ field: match[1], port: match[2] }));

  // Positive control: this scan is the whole gate, so a rename or a formatting
  // change here must fail loudly instead of leaving the assertions below with
  // nothing to check.
  assert.deepEqual(
    optionalPorts.map((entry) => entry.port),
    ['TrafficUsageReadPort'],
    'the set of optionally-consumed capability ports changed; re-point this gate at the new set',
  );

  const rustSources = filesBelow(
    'crates',
    (file) => file.endsWith('.rs') && !relative(file).includes('/tests/'),
  );
  const sources = rustSources.map((file) => ({
    path: relative(file),
    text: withoutRustComments(readFileSync(file, 'utf8')),
  }));
  const serviceCrate = crateOf(servicePath);

  for (const { field, port } of optionalPorts) {
    // The trait's own declaration is not an implementation, and neither crate
    // that declares the contract can satisfy the two rules below.
    const traitCrate = sources
      .filter((source) => new RegExp(`pub\\s+trait\\s+${port}\\b`, 'u').test(source.text))
      .map((source) => crateOf(source.path));
    assert.ok(traitCrate.length > 0, `${port} is consumed but no crate declares the trait`);

    const implementedIn = sources
      .filter((source) => !traitCrate.includes(crateOf(source.path)))
      .filter((source) => new RegExp(`impl\\s+${port}\\s+for\\s`, 'u').test(source.text))
      .map((source) => source.path);
    assert.ok(
      implementedIn.length > 0,
      `${port} is consumed optionally but no crate implements it; the endpoint can only answer 503`,
    );

    // Injection is checked outside the service crate on purpose: the service
    // owns the builder, and the bug this gate locks is a builder nobody calls.
    // The consuming-builder ordering (attach before sharing the service behind
    // an `Arc`) needs no static check — it is a compile error, not a convention.
    const injectedIn = sources
      .filter((source) => crateOf(source.path) !== serviceCrate)
      .filter((source) => source.text.includes(`with_${field}`))
      .map((source) => source.path);
    assert.ok(
      injectedIn.length > 0,
      `no host injects ${port}; the service was built with the capability left out (${field}: None)`,
    );
  }
});
