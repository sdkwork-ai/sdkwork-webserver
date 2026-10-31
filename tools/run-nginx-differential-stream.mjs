#!/usr/bin/env node
/**
 * Differential gate for the stream plane: the Rust data plane against a real
 * nginx oracle.
 *
 * `tests/nginx/differential-stream` renders the same stream fixture twice (real
 * nginx as the oracle, `serve-nginx` as the runtime) and drives one connection
 * battery against both sides. The battery records per-connection outcomes
 * (echoed bytes vs. silent close) for:
 *
 * - pass-through proxying behind the ordered stream access phase (`allow`/`deny`),
 * - the stream `return <value>` handler,
 * - `limit_conn` (over-limit connections are closed silently; the slot is
 *   released after the holder disconnects),
 * - `limit_conn_dry_run` (over-limit connections are counted but still proxied).
 *
 * The committed baseline (`nginx-oracle.json`) is a real nginx capture. Build the
 * gateway first; `--refresh-oracle` recaptures the baseline from the pinned
 * nginx build. A busy gateway port fails the run instead of measuring another
 * server, the gateway binary must be newer than the sources it exercises, and
 * the executed case count is checked against a floor.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const differentialDir = path.join(root, 'tests', 'nginx', 'differential-stream');
const BASELINE = path.join(differentialDir, 'nginx-oracle.json');
const GATEWAY_CONF_DIR = path.join(root, 'tmp', 'nginx-differential-stream');
const GATEWAY_CONF = path.join(GATEWAY_CONF_DIR, 'nginx.conf');
const OUR_CAPTURE = path.join(root, 'tmp', 'nginx-differential-stream-ours.json');
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const REFERENCE_NGINX = 'nginx/1.29.6';
const CASE_FLOOR = 14;
const PORT_SEARCH_START = 23100;
const PORT_SEARCH_END = 23900;
const ORACLE_UNAVAILABLE_EXITS = new Set([3, 4, 5]);

const log = (message) => process.stderr.write(`[nginx-differential-stream] ${message}\n`);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

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

/** Newest existing build wins; a stale first path would test code that never ran. */
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
    return best;
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

/** Allocate one free loopback port in a range that other slices do not use. */
async function pickFreePort(cursor) {
  for (let port = cursor; port <= PORT_SEARCH_END; port += 1) {
    if (await portFree(port)) return port;
  }
  throw new Error(`no free loopback port in ${PORT_SEARCH_START}..${PORT_SEARCH_END}`);
}

/** Mint the CA-signed TLS echo identity and the unrelated CA for proxy_ssl. */
function generateStreamCertificates(directory) {
  fs.mkdirSync(directory, { recursive: true });
  const tlsCa = path.join(directory, 'tls-ca.pem');
  const otherCa = path.join(directory, 'tls-other-ca.pem');
  const tlsCert = path.join(directory, 'tls-stream.pem');
  const tlsKey = path.join(directory, 'tls-stream.key');
  if (fs.existsSync(tlsCa) && fs.existsSync(otherCa) && fs.existsSync(tlsCert) && fs.existsSync(tlsKey)) {
    return { tlsCa, otherCa, tlsCert, tlsKey };
  }
  const run = (...args) => {
    const result = spawnSync('openssl', args, { encoding: 'utf8' });
    if (result.error || result.status !== 0) {
      throw new Error(`openssl ${args.join(' ')} failed: ${result.stderr ?? result.error}`);
    }
  };
  run('req', '-x509', '-newkey', 'rsa:2048', '-nodes',
    '-keyout', path.join(directory, 'tls-ca.key'), '-out', tlsCa,
    '-days', '3', '-subj', '/CN=stream-tls-ca');
  run('req', '-x509', '-newkey', 'rsa:2048', '-nodes',
    '-keyout', path.join(directory, 'tls-other-ca.key'), '-out', otherCa,
    '-days', '3', '-subj', '/CN=stream-tls-other-ca');
  const extensions = path.join(directory, 'tls-stream.ext');
  fs.writeFileSync(extensions,
    'subjectAltName=DNS:tls-stream.test\n' +
    'basicConstraints=critical,CA:FALSE\n' +
    'keyUsage=critical,digitalSignature,keyEncipherment\n' +
    'extendedKeyUsage=serverAuth\n');
  run('req', '-newkey', 'rsa:2048', '-nodes', '-keyout', tlsKey,
    '-out', path.join(directory, 'tls-stream.csr'), '-subj', '/CN=tls-stream.test');
  run('x509', '-req', '-in', path.join(directory, 'tls-stream.csr'),
    '-CA', tlsCa, '-CAkey', path.join(directory, 'tls-ca.key'),
    '-CAcreateserial', '-days', '3', '-out', tlsCert, '-extfile', extensions);
  return { tlsCa, otherCa, tlsCert, tlsKey };
}

