#!/usr/bin/env node
// Field parity between the authored OpenAPI schemas and the Rust contract
// structs that actually serialize them.
//
// Why this exists: the authored `openapi.yaml` is the source the twelve
// generated SDKs are built from, while the wire shape is the `serde` field set
// of the Rust struct. Nothing compared the two. `ClusterInstanceResponse`
// drifted by sixteen fields - routing, sync, restart and probe state were served
// by the API, absent from the contract, and therefore invisible to every typed
// SDK consumer - and the console could only read them by treating the payload as
// an untyped map. The response schema also declares
// `additionalProperties: false`, so the served payload did not even validate
// against its own contract.
//
// The check runs both directions:
//   rust-only  -> a served field no client can see (the cluster defect)
//   yaml-only  -> a documented field that is never sent
// It is intentionally a text-level comparison: the point is to compare two
// independently authored artifacts, so parsing the Rust source is the signal,
// not a limitation.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { parse } from 'yaml';

const workspaceRoot = process.cwd();
const authoredContract = 'apis/backend-api/web/openapi.yaml';
const rustContractDir = 'crates/sdkwork-webserver-contract/src';

/**
 * Pairs that are known to differ, with the reason. Every entry is a recorded
 * debt: the gate fails if an entry stops drifting (delete it, do not let the
 * ledger rot) and equally if a drifted pair appears that is not listed here.
 *
 * `unclassified` means exactly that - the mismatch has been measured but not
 * yet arbitrated against the serving code, so no fix has been attempted.
 * `entity-collision` means the schema name in the contract and the struct name
 * in Rust denote different things, so field-by-field equality is not the
 * right assertion and the pair needs a rename, not a field backfill.
 */
const KNOWN_DRIFT = [
  { schema: 'ApplicationResponse', reason: 'unclassified', missing: ['siteId'] },
  { schema: 'AuditLogPage', reason: 'unclassified', missing: ['hasMore', 'nextCursor', 'page', 'pageSize'] },
  { schema: 'AuditLogResponse', reason: 'entity-collision', missing: ['resource'] },
  { schema: 'CreateServerResponse', reason: 'unclassified', missing: ['agentToken', 'server'] },
  { schema: 'ListenerCertificateBindingResponse', reason: 'entity-collision', missing: ['applicationId'] },
  { schema: 'NginxConfigPage', reason: 'unclassified', missing: ['page', 'pageSize'] },
  { schema: 'NginxConfigResponse', reason: 'entity-collision', missing: ['siteId'] },
  { schema: 'NginxReloadResponse', reason: 'entity-collision', missing: ['reloaded'] },
  { schema: 'NginxStatusResponse', reason: 'entity-collision', missing: ['activeConfigs'] },
  { schema: 'NginxValidateResponse', reason: 'entity-collision', missing: ['message'] },
  { schema: 'ServerPage', reason: 'unclassified', missing: ['hasMore', 'nextCursor'] },
];

const rustSource = fs
  .readdirSync(path.join(workspaceRoot, rustContractDir))
  .filter((entry) => entry.endsWith('.rs'))
  .map((entry) => fs.readFileSync(path.join(workspaceRoot, rustContractDir, entry), 'utf8'))
  .join('\n');

/**
 * Wire name of one Rust field: the explicit `serde(rename)` when present,
 * otherwise the conventional `snake_case` -> `camelCase` mapping that
 * `sdkwork_utils_rust` derives use.
 */
function wireName(attributes, field) {
  const renamed = /rename\s*=\s*"([^"]+)"/.exec(attributes);
  if (renamed) return renamed[1];
  return field.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
}

