#!/usr/bin/env node
/**
 * Host-selection differential gate: the Rust data plane against a real nginx
 * oracle.
 *
 * The slice compares virtual-host selection (PRD-nginx-compatibility §5) and
 * the bounded named-location internal redirect (§6) — exact, leading/trailing
 * wildcard, dot-prefixed and regex `server_name` forms, precedence between the
 * tables, default-server fallback, and `try_files`/`error_page` named targets.
 * `tests/nginx/full-surface` asserts behaviour against this repository's own
 * expectations, so every case inherits whatever the implementation believes;
 * this slice is the opposite: the expected side is `nginx-oracle.json`,
 * captured from the pinned real nginx 1.29.6 package binary, and only status +
 * body are compared.
 *
 * The gate makes provenance explicit:
 * - each side is started on its own port, and a busy port fails the run rather
 *   than silently measuring the other implementation;
 * - the committed baseline is rejected unless it names the pinned nginx build,
 *   and our capture is rejected if it answers with the nginx banner;
 * - the case count has a floor, so deleting cases cannot shrink coverage
 *   silently.
 *
 * `--refresh-oracle` re-captures the baseline (needs a real nginx: `NGINX_BIN`,
 * one on PATH, or the extracted 1.29.6 binary under /tmp/nginx-deb). Without an
 * oracle and without a committed baseline the gate skips loudly rather than
 * passing silently; `--require-oracle` and SDKWORK_REQUIRE_NGINX_EVIDENCE=1 turn
 * that skip into a failure for release verification.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sliceDir = path.join(root, 'tests', 'nginx', 'differential-host');
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const BASELINE = path.join(sliceDir, 'nginx-oracle.json');
const RENDERED = path.join(sliceDir, 'nginx.rendered.conf');
const OUR_CAPTURE = path.join(root, 'tmp', 'nginx-differential-host-ours.json');
const ORACLE_PORT = 21010;
const GATEWAY_PORT = 21011;
const MOCK_PORT = 21012;
const CASE_FLOOR = 39;
/** The exact nginx build the committed oracle was captured from. */
const REFERENCE_NGINX = 'nginx/1.29.6';
const REQUIRE_EVIDENCE = process.env.SDKWORK_REQUIRE_NGINX_EVIDENCE === '1';
const ORACLE_UNAVAILABLE_EXITS = new Set([3, 4, 5]);

const log = (message) => process.stderr.write(`[nginx-differential-host] ${message}\n`);

function resolvePython() {
  for (const candidate of ['python3', 'python']) {
    const probe = spawnSync(candidate, ['--version'], { encoding: 'utf8' });
    if (probe.status === 0) return candidate;
  }
  return null;
}

function candidateBinaries() {
  const candidates = [];
  if (process.env.SDKWORK_WEBSERVER_GATEWAY_BIN) {
    candidates.push(process.env.SDKWORK_WEBSERVER_GATEWAY_BIN);
  }
  const targetRoots = [path.join(root, 'target')];
  if (process.env.CARGO_TARGET_DIR) {
    targetRoots.push(path.resolve(root, process.env.CARGO_TARGET_DIR));
  }
  for (const targetRoot of targetRoots) {
    for (const profile of ['debug', 'release']) {
      for (const suffix of ['.exe', '']) {
        candidates.push(path.join(targetRoot, profile, `${BINARY_STEM}${suffix}`));
      }
    }
  }
  return candidates;
}

/** Newest existing build wins, so a fresh build never loses to a stale one. */
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
    if (best === null || stat.mtimeMs > best.mtimeMs) {
      best = { path: candidate, mtimeMs: stat.mtimeMs };
    }
  }
  if (best !== null) {
    log(`binary: ${best.path} (mtime ${new Date(best.mtimeMs).toISOString()})`);
    return best.path;
  }
  return null;
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function portFree(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once('error', () => resolve(false));
    server.once('listening', () => server.close(() => resolve(true)));
    server.listen(port, '127.0.0.1');
  });
}

