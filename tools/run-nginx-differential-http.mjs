#!/usr/bin/env node
/**
 * Differential gate: the Rust data plane against a real nginx oracle for the
 * broad HTTP surface (static delivery, routing/rewrite, response headers, error
 * pages, gzip negotiation, access control, request limits and upstream proxying).
 *
 * `tests/nginx/full-surface` asserts this surface against this repository's own
 * expectations, so every case inherits whatever the implementation believes.
 * `tests/nginx/differential-http` is the opposite: the same fixture rendered for
 * real nginx and for the gateway, measured by one battery and compared field by
 * field, with every intentional difference justified in `compare.py`.
 *
 * The gate therefore makes the oracle's provenance explicit:
 *
 * - the fixture mtimes are pinned before either side is measured, because git
 *   does not preserve mtimes and `ETag` is `<mtime>-<size>` (the battery owns the
 *   pin so both sides receive the same values);
 * - the committed baseline must be an actual nginx capture from the pinned
 *   reference build, and our capture must NOT answer with an `nginx/` banner —
 *   each side proves which implementation produced it;
 * - a busy gateway port fails the run instead of measuring somebody else's
 *   server, and the gateway binary must be newer than the sources it exercises;
 * - the executed case count is checked against a floor, so deleting cases cannot
 *   shrink coverage silently;
 * - the fixture bytes are checked against the size the baseline was captured
 *   from, so a line-ending conversion cannot silently redefine the contract.
 *
 * The baseline is `tests/nginx/differential-http/nginx-oracle.json`, captured
 * from nginx 1.29.6 (`--refresh-oracle`). Without an oracle and without a
 * committed baseline the gate skips loudly rather than passing silently;
 * `--require-oracle` turns that skip into a failure for release verification.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const differentialDir = path.join(root, 'tests', 'nginx', 'differential-http');
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const FIXTURE = path.join(differentialDir, 'public', 'hello.txt');
const BASELINE = path.join(differentialDir, 'nginx-oracle.json');
const RENDERED = path.join(differentialDir, 'nginx.rendered.conf');
/**
 * The gateway is served from a *generated* configuration inside `tmp/`, not the
 * committed `nginx.rendered.conf` review artifact: the empty-string-key `map`
 * block is injected into it, and the committed artifact must stay parseable by
 * the repository's JS configuration parser (the directive-matrix gate scans
 * every committed `*.conf`).
 */
const GATEWAY_CONF_DIR = path.join(root, 'tmp', 'nginx-differential-http');
const GATEWAY_CONF = path.join(GATEWAY_CONF_DIR, 'nginx.conf');
const OUR_CAPTURE = path.join(root, 'tmp', 'nginx-differential-http-ours.json');
const ORACLE_PORT = 20992;
const GATEWAY_PORT = 20993;
/**
 * The mock upstream and the unreachable-upstream target are derived from the
 * listen port so the oracle and the gateway never share a socket:
 * oracle runs 20994/20997, gateway runs 20995/20998.
 */
const ORACLE_UPSTREAM_PORT = ORACLE_PORT + 2;
const ORACLE_DEAD_PORT = ORACLE_PORT + 5;
const GATEWAY_UPSTREAM_PORT = GATEWAY_PORT + 2;
const GATEWAY_DEAD_PORT = GATEWAY_PORT + 5;
const CASE_FLOOR = 236;
/**
 * The exact nginx build the committed oracle was captured from. Bump it only
 * together with a fresh `--refresh-oracle` capture and the per-slice note in
 * `specs/nginx-behavioral-corpus.manifest.json`, so the expected behaviour and
 * its reference always move as one reviewed change.
 */
const REFERENCE_NGINX = 'nginx/1.29.6';
const ORACLE_UNAVAILABLE_EXITS = new Set([3, 4, 5]);

const log = (message) => process.stderr.write(`[nginx-differential-http] ${message}\n`);

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

/** Newest existing build wins; a stale first path would report a green run for code that never ran. */
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

