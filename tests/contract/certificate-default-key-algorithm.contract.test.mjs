#!/usr/bin/env node
// The managed-certificate key algorithm default is one decision, and this gate
// exists because it is spelled in three places that nothing used to compare.
//
// Why it matters here specifically: this default is *repeated silently*. An
// operator fills in the certificate form without touching the control, and then
// every unattended renewal inherits that choice for the life of the certificate.
// Whoever changes it is deciding what every deployment that never expressed an
// opinion ends up serving — and the failure mode is an old client that cannot
// complete a handshake months later, long after the change is forgotten.
//
// Three things are asserted, each for a different way the decision can be lost:
//   1. the authored contract says RSA, so the published API documents it;
//   2. every derived artifact agrees with the source, because a one-sided edit
//      of a generated copy is invisible until the next regeneration undoes it;
//   3. the Rust wire default reads the shared vocabulary instead of restating a
//      literal, so this repository cannot drift from the deployment control
//      plane and the ACME engine, which key off the same constant.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { parse } from 'yaml';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const SCHEMA = 'IssueCertificateRequest';
const FIELD = 'keyAlgorithm';
const EXPECTED_DEFAULT = 'RSA';

const source = 'apis/backend-api/web/openapi.yaml';
// The authority the gateway serves, plus the two copies the SDK generator reads.
const artifacts = [
  'apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json',
  'sdks/sdkwork-webserver-backend-sdk/openapi/sdkwork-webserver-backend-api.openapi.json',
  'sdks/sdkwork-webserver-backend-sdk/openapi/sdkwork-webserver-backend-api.sdkgen.yaml',
];
const rustContract = 'crates/sdkwork-webserver-contract/src/dto.rs';

function read(relative) {
  return fs.readFileSync(path.join(root, relative), 'utf8');
}

function fieldOf(doc, schema, field) {
  const properties = doc?.components?.schemas?.[schema]?.properties;
  return properties ? properties[field] : undefined;
}

test('the authored certificate contract names RSA as the default key algorithm', () => {
  const declared = fieldOf(parse(read(source)), SCHEMA, FIELD);
  assert.ok(declared, `${SCHEMA}.${FIELD} is missing from ${source}`);
  assert.equal(
    declared.default,
    EXPECTED_DEFAULT,
    `${SCHEMA}.${FIELD} must default to ${EXPECTED_DEFAULT} in the source contract`,
  );
  // The vocabulary is unchanged: switching the default may not quietly retire
  // the other algorithm, which operators still select explicitly per certificate.
  assert.deepEqual([...declared.enum].sort(), ['ECDSA', EXPECTED_DEFAULT]);
});

test('every derived contract artifact carries the same default', () => {
  const declared = fieldOf(parse(read(source)), SCHEMA, FIELD);
  for (const relative of artifacts) {
    const derived = fieldOf(JSON.parse(read(relative)), SCHEMA, FIELD);
    assert.ok(derived, `${SCHEMA}.${FIELD} is missing from ${relative}`);
    assert.deepEqual(derived, declared, `${relative} does not match ${source}`);
  }
});

test('the rust wire default reads the shared vocabulary instead of a literal', () => {
  const dto = read(rustContract);
  const body = dto.slice(dto.indexOf(`fn default_${FIELD.replace('keyAlgorithm', 'certificate_key_algorithm')}()`));
  assert.ok(body.length > 0, 'default_certificate_key_algorithm() is missing');
  assert.match(
    body.slice(0, body.indexOf('}')),
    /sdkwork_deploy_core::CERTIFICATE_DEFAULT_KEY_ALGORITHM/,
    'the certificate default must come from sdkwork_deploy_core so this repo, the ' +
      'deployment control plane and the ACME engine cannot disagree',
  );
});
