#!/usr/bin/env node
/**
 * Differential gate for the Wave 5 upstream-protocol adapter (FastCGI slice).
 *
 * The same fixture is rendered twice: real nginx 1.29.6 (`fastcgi_pass`) and the
 * Rust data plane (`serve-nginx`) each talk to their own instance of the
 * deterministic FastCGI responder in `mock_fastcgi.py`. The responder echoes
 * every received `PARAMS` entry and the raw `STDIN` bytes in its JSON body, so
 * the battery compares the parameter mapping (declared params, nginx's automatic
 * `HTTP_*` fields, `SCRIPT_NAME`/`PATH_INFO` from `fastcgi_split_path_info`,
 * `fastcgi_index`, request body bytes) byte for byte, plus status, headers and
 * the `fastcgi_hide_header`/`fastcgi_pass_header`/`fastcgi_intercept_errors`/
 * `fastcgi_next_upstream` mappings.
 *
 * The committed baseline (`nginx-oracle.json`) is a real nginx capture. Build
 * the gateway first (`cargo build -p sdkwork-api-webserver-standalone-gateway
 * --bin sdkwork-api-webserver-standalone-gateway --features management`) and
 * pass `--refresh-oracle` only together with a fresh reference build.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'tests', 'nginx', 'differential-protocols');
const BASELINE = path.join(fixtureDir, 'nginx-oracle.json');
const GATEWAY_CONF_DIR = path.join(root, 'tmp', 'nginx-differential-protocols');
const GATEWAY_CONF = path.join(GATEWAY_CONF_DIR, 'nginx.conf');
const OUR_CAPTURE = path.join(root, 'tmp', 'nginx-differential-protocols-ours.json');
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const REFERENCE_NGINX = 'nginx/1.29.6';
const CASE_FLOOR = 26;
/**
 * nginx renders its own generated error page for a memcached miss/upstream
 * failure; the status mapping is the contract under test there, while the body
 * is the same documented deployment difference the broad HTTP slice allowlists.
 */
const STATUS_ONLY_CASES = new Set([
  'memcached-miss',
  'memcached-raw-502',
  'memcached-post',
]);
const ORACLE_PORT = 21092;
const GATEWAY_PORT = 21093;
const ORACLE_FCGI_PORT = ORACLE_PORT + 2;
const ORACLE_UWSGI_PORT = ORACLE_PORT + 10;
const ORACLE_SCGI_PORT = ORACLE_PORT + 11;
const ORACLE_MEMCACHED_PORT = ORACLE_PORT + 14;
const GATEWAY_FCGI_PORT = GATEWAY_PORT + 2;
const GATEWAY_UWSGI_PORT = GATEWAY_PORT + 10;
const GATEWAY_SCGI_PORT = GATEWAY_PORT + 11;
const GATEWAY_MEMCACHED_PORT = GATEWAY_PORT + 14;
const ORACLE_DEAD_PORT = ORACLE_PORT + 5;
const GATEWAY_DEAD_PORT = GATEWAY_PORT + 5;
const ORACLE_UNAVAILABLE_EXITS = new Set([3, 4, 5]);

const log = (message) => process.stderr.write(`[nginx-differential-protocols] ${message}\n`);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function resolvePython() {
  for (const candidate of ['python3', 'python']) {
    const result = spawnSync(candidate, ['--version'], { stdio: 'ignore' });
    if (!result.error && result.status === 0) return candidate;
  }
  return null;
}

function portFree(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once('error', () => resolve(false));
    server.once('listening', () => server.close(() => resolve(true)));
    server.listen(port, '127.0.0.1');
  });
}

async function waitForPort(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const free = await portFree(port);
    if (!free) return true;
    await delay(100);
  }
  return false;
}

function renderTemplate(port, fcgiPort, uwsgiPort, scgiPort, memcachedPort, deadPort) {
  let text = fs.readFileSync(path.join(fixtureDir, 'nginx.conf'), 'utf8');
  text = text
    .replaceAll('@ROOT@', path.join(fixtureDir, 'public'))
    .replaceAll('@PORT@', String(port))
    .replaceAll('@FCGI@', String(fcgiPort))
    .replaceAll('@UWSGI@', String(uwsgiPort))
    .replaceAll('@SCGI@', String(scgiPort))
    .replaceAll('@MEMCACHED@', String(memcachedPort))
    .replaceAll('@DEAD@', String(deadPort));
  for (const marker of ['@ROOT@', '@PORT@', '@FCGI@', '@UWSGI@', '@SCGI@', '@MEMCACHED@', '@DEAD@']) {
    if (text.includes(marker)) throw new Error(`placeholder ${marker} left unrendered`);
  }
  return text;
}

