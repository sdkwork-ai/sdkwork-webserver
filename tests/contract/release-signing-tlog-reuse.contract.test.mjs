import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SIGN_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-sign.mjs');
const MANIFEST = path.join(REPO_ROOT, 'package.json');

// Release archives are reproducible, so a re-run re-signs a byte-identical
// payload. Rekor rejects the duplicate with `409 an equivalent entry already
// exists`. The `sigstore` package's top-level `sign()` hardcodes
// `fetchOnConflict: false` (node_modules/sigstore/dist/config.js) and its
// SignOptions type does not expose the flag, so it cannot be used here. The
// signer has to assemble the Rekor witness itself.
//
// These assertions pin that decision so a future "simplify back to sign()"
// edit cannot silently reintroduce a release-blocking 409.

test('the sign script opts into Rekor conflict reuse instead of the hardcoded-off helper', async () => {
  const source = readFileSync(SIGN_SCRIPT, 'utf8');

  assert.doesNotMatch(
    source,
    /import\s*\{[^}]*\bsign\b[^}]*\}\s*from\s*'sigstore'/u,
    'the sign script must not import sign() from sigstore, which hardcodes fetchOnConflict: false',
  );
  assert.match(
    source,
    /new RekorWitness\(\{[\s\S]*?fetchOnConflict:\s*REKOR_FETCH_ON_CONFLICT/u,
    'the Rekor witness must be constructed with fetchOnConflict set explicitly',
  );
  assert.match(
    source,
    /const REKOR_FETCH_ON_CONFLICT = true;/u,
    'REKOR_FETCH_ON_CONFLICT must stay true; setting it false makes re-runs fail with a 409',
  );

  // The flag has to survive the trip into the tlog client, not merely be
  // written down next to the constructor call.
  const { RekorWitness } = await import('@sigstore/sign');
  const witness = new RekorWitness({
    rekorBaseURL: 'https://rekor.sigstore.dev',
    fetchOnConflict: true,
  });
  assert.equal(witness.tlogV1.fetchOnConflict, true);
});

test('the manifest pins the sigstore packages the signer imports directly', () => {
  const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
  // pnpm keeps node_modules strict: a transitive dependency is not importable
  // by name, so both packages the script imports must be declared here.
  assert.equal(manifest.dependencies['@sigstore/sign'], '5.0.0');
  assert.equal(manifest.dependencies['@sigstore/bundle'], '5.0.0');
  // Versions must agree with the `sigstore` package that verifies the bundle,
  // otherwise two copies of the protobuf bundle types meet in one process.
  assert.equal(manifest.dependencies.sigstore, '5.0.0');

  const source = readFileSync(SIGN_SCRIPT, 'utf8');
  for (const name of ['@sigstore/bundle', '@sigstore/sign']) {
    assert.match(source, new RegExp(`from '${name.replace('/', '\\/')}'`, 'u'), `${name} must be imported`);
  }
});