function renderConf(basePort, upstreamPort, tlsEchoPort, tlsEcho2Port, halfClosePort, certificates) {
  let template;
  try {
    template = fs.readFileSync(path.join(differentialDir, 'nginx.conf'), 'utf8');
  } catch (error) {
    throw new Error(`missing stream fixture nginx.conf: ${error.message}`);
  }
  return template
    .replaceAll('@TLS_CA@', path.basename(certificates.tlsCa))
    .replaceAll('@OTHER_CA@', path.basename(certificates.otherCa))
    .replaceAll('@UPSTREAM_TLS@', String(tlsEchoPort))
    .replaceAll('@UPSTREAM_TLS2@', String(tlsEcho2Port))
    .replaceAll('@UPSTREAM_HC@', String(halfClosePort))
    .replaceAll('@PORT0@', String(basePort))
    .replaceAll('@PORT1@', String(basePort + 1))
    .replaceAll('@PORT2@', String(basePort + 2))
    .replaceAll('@PORT3@', String(basePort + 3))
    .replaceAll('@PORT4@', String(basePort + 4))
    .replaceAll('@PORT5@', String(basePort + 5))
    .replaceAll('@PORT6@', String(basePort + 6))
    .replaceAll('@PORT7@', String(basePort + 7))
    .replaceAll('@PORT8@', String(basePort + 8))
    .replaceAll('@PORT9@', String(basePort + 9))
    .replaceAll('@PORT10@', String(basePort + 10))
    .replaceAll('@PORT11@', String(basePort + 11))
    .replaceAll('@UPSTREAM@', String(upstreamPort));
}

function captureOracle(oraclePort) {
  const result = spawnSync('bash', [path.join(differentialDir, 'run-nginx.sh')], {
    cwd: root,
    encoding: 'utf8',
    env: {
      ...process.env,
      NGINX_DIFF_STREAM_ORACLE_PORT: String(oraclePort),
      NGINX_DIFF_STREAM_WORK: path.join(root, 'tmp', 'nginx-differential-stream-oracle'),
      NGINX_DIFF_STREAM_ORACLE_OUT: BASELINE,
    },
  });
  if (result.error) return { status: 'unavailable', reason: result.error.message };
  if (ORACLE_UNAVAILABLE_EXITS.has(result.status)) {
    return { status: 'unavailable', reason: (result.stderr ?? '').trim().split('\n').at(-1) };
  }
  if (result.status !== 0) return { status: 'failed', code: result.status };
  return { status: 'captured' };
}

function runBattery(python, basePort, upstreamPort, tlsEchoPort, tlsEcho2Port, halfClosePort, certificates) {
  const result = spawnSync(
    python,
    [
      path.join(differentialDir, 'battery.py'),
      '--port0',
      String(basePort),
      '--port1',
      String(basePort + 1),
      '--port2',
      String(basePort + 2),
      '--port3',
      String(basePort + 3),
      '--port4',
      String(basePort + 4),
      '--port5',
      String(basePort + 5),
      '--port6',
      String(basePort + 6),
      '--port7',
      String(basePort + 7),
      '--port8',
      String(basePort + 8),
      '--port9',
      String(basePort + 9),
      '--port10',
      String(basePort + 10),
      '--port11',
      String(basePort + 11),
      '--upstream-port',
      String(upstreamPort),
      '--tls-echo-port',
      String(tlsEchoPort),
      '--tls-echo-2-port',
      String(tlsEcho2Port),
      '--half-close-echo-port',
      String(halfClosePort),
      '--tls-cert',
      certificates.tlsCert,
      '--tls-key',
      certificates.tlsKey,
      '--out',
      OUR_CAPTURE,
    ],
    { cwd: root, encoding: 'utf8' },
  );
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  process.stderr.write(output);
  if (result.status !== 0) return { ok: false, cases: 0 };
  const match = output.match(/\((\d+) stream cases\)/);
  return { ok: true, cases: match ? Number(match[1]) : 0 };
}

