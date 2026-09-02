#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { parse } from 'yaml';

const workspaceRoot = process.cwd();
const contracts = [
  {
    label: 'authored backend OpenAPI',
    path: 'apis/backend-api/web/openapi.yaml',
    parse: (text) => parse(text),
  },
  {
    label: 'materialized backend API authority',
    path: 'apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json',
    parse: JSON.parse,
  },
  {
    label: 'materialized backend SDK authority',
    path: 'sdks/sdkwork-webserver-backend-sdk/openapi/sdkwork-webserver-backend-api.openapi.json',
    parse: JSON.parse,
  },
];

const portableMetadata = /^[\x09\x0A\x0D\x20-\x7E]*$/;

function validateMetadata(node, location, failures) {
  if (Array.isArray(node)) {
    node.forEach((item, index) => validateMetadata(item, `${location}[${index}]`, failures));
    return;
  }
  if (node === null || typeof node !== 'object') {
    return;
  }
  for (const [key, value] of Object.entries(node)) {
    const childLocation = `${location}.${key}`;
    if ((key === 'summary' || key === 'description') && typeof value === 'string') {
      if (!portableMetadata.test(value)) {
        failures.push(childLocation);
      }
    }
    validateMetadata(value, childLocation, failures);
  }
}

for (const contract of contracts) {
  const absolutePath = path.join(workspaceRoot, contract.path);
  const text = fs.readFileSync(absolutePath, 'utf8');
  const document = contract.parse(text);
  const failures = [];
  validateMetadata(document, '$', failures);
  assert.deepEqual(
    failures,
    [],
    `${contract.label} contains non-portable summary/description text at: ${failures.join(', ')}`,
  );

  for (const routePath of [
    '/backend/v3/api/agent/heartbeat',
    '/backend/v3/api/agent/sync',
  ]) {
    const pathItem = document.paths?.[routePath];
    const operation = pathItem?.post ?? pathItem?.get;
    assert.ok(operation, `${contract.label} is missing ${routePath}`);
    assert.deepEqual(
      operation.security,
      [{ AgentToken: [], AccessToken: [] }],
      `${contract.label} must expose ${routePath} with AgentToken and AccessToken protection`,
    );
    assert.equal(operation['x-sdkwork-auth-mode'], 'api-key');
    assert.equal(operation['x-sdkwork-route-auth'], 'agent-token');
  }
}

process.stdout.write('backend-openapi-text.contract.test.mjs passed\n');
