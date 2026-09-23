#!/usr/bin/env node
/**
 * Differential gate: the Rust data plane against a real nginx oracle.
 *
 * `tests/nginx/full-surface` asserts behaviour against this repository's own
 * expectations, so every case inherits whatever the implementation believes.
 * `tests/nginx/differential` is the opposite: one 50-case battery over
 * conditionals, ranges and cache policy, captured from two endpoints and
 * compared field by field. Its value is entirely in the *oracle*, so the gate
 * makes the oracle's provenance explicit instead of implicit:
 *
 * - the fixture mtime is pinned before either side is measured, because git does
 *   not preserve mtimes and `ETag` is `<mtime>-<size>`; without the pin a fresh
 *   checkout would compare two different tags;
 * - the gateway is started on a port real nginx does not own, and a busy port
 *   fails the run: Windows localhost forwarding can route a connection into a
 *   WSL listener, which previously made both batteries measure nginx against
 *   itself and report pure wall-clock noise as eight behavioural differences;
 * - our capture is rejected outright when it answers with an `nginx/` banner, and
 *   the committed baseline is rejected when it does *not*, so each side must
 *   prove which implementation produced it;
 * - the case count is checked against a floor, so deleting a case cannot shrink
 *   coverage silently.
 *
 * The baseline is `tests/nginx/differential/nginx-oracle.json`, captured from
 * nginx 1.29.6. `--refresh-oracle` re-captures it (needs a real nginx: a native
 * one on PATH, or WSL on Windows). Without an oracle and without a committed
 * baseline the gate skips loudly rather than passing silently; `--require-oracle`
 * turns that skip into a failure for release verification.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const differentialDir = path.join(root, 'tests', 'nginx', 'differential');
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const FIXTURE = path.join(differentialDir, 'public', 'big.txt');
const BASELINE = path.join(differentialDir, 'nginx-oracle.json');
const RENDERED = path.join(differentialDir, 'nginx.rendered.conf');
const OUR_CAPTURE = path.join(root, 'tmp', 'nginx-differential-ours.json');
const GATEWAY_PORT = 20991;
const CASE_FLOOR = 50;
/** 2020-01-01T00:00:00Z — the epoch the fixture's ETag is derived from. */
const PINNED_MTIME_SECONDS = 1577836800;
const ORACLE_UNAVAILABLE_EXITS = new Set([3, 4, 5]);

const log = (message) => process.stderr.write(`[nginx-differential] ${message}\n`);

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

/**
 * Newest existing build wins. The workspace `target/debug` tree is locked while
 * an operator's server runs out of it, so a freshly built copy elsewhere has to
 * take precedence; picking the first path that exists would silently exercise a
 * stale revision and report a green run for code that never ran.
 */
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

/**
 * Render a document root in the form the nginx-compat validator accepts.
 *
 * The validator requires a POSIX absolute path (it fails closed on anything not
 * starting with `/`), so a Windows drive path is reduced to its drive-relative
 * form; the gateway is spawned with the repository as its working directory, so
 * the drive it resolves against is the repository's own. Mirrors `posix_root` in
 * tests/nginx/full-surface/probe.py, which is the repository's convention.
 */
function posixRoot(target) {
  return path.resolve(target).replace(/\\/g, '/').replace(/^[A-Za-z]:\//, '/');
}

function renderConf(publicRoot, port) {
  const text = fs
    .readFileSync(path.join(differentialDir, 'nginx.conf'), 'utf8')
    .replaceAll('@PUBLIC@', publicRoot)
    .replaceAll('@PORT@', String(port));
  for (const placeholder of ['@PUBLIC@', '@PORT@']) {
    if (text.includes(placeholder)) {
      throw new Error(`placeholder ${placeholder} left unrendered`);
    }
  }
  fs.writeFileSync(RENDERED, text);
}

/** Windows reaches the oracle through WSL; other platforms use a native nginx. */
function oracleCommand() {
  const script = path.join(differentialDir, 'run-nginx.sh');
  if (process.platform === 'win32') {
    const match = /^([A-Za-z]):[\\/](.*)$/.exec(path.resolve(script));
    if (!match) return null;
    const wslPath = `/mnt/${match[1].toLowerCase()}/${match[2].replace(/\\/g, '/')}`;
    return { command: 'wsl', args: ['-e', 'bash', wslPath] };
  }
  return { command: 'bash', args: [script] };
}

function captureOracle() {
  const invocation = oracleCommand();
  if (!invocation) return { status: 'unavailable', reason: 'cannot address the oracle script' };
  const result = spawnSync(invocation.command, invocation.args, { cwd: root, stdio: 'inherit' });
  if (result.error) return { status: 'unavailable', reason: result.error.message };
  if (ORACLE_UNAVAILABLE_EXITS.has(result.status)) return { status: 'unavailable' };
  if (result.status !== 0) return { status: 'failed', status_code: result.status };
  return { status: 'captured' };
}

function runBattery(python, port) {
  const result = spawnSync(
    python,
    [path.join(differentialDir, 'battery.py'), '--port', String(port), '--out', OUR_CAPTURE],
    { cwd: root, encoding: 'utf8' },
  );
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  process.stderr.write(output);
  if (result.status !== 0) return { ok: false, cases: 0 };
  const match = output.match(/\((\d+) cases\)/);
  return { ok: true, cases: match ? Number(match[1]) : 0 };
}

function runCompare(python, expected, actual) {
  const result = spawnSync(
    python,
    [path.join(differentialDir, 'compare.py'), '--expected', expected, '--actual', actual],
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
    return requireOracle ? 1 : 0;
  }
  if (!fs.existsSync(FIXTURE)) {
    log(`FAIL: missing fixture ${FIXTURE}`);
    return 1;
  }

  // The tags are `<mtime>-<size>`, so the fixture window has to be identical on
  // both sides before either one is measured.
  fs.utimesSync(FIXTURE, PINNED_MTIME_SECONDS, PINNED_MTIME_SECONDS);
  log(`fixture mtime pinned to ${new Date(PINNED_MTIME_SECONDS * 1000).toISOString()}`);

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
  log(`baseline identity: server=${recordedServer}`);

  if (!(await portFree(GATEWAY_PORT))) {
    log(`FAIL: 127.0.0.1:${GATEWAY_PORT} is already listening; refusing to measure someone else's server`);
    return 1;
  }

  const binary = findBinary();
  if (!binary) {
    log('SKIP: no gateway binary found (build it, or set SDKWORK_WEBSERVER_GATEWAY_BIN)');
    return requireOracle ? 1 : 0;
  }
  if (staleness(binary, root).stale) {
    log(`FAIL: ${stalenessMessage(binary, root, 'node tools/run-nginx-differential.mjs')}`);
    return 1;
  }

  renderConf(posixRoot(path.join(differentialDir, 'public')), GATEWAY_PORT);

  const logPath = path.join(root, 'tmp', 'nginx-differential-gateway.log');
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
    log(`gateway identity witness: server=${JSON.stringify(oursServer)} (no banner, as expected)`);

    const matched = runCompare(python, BASELINE, OUR_CAPTURE);
    log(
      matched
        ? `PASS: ${battery.cases} differential cases match real nginx (allowances audited by compare.py)`
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