function runCompare(python) {
  const result = spawnSync(
    python,
    [
      path.join(differentialDir, 'compare.py'),
      '--expected',
      BASELINE,
      '--actual',
      OUR_CAPTURE,
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
    return requireOracle ? 1 : 0;
  }

  const gatewayPort = await pickFreePort(PORT_SEARCH_START);
  const gatewayUpstreamPort = await pickFreePort(gatewayPort + 20);
  const gatewayTlsEchoPort = await pickFreePort(gatewayUpstreamPort + 1);
  const gatewayTlsEcho2Port = await pickFreePort(gatewayTlsEchoPort + 1);
  const gatewayHalfClosePort = await pickFreePort(gatewayTlsEcho2Port + 1);
  const oraclePort = await pickFreePort(gatewayHalfClosePort + 10);
  let certificates;
  try {
    certificates = generateStreamCertificates(GATEWAY_CONF_DIR);
  } catch (error) {
    log(`SKIP: cannot mint the stream TLS certificates: ${error.message}`);
    return requireOracle ? 1 : 0;
  }

  let baseline = fs.existsSync(BASELINE);
  if (refreshOracle || !baseline) {
    log(refreshOracle ? 'refreshing the nginx oracle baseline…' : 'no committed baseline; capturing one…');
    const oracle = captureOracle(oraclePort);
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

  for (const [label, port] of [
    ['gateway listener', gatewayPort],
    ['gateway echo upstream', gatewayUpstreamPort],
  ]) {
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} (${label}) is already listening; refusing to measure it`);
      return 1;
    }
  }

  const binary = findBinary();
  if (!binary) {
    log(`FAIL: no ${BINARY_STEM} binary under target/{debug,release}; build it first`);
    return 1;
  }
  if (staleness(binary.path, root).stale) {
    log(`FAIL: ${stalenessMessage(binary.path, root, 'node tools/run-nginx-differential-stream.mjs')}`);
    return 1;
  }

  fs.mkdirSync(GATEWAY_CONF_DIR, { recursive: true });
  try {
    fs.writeFileSync(
      GATEWAY_CONF,
      renderConf(
        gatewayPort,
        gatewayUpstreamPort,
        gatewayTlsEchoPort,
        gatewayTlsEcho2Port,
        gatewayHalfClosePort,
        certificates,
      ),
      'utf8',
    );
  } catch (error) {
    log(`FAIL: ${error.message}`);
    return 1;
  }

  const logPath = path.join(root, 'tmp', 'nginx-differential-stream-gateway.log');
  const logHandle = fs.openSync(logPath, 'w');
  log(
    `gateway listeners ${gatewayPort}..${gatewayPort + 11}, echo upstream ${gatewayUpstreamPort}, tls echo ${gatewayTlsEchoPort}/${gatewayTlsEcho2Port}, half-close echo ${gatewayHalfClosePort}`,
  );
  const gateway = spawn(binary.path, ['serve-nginx', GATEWAY_CONF], {
    cwd: root,
    stdio: ['ignore', logHandle, logHandle],
    env: { ...process.env, TZ: 'UTC' },
  });

  let exitCode = 1;
  try {
    if (!(await waitForPort(gatewayPort, 20_000))) {
      log(`FAIL: the gateway never listened on ${gatewayPort}; see ${path.relative(root, logPath)}`);
      return 1;
    }
    if (gateway.exitCode !== null) {
      log(`FAIL: the gateway exited with ${gateway.exitCode} instead of serving`);
      return 1;
    }
    const battery = runBattery(
      python,
      gatewayPort,
      gatewayUpstreamPort,
      gatewayTlsEchoPort,
      gatewayTlsEcho2Port,
      gatewayHalfClosePort,
      certificates,
    );
    if (!battery.ok) {
      log('FAIL: the battery could not capture the gateway');
      return 1;
    }
    if (battery.cases < CASE_FLOOR) {
      log(`FAIL: the battery reported ${battery.cases} cases, below the floor of ${CASE_FLOOR}`);
      return 1;
    }
    try {
      JSON.parse(fs.readFileSync(OUR_CAPTURE, 'utf8'));
    } catch (error) {
      log(`FAIL: cannot read the gateway capture: ${error.message}`);
      return 1;
    }
    const matched = runCompare(python);
    log(
      matched
        ? `PASS: ${battery.cases} differential stream cases match real nginx (${REFERENCE_NGINX})`
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