function candidateBinaries() {
  const candidates = [];
  if (process.env.SDKWORK_WEBSERVER_GATEWAY_BIN) {
    candidates.push(process.env.SDKWORK_WEBSERVER_GATEWAY_BIN);
  }
  for (const profile of ['debug', 'release']) {
    for (const suffix of ['.exe', '']) {
      candidates.push(path.join(root, 'target', profile, `${BINARY_STEM}${suffix}`));
    }
  }
  return candidates;
}

function findBinary() {
  let best = null;
  for (const candidate of candidateBinaries()) {
    let stat;
    try {
      stat = fs.statSync(candidate);
    } catch {
      continue;
    }
    if (!stat.isFile()) continue;
    if (best === null || stat.mtimeMs > best.mtimeMs) best = { path: candidate, mtimeMs: stat.mtimeMs };
  }
  return best;
}

function captureOracle() {
  const result = spawnSync('bash', [path.join(fixtureDir, 'run-nginx.sh')], {
    cwd: root,
    stdio: 'inherit',
    env: {
      ...process.env,
      NGINX_DIFF_PROTOCOLS_ORACLE_PORT: String(ORACLE_PORT),
      NGINX_DIFF_PROTOCOLS_WORK: path.join(root, 'tmp', 'nginx-differential-protocols-oracle'),
    },
  });
  if (result.error) return { status: 'unavailable', reason: result.error.message };
  if (ORACLE_UNAVAILABLE_EXITS.has(result.status)) return { status: 'unavailable' };
  if (result.status !== 0) return { status: 'failed', code: result.status };
  return { status: 'captured' };
}

function runBattery(python, port) {
  const result = spawnSync(
    python,
    [path.join(fixtureDir, 'battery.py'), '--port', String(port), '--out', OUR_CAPTURE],
    { cwd: root, encoding: 'utf8' },
  );
  process.stderr.write(`${result.stdout ?? ''}${result.stderr ?? ''}`);
  return result.status === 0;
}

function compare(expected, actual, statusOnlyCases = new Set()) {
  const differences = [];
  const expectedCases = expected.cases ?? {};
  const actualCases = actual.cases ?? {};
  const names = new Set([...Object.keys(expectedCases), ...Object.keys(actualCases)]);
  for (const name of [...names].sort()) {
    const want = expectedCases[name];
    const got = actualCases[name];
    if (!want) {
      differences.push(`${name}: unexpected case`);
      continue;
    }
    if (!got) {
      differences.push(`${name}: missing case`);
      continue;
    }
    if (want.status !== got.status) {
      differences.push(`${name}: status ${want.status} != ${got.status}`);
    }
    if (statusOnlyCases.has(name)) continue;
    const wantHeaders = JSON.stringify(want.headers ?? {});
    const gotHeaders = JSON.stringify(got.headers ?? {});
    if (wantHeaders !== gotHeaders) {
      differences.push(`${name}: headers ${wantHeaders} != ${gotHeaders}`);
    }
    if (want.body !== got.body) {
      differences.push(`${name}: body differs`);
    }
  }
  return differences;
}