function renderConf(publicRoot, listenPort, upstreamPort, deadPort) {
  const template = fs
    .readFileSync(path.join(differentialDir, 'nginx.conf'), 'utf8')
    .replaceAll('@PUBLIC@', publicRoot)
    .replaceAll('@PORT@', String(listenPort))
    .replaceAll('@UPSTREAM@', String(upstreamPort))
    .replaceAll('@DEAD@', String(deadPort))
    .replaceAll('@HTPASSWD@', posixRoot(path.join(differentialDir, 'htpasswd')));
  const emptyKeyMaps = fs.readFileSync(
    path.join(differentialDir, 'maps-empty-key.inc'),
    'utf8',
  );
  // Gateway configuration: the empty-string-key maps are injected inline.
  const gatewayText = template.replaceAll('# @EMPTY_KEY_MAP@', emptyKeyMaps.trimEnd());
  if (gatewayText.includes('@EMPTY_KEY_MAP@')) {
    throw new Error('empty-key map marker was left unrendered');
  }
  fs.mkdirSync(GATEWAY_CONF_DIR, { recursive: true });
  fs.writeFileSync(GATEWAY_CONF, gatewayText);
  // The configuration includes `maps.fragment`, which nginx resolves against
  // the rendered configuration's own directory; mirror that for the gateway
  // loader (its base directory is the served configuration's directory).
  fs.copyFileSync(
    path.join(differentialDir, 'maps.fragment'),
    path.join(GATEWAY_CONF_DIR, 'maps.fragment'),
  );
  // The committed review artifact keeps the marker comment and the include, so
  // the fixture-directive scan can parse it.
  const text = template;
  for (const placeholder of [
    '@PUBLIC@',
    '@PORT@',
    '@UPSTREAM@',
    '@DEAD@',
    '@HTPASSWD@',
  ]) {
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

function runBattery(python, port, upstreamPort, deadPort) {
  const result = spawnSync(
    python,
    [
      path.join(differentialDir, 'battery.py'),
      '--port',
      String(port),
      '--upstream-port',
      String(upstreamPort),
      '--dead-port',
      String(deadPort),
      '--out',
      OUR_CAPTURE,
    ],
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
  if (recordedServer !== REFERENCE_NGINX) {
    log(
      `FAIL: baseline was captured from ${recordedServer}, but this gate pins ${REFERENCE_NGINX}; ` +
        're-capture with --refresh-oracle against the pinned build and update the manifest note',
    );
    return 1;
  }
  const fixtureBytes = fs.statSync(FIXTURE).size;
  const recordedBytes = Number(recorded._fixture_hello_bytes);
  if (!Number.isInteger(recordedBytes) || fixtureBytes !== recordedBytes) {
    log(
      `FAIL: fixture is ${fixtureBytes} bytes but the baseline recorded ${recordedBytes}; ` +
        'the fixture checkout changed (line endings?) - restore it or re-capture the oracle',
    );
    return 1;
  }
  const recordedCases = Object.keys(recorded).filter((key) => !key.startsWith('_')).length;
  if (recordedCases < CASE_FLOOR) {
    log(`FAIL: the committed baseline holds ${recordedCases} cases, below the floor of ${CASE_FLOOR}`);
    return 1;
  }
  log(
    `baseline identity: server=${recordedServer} fixture=${fixtureBytes} bytes cases=${recordedCases}`,
  );

  if (!(await portFree(GATEWAY_PORT))) {
    log(`FAIL: 127.0.0.1:${GATEWAY_PORT} is already listening; refusing to measure someone else's server`);
    return 1;
  }
  // The battery binds the mock upstream and the 502 case needs a genuinely
  // unreachable port; either being busy would measure a different server.
  for (const [label, port] of [
    ['mock upstream', GATEWAY_UPSTREAM_PORT],
    ['dead upstream', GATEWAY_DEAD_PORT],
  ]) {
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} (${label}) is already listening; refusing to measure it`);
      return 1;
    }
  }

  const binary = findBinary();
  if (!binary) {
    log('SKIP: no gateway binary found (build it, or set SDKWORK_WEBSERVER_GATEWAY_BIN)');
    return requireOracle ? 1 : 0;
  }
  if (staleness(binary, root).stale) {
    log(`FAIL: ${stalenessMessage(binary, root, 'node tools/run-nginx-differential-http.mjs')}`);
    return 1;
  }

  renderConf(
    posixRoot(path.join(differentialDir, 'public')),
    GATEWAY_PORT,
    GATEWAY_UPSTREAM_PORT,
    GATEWAY_DEAD_PORT,
  );

  const logPath = path.join(root, 'tmp', 'nginx-differential-http-gateway.log');
  fs.mkdirSync(path.dirname(logPath), { recursive: true });
  const logHandle = fs.openSync(logPath, 'w');
  // The autoindex fixture pins `autoindex_localtime on`; nginx runs with
  // TZ=UTC (run-nginx.sh) so the gateway must see the same zone for the
  // rendered mtimes to be the compared contract rather than a host artefact.
  const gateway = spawn(binary, ['serve-nginx', GATEWAY_CONF], {
    cwd: root,
    stdio: ['ignore', logHandle, logHandle],
    env: { ...process.env, TZ: 'UTC' },
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

    const battery = runBattery(
      python,
      GATEWAY_PORT,
      GATEWAY_UPSTREAM_PORT,
      GATEWAY_DEAD_PORT,
    );
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
