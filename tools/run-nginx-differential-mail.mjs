#!/usr/bin/env node
/**
 * Differential gate: the nginx-compatible mail proxy plane (Wave 9B) against a
 * real nginx 1.29.6 oracle.
 *
 * The same `mail {}` fixture is rendered twice with different listener, auth
 * service and backend ports: once for real nginx (built `--with-mail
 * --with-mail_ssl_module`) and once for the gateway (`serve-nginx`). The
 * battery drives scripted POP3/IMAP/SMTP clients, the deterministic mock
 * auth_http service and the mock backends, then compares:
 *
 * - the client-visible byte transcript (TLS cases compare the decrypted
 *   application bytes; the handshake itself is not the proxy contract);
 * - every line the mock backend received (backend authentication, XCLIENT,
 *   client HELO replay, relayed commands);
 * - every header of the auth_http request the proxy issued.
 *
 * The committed baseline (`nginx-oracle.json`) is a real nginx capture. The
 * gate pins nginx/1.29.6, enforces a case floor, and refuses to measure a busy
 * port. `--refresh-oracle` re-captures the baseline from the pinned build.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { staleness, stalenessMessage } from './gateway-binary-freshness.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'tests', 'nginx', 'differential-mail');
const BASELINE = path.join(fixtureDir, 'nginx-oracle.json');
const RENDERED = path.join(fixtureDir, 'nginx.rendered.conf');
const GATEWAY_CONF_DIR = path.join(root, 'tmp', 'nginx-differential-mail');
const GATEWAY_CONF = path.join(GATEWAY_CONF_DIR, 'nginx.conf');
const OUR_CAPTURE = path.join(root, 'tmp', 'nginx-differential-mail-ours.json');
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const REFERENCE_NGINX = 'nginx/1.29.6';
const CASE_FLOOR = 9;
const ORACLE_UNAVAILABLE_EXITS = new Set([3, 4, 5]);

// Oracle side ports (proxy, auth service, mock backends).
const ORACLE = { proxy: 21490, auth: 21500, pop3: 21501, imap: 21502, smtp: 21503 };
// Gateway side ports, offset so the two sides never share a socket.
const GATEWAY = { proxy: 21480, auth: 21510, pop3: 21511, imap: 21512, smtp: 21513 };

const log = (message) => process.stderr.write(`[nginx-differential-mail] ${message}\n`);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function resolvePython() {
  for (const candidate of ['python3', 'python']) {
    const result = spawnSync(candidate, ['--version'], { encoding: 'utf8' });
    if (result.status === 0) return candidate;
  }
  return null;
}

function posixRoot(value) {
  return value.split(path.sep).join('/');
}

function renderConfig(ports, target = RENDERED) {
  const template = fs.readFileSync(path.join(fixtureDir, 'nginx.conf'), 'utf8');
  const certificate = posixRoot(path.join(root, 'tests', 'nginx', 'full-surface', 'cert-a.pem'));
  const key = posixRoot(path.join(root, 'tests', 'nginx', 'full-surface', 'key-a.pem'));
  const rendered = template
    .replaceAll('@AUTH@', String(ports.auth))
    .replaceAll('@POP3@', String(ports.proxy))
    .replaceAll('@IMAP@', String(ports.proxy + 1))
    .replaceAll('@SMTP@', String(ports.proxy + 2))
    .replaceAll('@SMTP_TLS@', String(ports.proxy + 3))
    .replaceAll('@SMTP_STARTTLS@', String(ports.proxy + 4))
    .replaceAll('@CERT@', certificate)
    .replaceAll('@KEY@', key);
  for (const placeholder of ['@AUTH@', '@POP3@', '@IMAP@', '@SMTP@', '@SMTP_TLS@', '@SMTP_STARTTLS@', '@CERT@', '@KEY@']) {
    if (rendered.includes(placeholder)) {
      throw new Error(`mail fixture placeholder ${placeholder} was left unrendered`);
    }
  }
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, rendered);
}

function captureOracle() {
  const script = path.join(fixtureDir, 'run-nginx.sh');
  const result = spawnSync('bash', [script, BASELINE], { cwd: root, stdio: 'inherit' });
  if (result.error) return { status: 'unavailable', reason: result.error.message };
  if (ORACLE_UNAVAILABLE_EXITS.has(result.status)) return { status: 'unavailable' };
  if (result.status !== 0) return { status: 'failed', status_code: result.status };
  return { status: 'captured' };
}

function runBattery(python, ports) {
  const result = spawnSync(
    python,
    [
      path.join(fixtureDir, 'battery.py'),
      '--pop3', String(ports.proxy),
      '--imap', String(ports.proxy + 1),
      '--smtp', String(ports.proxy + 2),
      '--smtp-tls', String(ports.proxy + 3),
      '--smtp-starttls', String(ports.proxy + 4),
      '--auth', String(ports.auth),
      '--backend-pop3', String(ports.pop3),
      '--backend-imap', String(ports.imap),
      '--backend-smtp', String(ports.smtp),
      '--server-identity', 'gateway',
      '--out', OUR_CAPTURE,
    ],
    { cwd: root, encoding: 'utf8' },
  );
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  process.stderr.write(output);
  if (result.status !== 0) return { ok: false, cases: 0 };
  const match = output.match(/\((\d+) cases\)/);
  return { ok: true, cases: match ? Number(match[1]) : 0 };
}

function runCompare(python) {
  const result = spawnSync(
    python,
    [path.join(fixtureDir, 'compare.py'), '--expected', BASELINE, '--actual', OUR_CAPTURE],
    { cwd: root, stdio: 'inherit' },
  );
  return result.status === 0;
}

function portFree(port) {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: '127.0.0.1', port });
    socket.setTimeout(500);
    socket.once('connect', () => {
      socket.destroy();
      resolve(false);
    });
    socket.once('timeout', () => {
      socket.destroy();
      resolve(true);
    });
    socket.once('error', () => resolve(true));
  });
}

async function waitForPort(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (!(await portFree(port))) return true;
    await delay(100);
  }
  return false;
}

function findBinary() {
  const explicit = process.env.SDKWORK_WEBSERVER_GATEWAY_BIN;
  if (explicit && fs.existsSync(explicit)) return explicit;
  const candidates = [
    path.join(root, 'target', 'debug', BINARY_STEM),
    path.join(root, 'target', 'debug', `${BINARY_STEM}.exe`),
  ];
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
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
    log(refreshOracle ? 'refreshing the nginx oracle baseline…' : 'no committed baseline; capturing one…');
    const oracle = captureOracle();
    if (oracle.status === 'captured') {
      baseline = true;
      log(`oracle baseline written to ${path.relative(root, BASELINE)}`);
    } else {
      log(`oracle ${oracle.status}: ${oracle.reason ?? 'no real nginx mail build available here'}`);
      if (!baseline) {
        if (requireOracle) {
          log('FAIL: --require-oracle was requested but no oracle produced a baseline');
          return 1;
        }
        log('SKIP: no baseline to compare against; the mail differential slice did NOT run');
        return 0;
      }
      log('falling back to the committed baseline');
    }
  }
  const recorded = JSON.parse(fs.readFileSync(BASELINE, 'utf8'));
  if (String(recorded._server ?? '') !== REFERENCE_NGINX) {
    log(`FAIL: baseline was captured from ${JSON.stringify(recorded._server)}, but this gate pins ${REFERENCE_NGINX}`);
    return 1;
  }
  const recordedCases = Object.keys(recorded).filter((key) => !key.startsWith('_')).length;
  if (recordedCases < CASE_FLOOR) {
    log(`FAIL: the committed baseline holds ${recordedCases} cases, below the floor of ${CASE_FLOOR}`);
    return 1;
  }
  log(`baseline identity: server=${recorded._server} cases=${recordedCases}`);

  for (const [label, port] of [
    ['gateway proxy', GATEWAY.proxy],
    ['mock auth_http', GATEWAY.auth],
    ['mock POP3 backend', GATEWAY.pop3],
    ['mock IMAP backend', GATEWAY.imap],
    ['mock SMTP backend', GATEWAY.smtp],
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
    log(`FAIL: ${stalenessMessage(binary, root, 'node tools/run-nginx-differential-mail.mjs')}`);
    return 1;
  }

  renderConfig(GATEWAY, GATEWAY_CONF);
  const logPath = path.join(root, 'tmp', 'nginx-differential-mail-gateway.log');
  fs.mkdirSync(path.dirname(logPath), { recursive: true });
  const logHandle = fs.openSync(logPath, 'w');
  const gateway = spawn(binary, ['serve-nginx', GATEWAY_CONF], {
    cwd: root,
    stdio: ['ignore', logHandle, logHandle],
  });
  let exitCode = 1;
  try {
    if (!(await waitForPort(GATEWAY.proxy, 20_000))) {
      log(`FAIL: the gateway never listened on ${GATEWAY.proxy}; see ${path.relative(root, logPath)}`);
      return 1;
    }
    const battery = runBattery(python, GATEWAY);
    if (!battery.ok) {
      log('FAIL: the battery could not capture the gateway');
      return 1;
    }
    if (battery.cases < CASE_FLOOR) {
      log(`FAIL: the battery reported ${battery.cases} cases, below the floor of ${CASE_FLOOR}`);
      return 1;
    }
    const matched = runCompare(python);
    log(
      matched
        ? `PASS: ${battery.cases} mail differential cases match real nginx`
        : 'FAIL: unexpected mail differential differences (see above)',
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