/** Field name -> wire name for every field of one `pub struct`. */
function structFields(name) {
  const body = new RegExp(`pub struct ${name} \\{([\\s\\S]*?)\\n\\}`).exec(rustSource);
  if (body === null) return null;
  const fields = new Map();
  const pattern = /((?:#\[[^\]]*\]\s*)*)pub (\w+):/g;
  let match = pattern.exec(body[1]);
  while (match !== null) {
    fields.set(match[2], wireName(match[1], match[2]));
    match = pattern.exec(body[1]);
  }
  return fields;
}

// Canary: a regex that silently stops matching would turn this gate green while
// covering nothing. The Rust contract is far larger than these floors.
const declaredStructs = [...rustSource.matchAll(/pub struct (\w+) \{/g)].map((match) => match[1]);
assert.ok(
  declaredStructs.length >= 60,
  `expected the Rust contract to declare at least 60 structs, saw ${declaredStructs.length}`,
);
const totalFields = declaredStructs.reduce((sum, name) => sum + (structFields(name)?.size ?? 0), 0);
assert.ok(totalFields >= 400, `expected at least 400 Rust contract fields, saw ${totalFields}`);
const instanceFieldCount = structFields('ClusterInstanceResponse')?.size ?? 0;
assert.ok(
  instanceFieldCount >= 30,
  `ClusterInstanceResponse must parse to its full field set, saw ${instanceFieldCount}`,
);

const document = parse(fs.readFileSync(path.join(workspaceRoot, authoredContract), 'utf8'));
const schemas = document.components.schemas;

const drift = [];
let compared = 0;
for (const [schemaName, schema] of Object.entries(schemas)) {
  const fields = structFields(schemaName);
  if (fields === null) continue;
  compared += 1;
  const declared = new Set(Object.keys(schema.properties ?? {}));
  const served = new Set(fields.values());
  const missing = [...served].filter((name) => !declared.has(name)).sort();
  const unused = [...declared].filter((name) => !served.has(name)).sort();
  if (missing.length > 0 || unused.length > 0) {
    drift.push({ schema: schemaName, missing, unused, reason: 'unclassified' });
  }
}
assert.ok(compared >= 50, `expected at least 50 paired schemas, compared ${compared}`);

const recorded = new Map(KNOWN_DRIFT.map((entry) => [entry.schema, entry]));
const unknown = drift.filter((entry) => !recorded.has(entry.schema));
assert.deepEqual(
  unknown,
  [],
  `contract schema drift is not recorded in KNOWN_DRIFT: ${unknown.map((entry) => entry.schema).join(', ')}`,
);

// Detect the reason drifting away from the measurement: an entry that still
// exists but whose `missing` list no longer describes reality, or an entry that
// is clean now, means the ledger is out of date - and a stale allowlist is how
// this kind of gate quietly stops protecting anything.
const present = new Map(drift.map((entry) => [entry.schema, entry]));
const stale = [];
for (const entry of KNOWN_DRIFT) {
  const actual = present.get(entry.schema);
  if (actual === undefined) {
    stale.push(`${entry.schema} (no longer drifting - remove the entry)`);
    continue;
  }
  if (JSON.stringify(actual.missing) !== JSON.stringify([...entry.missing].sort())) {
    stale.push(
      `${entry.schema} (recorded missing ${JSON.stringify(entry.missing)}, measured ${JSON.stringify(actual.missing)})`,
    );
  }
}
assert.deepEqual(stale, [], `KNOWN_DRIFT is out of date: ${stale.join('; ')}`);

// The cluster surfaces this repository owns are closed: any drift that reappears
// there is a regression, not debt.
for (const schemaName of [
  'ClusterResponse',
  'ClusterHostResponse',
  'ClusterInstanceResponse',
  'CreateClusterRequest',
  'UpdateClusterRequest',
  'UpdateClusterInstanceRequest',
]) {
  assert.ok(
    !present.has(schemaName),
    `${schemaName} drifted from the Rust contract again: ${JSON.stringify(present.get(schemaName))}`,
  );
}

console.log(
  `contract schema field parity: ${compared} paired schemas compared, ` +
    `${drift.length} recorded debt entries, ${KNOWN_DRIFT.length} in the ledger`,
);