async function waitForPort(port, deadlineMs) {
  const deadline = Date.now() + deadlineMs;
  while (Date.now() < deadline) {
    const reachable = await new Promise((resolve) => {
      const socket = net.createConnection({ host: '127.0.0.1', port });
      socket.once('connect', () => {
        socket.destroy();
        resolve(true);
      });
      socket.once('error', () => {
        socket.destroy();
        resolve(false);
      });
    });
    if (reachable) return true;
    await delay(250);
  }
  return false;
}

function posixRoot(target) {
  return path.resolve(target).replace(/\\/g, '/').replace(/^[A-Za-z]:\//, '/');
}

function renderConf(port) {
  let text = fs.readFileSync(path.join(sliceDir, 'nginx.conf'), 'utf8');
  text = text
    .replaceAll('@PUBLIC@', posixRoot(path.join(sliceDir, 'public')))
    .replaceAll('@PORT@', String(port))
    .replaceAll('@MOCK@', String(MOCK_PORT));
  for (const placeholder of ['@PUBLIC@', '@PORT@', '@MOCK@']) {
    if (text.includes(placeholder)) {
      throw new Error(`placeholder ${placeholder} left unrendered`);
    }
  }
  fs.writeFileSync(RENDERED, text);
}

function captureOracle() {
  const script = path.join(sliceDir, 'run-nginx.sh');
  const result = spawnSync('bash', [script], {
    cwd: root,
    stdio: 'inherit',
    env: {
      ...process.env,
      NGINX_HOST_ORACLE_PORT: String(ORACLE_PORT),
      NGINX_HOST_MOCK_PORT: String(MOCK_PORT),
      NGINX_HOST_ORACLE_OUT: BASELINE,
    },
  });
  if (result.error) return { status: 'unavailable', reason: result.error.message };
  if (ORACLE_UNAVAILABLE_EXITS.has(result.status)) return { status: 'unavailable' };
  if (result.status !== 0) return { status: 'failed', statusCode: result.status };
  return { status: 'captured' };
}

function runBattery(python, port) {
  const result = spawnSync(
    python,
    [
      path.join(sliceDir, 'battery.py'),
      '--port',
      String(port),
      '--mock-port',
      String(MOCK_PORT),
      '--out',
      OUR_CAPTURE,
    ],
    { cwd: root, encoding: 'utf8' },
  );
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  process.stderr.write(output);
  if (result.status !== 0) return { ok: false, cases: 0 };
  const match = output.match(/captured (\d+) host-selection cases/);
  return { ok: true, cases: match ? Number(match[1]) : 0 };
}

function runCompare(python, expected, actual) {
  const result = spawnSync(
    python,
    [
      path.join(sliceDir, 'compare.py'),
      '--expected',
      expected,
      '--actual',
      actual,
    ],
    { cwd: root, stdio: 'inherit' },
  );
  return result.status === 0;
}

async function main() {
  const refreshOracle = process.argv.includes('--refresh-oracle');
  const requireOracle = process.argv.includes('--require-oracle');

  const python = resolvePython();
  if (!python) {
    log('SKIP: no python3/python on PATH; the battery cannot run');
    return requireOracle || REQUIRE_EVIDENCE ? 1 : 0;
  }

  for (const port of [ORACLE_PORT, GATEWAY_PORT, MOCK_PORT]) {
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} is already listening; refusing to measure someone else's server`);
      return 1;
    }
  }

  let baseline = fs.existsSync(BASELINE);
  if (refreshOracle || !baseline) {
    log(refreshOracle ? 'refreshing the nginx oracle baseline…' : 'no committed baseline; capturing one…');
    const oracle = captureOracle();
    if (oracle.status === 'captured') {
      baseline = true;
      log(`oracle baseline written to ${path.relative(root, BASELINE)}`);
    } else {
      log(`oracle ${oracle.status}: ${oracle.reason ?? 'no real nginx available here'}`);
      if (!baseline) {
        if (requireOracle) {
          log('FAIL: --require-oracle was requested but no oracle produced a baseline');
          return 1;
        }
        log('SKIP: no baseline to compare against; the host-selection differential did NOT run');
        return REQUIRE_EVIDENCE ? 1 : 0;
      }
      log('falling back to the committed baseline');
    }
  }

  const recorded = JSON.parse(fs.readFileSync(BASELINE, 'utf8'));
  const recordedServer = String(recorded._server ?? '');
  if (recordedServer !== REFERENCE_NGINX) {
    log(
      `FAIL: baseline was captured from ${JSON.stringify(recorded._server)} but this gate pins ${REFERENCE_NGINX}; ` +
        're-capture with --refresh-oracle against the pinned build',
    );
    return 1;
  }
  const recordedCases = Number(recorded._cases ?? 0);
  if (recordedCases < CASE_FLOOR) {
    log(`FAIL: the committed baseline has ${recordedCases} cases, below the floor of ${CASE_FLOOR}`);
    return 1;
  }
  log(`baseline identity: server=${recordedServer} cases=${recordedCases}`);

  const binary = findBinary();
  if (!binary) {
    log('SKIP: no gateway binary found (build it, or set SDKWORK_WEBSERVER_GATEWAY_BIN)');
    return requireOracle || REQUIRE_EVIDENCE ? 1 : 0;
  }
  if (staleness(binary, root).stale) {
    log(`FAIL: ${stalenessMessage(binary, root, 'node tools/run-nginx-differential-host.mjs')}`);
    return 1;
  }

  renderConf(GATEWAY_PORT);

  const logPath = path.join(root, 'tmp', 'nginx-differential-host-gateway.log');
  fs.mkdirSync(path.dirname(logPath), { recursive: true });
  const logHandle = fs.openSync(logPath, 'w');
  const gateway = spawn(binary, ['serve-nginx', RENDERED], {
    cwd: root,
    stdio: ['ignore', logHandle, logHandle],
  });

  let exitCode = 1;
  try {
    if (!(await waitForPort(GATEWAY_PORT, 20_000))) {
      log(`FAIL: the gateway never listened on ${GATEWAY_PORT}; see ${path.relative(root, logPath)}`);
      return 1;
    }
    if (gateway.exitCode !== null) {
      log(`FAIL: the gateway exited with ${gateway.exitCode} instead of serving`);
      return 1;
    }

    const battery = runBattery(python, GATEWAY_PORT);
    if (!battery.ok) {
      log('FAIL: the battery could not capture the gateway');
      return 1;
    }
    if (battery.cases < CASE_FLOOR) {
      log(`FAIL: the battery reported ${battery.cases} cases, below the floor of ${CASE_FLOOR}`);
      return 1;
    }

    const ours = JSON.parse(fs.readFileSync(OUR_CAPTURE, 'utf8'));
    const oursServer = ours._server;
    if (oursServer !== null && String(oursServer).toLowerCase().startsWith('nginx/')) {
      log(`FAIL: port ${GATEWAY_PORT} answered as ${JSON.stringify(oursServer)}; that is the oracle, not the gateway`);
      return 1;
    }
    log(`gateway identity witness: server=${JSON.stringify(oursServer)} (no nginx banner, as expected)`);

    const matched = runCompare(python, BASELINE, OUR_CAPTURE);
    log(
      matched
        ? `PASS: ${battery.cases} host-selection differential cases match real nginx (allowances audited by compare.py)`
        : 'FAIL: unexpected differential differences (see above)',
    );
    exitCode = matched ? 0 : 1;
  } finally {
    if (gateway.exitCode === null) {
      gateway.kill();
      await Promise.race([
        new Promise((resolve) => gateway.once('exit', resolve)),
        delay(10_000).then(() => gateway.kill('SIGKILL')),
      ]);
    }
    fs.closeSync(logHandle);
  }
  return exitCode;
}

process.exit(await main());