async function main() {
  const refreshOracle = process.argv.includes('--refresh-oracle');
  const requireOracle = process.argv.includes('--require-oracle');
  const python = resolvePython();
  if (!python) {
    log('SKIP: no python3/python on PATH; the battery cannot run');
    return requireOracle ? 1 : 0;
  }

  let baseline = fs.existsSync(BASELINE);
  if (refreshOracle || !baseline) {
    log(refreshOracle ? 'refreshing the nginx oracle baseline…' : 'no baseline; capturing one…');
    const oracle = captureOracle();
    if (oracle.status === 'captured') {
      baseline = true;
      log(`oracle baseline written to ${path.relative(root, BASELINE)}`);
    } else {
      log(`oracle ${oracle.status}: ${oracle.reason ?? ''}`);
      if (!baseline) {
        if (requireOracle) {
          log('FAIL: --require-oracle was requested but no oracle produced a baseline');
          return 1;
        }
        log('SKIP: no baseline to compare against; the differential slice did NOT run');
        return 0;
      }
      log('falling back to the committed baseline');
    }
  }

  const recorded = JSON.parse(fs.readFileSync(BASELINE, 'utf8'));
  const recordedServer = String(recorded._server ?? '');
  if (!recordedServer.toLowerCase().startsWith('nginx/')) {
    log(`FAIL: baseline is not a real nginx capture (_server=${JSON.stringify(recorded._server)})`);
    return 1;
  }
  if (recordedServer !== REFERENCE_NGINX) {
    log(`FAIL: baseline was captured from ${recordedServer}, but this gate pins ${REFERENCE_NGINX}`);
    return 1;
  }
  const recordedCases = Object.keys(recorded.cases ?? {}).length;
  if (recordedCases < CASE_FLOOR) {
    log(`FAIL: the committed baseline holds ${recordedCases} cases, below the floor of ${CASE_FLOOR}`);
    return 1;
  }

  for (const [label, port] of [
    ['gateway listener', GATEWAY_PORT],
    ['gateway FastCGI responder', GATEWAY_FCGI_PORT],
    ['gateway uWSGI responder', GATEWAY_UWSGI_PORT],
    ['gateway SCGI responder', GATEWAY_SCGI_PORT],
    ['gateway memcached responder', GATEWAY_MEMCACHED_PORT],
    ['gateway dead upstream', GATEWAY_DEAD_PORT],
  ]) {
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} (${label}) is already listening`);
      return 1;
    }
  }
  const binary = findBinary();
  if (!binary) {
    log('SKIP: no gateway binary found (build it, or set SDKWORK_WEBSERVER_GATEWAY_BIN)');
    return requireOracle ? 1 : 0;
  }
  if (staleness(binary.path, root).stale) {
    log(`FAIL: ${stalenessMessage(binary.path, root, 'node tools/run-nginx-differential-protocols.mjs')}`);
    return 1;
  }

  fs.mkdirSync(GATEWAY_CONF_DIR, { recursive: true });
  fs.writeFileSync(
    GATEWAY_CONF,
    renderTemplate(
      GATEWAY_PORT,
      GATEWAY_FCGI_PORT,
      GATEWAY_UWSGI_PORT,
      GATEWAY_SCGI_PORT,
      GATEWAY_MEMCACHED_PORT,
      GATEWAY_DEAD_PORT,
    ),
  );

  const mockLog = fs.openSync(path.join(root, 'tmp', 'nginx-differential-protocols-mock.log'), 'w');
  const mock = spawn(python, [path.join(fixtureDir, 'mock_fastcgi.py'), '--port', String(GATEWAY_FCGI_PORT)], {
    cwd: root,
    stdio: ['ignore', mockLog, mockLog],
  });
  const mockUwsgi = spawn(python, [path.join(fixtureDir, 'mock_uwsgi.py'), '--port', String(GATEWAY_UWSGI_PORT)], {
    cwd: root,
    stdio: ['ignore', mockLog, mockLog],
  });
  const mockScgi = spawn(python, [path.join(fixtureDir, 'mock_scgi.py'), '--port', String(GATEWAY_SCGI_PORT)], {
    cwd: root,
    stdio: ['ignore', mockLog, mockLog],
  });
  const mockMemcached = spawn(
    python,
    [path.join(fixtureDir, 'mock_memcached.py'), '--port', String(GATEWAY_MEMCACHED_PORT)],
    { cwd: root, stdio: ['ignore', mockLog, mockLog] },
  );
  const gatewayLog = fs.openSync(path.join(root, 'tmp', 'nginx-differential-protocols-gateway.log'), 'w');
  const gateway = spawn(binary.path, ['serve-nginx', GATEWAY_CONF], {
    cwd: root,
    stdio: ['ignore', gatewayLog, gatewayLog],
    env: { ...process.env, TZ: 'UTC' },
  });

  try {
    if (!(await waitForPort(GATEWAY_PORT, 20_000))) {
      log(`FAIL: the gateway never listened on ${GATEWAY_PORT}; see tmp/nginx-differential-protocols-gateway.log`);
      return 1;
    }
    if (gateway.exitCode !== null) {
      log(`FAIL: the gateway exited with ${gateway.exitCode}`);
      return 1;
    }
    if (!runBattery(python, GATEWAY_PORT)) {
      log('FAIL: the battery could not capture the gateway');
      return 1;
    }
    const ours = JSON.parse(fs.readFileSync(OUR_CAPTURE, 'utf8'));
    if (ours._server !== null && String(ours._server).toLowerCase().startsWith('nginx/')) {
      log(`FAIL: port ${GATEWAY_PORT} answered as ${JSON.stringify(ours._server)}; that is the oracle`);
      return 1;
    }
    const cases = Object.keys(ours.cases ?? {}).length;
    if (cases < CASE_FLOOR) {
      log(`FAIL: the battery reported ${cases} cases, below the floor of ${CASE_FLOOR}`);
      return 1;
    }
    const differences = compare(recorded, ours, STATUS_ONLY_CASES);
    if (differences.length > 0) {
      for (const difference of differences) log(`DIFF ${difference}`);
      log('FAIL: unexpected differences against real nginx');
      return 1;
    }
    log(`PASS: ${cases} differential CGI-family cases match real nginx ${recordedServer}`);
    return 0;
  } finally {
    for (const child of [gateway, mock, mockUwsgi, mockScgi, mockMemcached]) {
      if (child.exitCode === null) child.kill();
    }
    await delay(500);
    for (const child of [gateway, mock, mockUwsgi, mockScgi, mockMemcached]) {
      if (child.exitCode === null) child.kill('SIGKILL');
    }
    fs.closeSync(mockLog);
    fs.closeSync(gatewayLog);
  }
}

process.exit(await main());
