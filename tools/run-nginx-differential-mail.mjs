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
const CAPS_DIR = path.join(fixtureDir, 'capabilities');
const CAPS_BASELINE = path.join(CAPS_DIR, 'nginx-oracle.json');
const CAPS_GATEWAY_CONF = path.join(GATEWAY_CONF_DIR, 'capabilities.conf');
const CAPS_OUR_CAPTURE = path.join(root, 'tmp', 'nginx-differential-mail-capabilities-ours.json');
const CLIENT_CERT_DIR = path.join(fixtureDir, 'client-cert');
const CLIENT_CERT_BASELINE = path.join(CLIENT_CERT_DIR, 'nginx-oracle.json');
const CLIENT_CERT_GATEWAY_CONF = path.join(GATEWAY_CONF_DIR, 'client-cert.conf');
const CLIENT_CERT_OUR_CAPTURE = path.join(
  root,
  'tmp',
  'nginx-differential-mail-client-cert-ours.json',
);
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const REFERENCE_NGINX = 'nginx/1.29.6';
const CASE_FLOOR = 9;
const CAPS_CASE_FLOOR = 8;
const CLIENT_CERT_CASE_FLOOR = 19;
const ORACLE_UNAVAILABLE_EXITS = new Set([3, 4, 5]);

// Oracle side ports (proxy, auth service, mock backends).
const ORACLE = { proxy: 21490, auth: 21500, pop3: 21501, imap: 21502, smtp: 21503 };
// Gateway side ports, offset so the two sides never share a socket.
const GATEWAY = { proxy: 21480, auth: 21510, pop3: 21511, imap: 21512, smtp: 21513 };
// Capability/realip slice ports (plaintext-only fixture the oracle can run).
const CAPS_GATEWAY = { base: 21520, auth: 21540, backend: 21541 };
const CAPS_ORACLE = { base: 21560, auth: 21580, backend: 21581 };
// Client-certificate slice: listener offsets match the fixture placeholders.
const CLIENT_CERT_GATEWAY = { base: 21760, auth: 21780, backend: 21781 };
const CLIENT_CERT_ORACLE = { base: 21820, auth: 21840, backend: 21841 };

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

const CAPS_PLACEHOLDERS = [
  '@SMTP_DEFAULT@',
  '@SMTP_CUSTOM@',
  '@IMAP_DEFAULT@',
  '@IMAP_CUSTOM@',
  '@POP3_DEFAULT@',
  '@POP3_CUSTOM@',
  '@SMTP_PROXY_TRUSTED@',
  '@SMTP_PROXY_UNTRUSTED@',
];

/** Render the plaintext capability/realip fixture (oracle-runnable). */
function renderCapabilitiesConfig(base, target) {
  const template = fs.readFileSync(path.join(CAPS_DIR, 'nginx.conf'), 'utf8');
  let rendered = template;
  CAPS_PLACEHOLDERS.forEach((placeholder, index) => {
    rendered = rendered.replaceAll(placeholder, String(base + index));
  });
  rendered = rendered
    .replaceAll('@AUTH@', String(base + 20))
    .replaceAll('@BACKEND@', String(base + 21));
  for (const placeholder of [...CAPS_PLACEHOLDERS, '@AUTH@', '@BACKEND@']) {
    if (rendered.includes(placeholder)) {
      throw new Error(`mail capability fixture placeholder ${placeholder} was left unrendered`);
    }
  }
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, rendered);
}

function captureCapabilitiesOracle(base) {
  const result = spawnSync('bash', [path.join(CAPS_DIR, 'run-nginx.sh')], {
    cwd: root,
    encoding: 'utf8',
    env: {
      ...process.env,
      NGINX_DIFF_MAIL_CAPS_PORT: String(base),
      NGINX_DIFF_MAIL_CAPS_WORK: path.join(root, 'tmp', 'nginx-differential-mail-caps-oracle'),
      NGINX_DIFF_MAIL_CAPS_ORACLE_OUT: CAPS_BASELINE,
    },
  });
  if (result.error) return { status: 'unavailable', reason: result.error.message };
  if (ORACLE_UNAVAILABLE_EXITS.has(result.status)) {
    return { status: 'unavailable', reason: (result.stderr ?? '').trim().split('\n').at(-1) };
  }
  if (result.status !== 0) return { status: 'failed', code: result.status };
  return { status: 'captured' };
}

function runCapabilitiesBattery(python, base, outPath) {
  const result = spawnSync(
    python,
    [
      path.join(CAPS_DIR, 'battery.py'),
      '--smtp-default', String(base),
      '--smtp-custom', String(base + 1),
      '--imap-default', String(base + 2),
      '--imap-custom', String(base + 3),
      '--pop3-default', String(base + 4),
      '--pop3-custom', String(base + 5),
      '--smtp-proxy-trusted', String(base + 6),
      '--smtp-proxy-untrusted', String(base + 7),
      '--auth-port', String(base + 20),
      '--backend-port', String(base + 21),
      '--out', outPath,
    ],
    { cwd: root, encoding: 'utf8' },
  );
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  process.stderr.write(output);
  if (result.status !== 0) return { ok: false, cases: 0 };
  const match = output.match(/\((\d+) mail cases\)/);
  return { ok: true, cases: match ? Number(match[1]) : 0 };
}

const CLIENT_CERT_PLACEHOLDERS = [
  '@SMTP_ON@',
  '@SMTP_ON_PASS_OFF@',
  '@SMTP_TRUSTED_ONLY@',
  '@SMTP_OPTIONAL@',
  '@SMTP_NO_CA@',
  '@SMTP_DEPTH0@',
  '@SMTP_DEPTH1@',
  '@SMTP_DEPTH2@',
  '@SMTP_TRUSTED_INTERMEDIATE@',
  '@SMTP_STARTTLS@',
  '@IMAP_ON@',
  '@POP3_ON@',
  '@SMTP_VERIFY_OFF@',
];

/** Render the mail client-certificate fixture. */
function renderClientCertConfig(base, target) {
  const template = fs.readFileSync(path.join(CLIENT_CERT_DIR, 'nginx.conf'), 'utf8');
  const certificate = posixRoot(path.join(root, 'tests', 'nginx', 'full-surface', 'cert-a.pem'));
  const key = posixRoot(path.join(root, 'tests', 'nginx', 'full-surface', 'key-a.pem'));
  let rendered = template;
  CLIENT_CERT_PLACEHOLDERS.forEach((placeholder, index) => {
    rendered = rendered.replaceAll(placeholder, String(base + index));
  });
  rendered = rendered
    .replaceAll('@AUTH@', String(base + 20))
    .replaceAll('@BACKEND@', String(base + 21))
    .replaceAll('@SERVER_CERT@', certificate)
    .replaceAll('@SERVER_KEY@', key)
    .replaceAll('@CA@', posixRoot(path.join(CLIENT_CERT_DIR, 'ca.pem')))
    .replaceAll(
      '@INTERMEDIATE_CA@',
      posixRoot(path.join(CLIENT_CERT_DIR, 'intermediate-ca.pem')),
    );
  for (const placeholder of [
    ...CLIENT_CERT_PLACEHOLDERS,
    '@AUTH@',
    '@BACKEND@',
    '@SERVER_CERT@',
    '@SERVER_KEY@',
    '@CA@',
    '@INTERMEDIATE_CA@',
  ]) {
    if (rendered.includes(placeholder)) {
      throw new Error(`mail client-cert fixture placeholder ${placeholder} was left unrendered`);
    }
  }
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, rendered);
}

function captureClientCertOracle(base) {
  const result = spawnSync('bash', [path.join(CLIENT_CERT_DIR, 'run-nginx.sh')], {
    cwd: root,
    encoding: 'utf8',
    env: {
      ...process.env,
      NGINX_DIFF_MAIL_CLIENT_CERT_PORT: String(base),
      NGINX_DIFF_MAIL_CLIENT_CERT_WORK: path.join(
        root,
        'tmp',
        'nginx-differential-mail-client-cert-oracle',
      ),
      NGINX_DIFF_MAIL_CLIENT_CERT_ORACLE_OUT: CLIENT_CERT_BASELINE,
    },
  });
  if (result.error) return { status: 'unavailable', reason: result.error.message };
  if (ORACLE_UNAVAILABLE_EXITS.has(result.status)) {
    return { status: 'unavailable', reason: (result.stderr ?? '').trim().split('\n').at(-1) };
  }
  if (result.status !== 0) return { status: 'failed', code: result.status };
  return { status: 'captured' };
}

function runClientCertBattery(python, base, outPath) {
  const dir = CLIENT_CERT_DIR;
  const result = spawnSync(
    python,
    [
      path.join(dir, 'battery.py'),
      '--smtp-on', String(base),
      '--smtp-on-pass-off', String(base + 1),
      '--smtp-trusted-only', String(base + 2),
      '--smtp-optional', String(base + 3),
      '--smtp-no-ca', String(base + 4),
      '--smtp-depth0', String(base + 5),
      '--smtp-depth1', String(base + 6),
      '--smtp-depth2', String(base + 7),
      '--smtp-trusted-intermediate', String(base + 8),
      '--smtp-starttls', String(base + 9),
      '--imap-on', String(base + 10),
      '--pop3-on', String(base + 11),
      '--smtp-verify-off', String(base + 12),
      '--auth-port', String(base + 20),
      '--backend-port', String(base + 21),
      '--ca', path.join(dir, 'ca.pem'),
      '--client-trusted-cert', path.join(dir, 'client-trusted.pem'),
      '--client-trusted-key', path.join(dir, 'client-trusted.key'),
      '--client-untrusted-cert', path.join(dir, 'client-untrusted.pem'),
      '--client-untrusted-key', path.join(dir, 'client-untrusted.key'),
      '--client-selfsigned-cert', path.join(dir, 'client-selfsigned.pem'),
      '--client-selfsigned-key', path.join(dir, 'client-selfsigned.key'),
      '--client-intermediate-cert', path.join(dir, 'client-intermediate.pem'),
      '--client-intermediate-chain', path.join(dir, 'client-intermediate-chain.pem'),
      '--client-intermediate-key', path.join(dir, 'client-intermediate.key'),
      '--out', outPath,
    ],
    { cwd: root, encoding: 'utf8' },
  );
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  process.stderr.write(output);
  if (result.status !== 0) return { ok: false, cases: 0 };
  const match = output.match(/\((\d+) mail client-cert cases\)/);
  return { ok: true, cases: match ? Number(match[1]) : 0 };
}

async function main() {
  const refreshOracle = process.argv.includes('--refresh-oracle');
  // The aggregate release gate exports SDKWORK_REQUIRE_NGINX_EVIDENCE=1
  // instead of forwarding --require-oracle; both forms must fail closed.
  const requireOracle = process.argv.includes('--require-oracle')
    || process.env.SDKWORK_REQUIRE_NGINX_EVIDENCE === '1';
  const python = resolvePython();
  if (!python) {
    log(`${requireOracle ? 'FAIL' : 'SKIP'}: no python3/python on PATH; the battery cannot run`);
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
          log('FAIL: release evidence is required (--require-oracle or SDKWORK_REQUIRE_NGINX_EVIDENCE=1) but no oracle produced a baseline');
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
    log(`${requireOracle ? 'FAIL' : 'SKIP'}: no gateway binary found (build it, or set SDKWORK_WEBSERVER_GATEWAY_BIN)`);
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
  if (exitCode !== 0) return exitCode;

  // Mail residual slice: custom capability lists and `set_real_ip_from`. The
  // pinned oracle is built without mail_ssl, so this second fixture is
  // plaintext-only and can be captured from the oracle itself.
  for (const [label, port] of [
    ['capability gateway listener', CAPS_GATEWAY.base],
    ['capability mock auth_http', CAPS_GATEWAY.auth],
    ['capability mock SMTP backend', CAPS_GATEWAY.backend],
  ]) {
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} (${label}) is already listening; refusing to measure it`);
      return 1;
    }
  }
  let capsBaseline = fs.existsSync(CAPS_BASELINE);
  if (refreshOracle || !capsBaseline) {
    log(
      refreshOracle
        ? 'refreshing the nginx oracle baseline for the capability/realip fixture…'
        : 'no committed capability/realip baseline; capturing one…',
    );
    const oracle = captureCapabilitiesOracle(CAPS_ORACLE.base);
    if (oracle.status === 'captured') {
      capsBaseline = true;
      log(`oracle baseline written to ${path.relative(root, CAPS_BASELINE)}`);
    } else {
      log(`oracle ${oracle.status}: ${oracle.reason ?? 'no real nginx mail build available here'}`);
      if (!capsBaseline) {
        if (requireOracle) {
          log('FAIL: --require-oracle was requested but no capability/realip baseline was produced');
          return 1;
        }
        log('SKIP: no capability/realip baseline to compare against; that slice did NOT run');
        return 0;
      }
      log('falling back to the committed capability/realip baseline');
    }
  }
  const capsRecorded = JSON.parse(fs.readFileSync(CAPS_BASELINE, 'utf8'));
  const capsRecordedCases = Object.keys(capsRecorded).filter((key) => !key.startsWith('_')).length;
  if (capsRecordedCases < CAPS_CASE_FLOOR) {
    log(
      `FAIL: the committed capability baseline holds ${capsRecordedCases} cases, below the floor of ${CAPS_CASE_FLOOR}`,
    );
    return 1;
  }
  try {
    renderCapabilitiesConfig(CAPS_GATEWAY.base, CAPS_GATEWAY_CONF);
  } catch (error) {
    log(`FAIL: ${error.message}`);
    return 1;
  }
  const capsLogPath = path.join(root, 'tmp', 'nginx-differential-mail-capabilities-gateway.log');
  const capsLogHandle = fs.openSync(capsLogPath, 'w');
  log(`capability gateway listeners ${CAPS_GATEWAY.base}..${CAPS_GATEWAY.base + 7}`);
  const capsGateway = spawn(binary, ['serve-nginx', CAPS_GATEWAY_CONF], {
    cwd: root,
    stdio: ['ignore', capsLogHandle, capsLogHandle],
  });
  let capsExit = 1;
  try {
    if (!(await waitForPort(CAPS_GATEWAY.base, 20_000))) {
      log(
        `FAIL: the gateway never listened on ${CAPS_GATEWAY.base}; see ${path.relative(root, capsLogPath)}`,
      );
    } else if (capsGateway.exitCode !== null) {
      log(`FAIL: the capability gateway exited with ${capsGateway.exitCode} instead of serving`);
    } else {
      const battery = runCapabilitiesBattery(python, CAPS_GATEWAY.base, CAPS_OUR_CAPTURE);
      if (!battery.ok) {
        log('FAIL: the capability/realip battery could not capture the gateway');
      } else if (battery.cases < CAPS_CASE_FLOOR) {
        log(
          `FAIL: the capability/realip battery reported ${battery.cases} cases, below the floor of ${CAPS_CASE_FLOOR}`,
        );
      } else {
        const matched = spawnSync(
          python,
          [path.join(fixtureDir, 'compare.py'), '--expected', CAPS_BASELINE, '--actual', CAPS_OUR_CAPTURE],
          { cwd: root, stdio: 'inherit' },
        ).status === 0;
        log(
          matched
            ? `PASS: ${battery.cases} capability/realip cases match real nginx`
            : 'FAIL: unexpected capability/realip differences (see above)',
        );
        capsExit = matched ? 0 : 1;
      }
    }
  } finally {
    if (capsGateway.exitCode === null) {
      capsGateway.kill();
      await Promise.race([
        new Promise((resolve) => capsGateway.once('exit', resolve)),
        delay(10_000).then(() => capsGateway.kill('SIGKILL')),
      ]);
    }
    fs.closeSync(capsLogHandle);
  }
  if (capsExit !== 0) return capsExit;

  // Mail client-certificate slice (Wave 14B): verify modes, depth, the two CA
  // bundles and the auth_http certificate headers. Requires the SSL-enabled
  // oracle build; the fixture launcher exits 3/4/5 when it is unavailable.
  for (let index = 0; index < 13; index += 1) {
    const port = CLIENT_CERT_GATEWAY.base + index;
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} (client-cert gateway listener) is already listening; refusing to measure it`);
      return 1;
    }
  }
  for (const [label, port] of [
    ['client-cert mock auth_http', CLIENT_CERT_GATEWAY.auth],
    ['client-cert mock backend', CLIENT_CERT_GATEWAY.backend],
  ]) {
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} (${label}) is already listening; refusing to measure it`);
      return 1;
    }
  }
  let clientCertBaseline = fs.existsSync(CLIENT_CERT_BASELINE);
  if (refreshOracle || !clientCertBaseline) {
    log(
      refreshOracle
        ? 'refreshing the nginx oracle baseline for the client-certificate fixture…'
        : 'no committed client-certificate baseline; capturing one…',
    );
    const oracle = captureClientCertOracle(CLIENT_CERT_ORACLE.base);
    if (oracle.status === 'captured') {
      clientCertBaseline = true;
      log(`oracle baseline written to ${path.relative(root, CLIENT_CERT_BASELINE)}`);
    } else {
      log(`oracle ${oracle.status}: ${oracle.reason ?? 'no real nginx mail_ssl build available here'}`);
      if (!clientCertBaseline) {
        if (requireOracle) {
          log('FAIL: --require-oracle was requested but no client-certificate baseline was produced');
          return 1;
        }
        log('SKIP: no client-certificate baseline to compare against; that slice did NOT run');
        return 0;
      }
      log('falling back to the committed client-certificate baseline');
    }
  }
  const clientCertRecorded = JSON.parse(fs.readFileSync(CLIENT_CERT_BASELINE, 'utf8'));
  const clientCertRecordedCases = Object.keys(clientCertRecorded).filter(
    (key) => !key.startsWith('_'),
  ).length;
  if (clientCertRecordedCases < CLIENT_CERT_CASE_FLOOR) {
    log(
      `FAIL: the committed client-certificate baseline holds ${clientCertRecordedCases} cases, below the floor of ${CLIENT_CERT_CASE_FLOOR}`,
    );
    return 1;
  }
  try {
    renderClientCertConfig(CLIENT_CERT_GATEWAY.base, CLIENT_CERT_GATEWAY_CONF);
  } catch (error) {
    log(`FAIL: ${error.message}`);
    return 1;
  }
  const clientCertLogPath = path.join(root, 'tmp', 'nginx-differential-mail-client-cert-gateway.log');
  const clientCertLogHandle = fs.openSync(clientCertLogPath, 'w');
  log(
    `client-certificate gateway listeners ${CLIENT_CERT_GATEWAY.base}..${CLIENT_CERT_GATEWAY.base + 12}`,
  );
  const clientCertGateway = spawn(binary, ['serve-nginx', CLIENT_CERT_GATEWAY_CONF], {
    cwd: root,
    stdio: ['ignore', clientCertLogHandle, clientCertLogHandle],
  });
  let clientCertExit = 1;
  try {
    if (!(await waitForPort(CLIENT_CERT_GATEWAY.base, 20_000))) {
      log(
        `FAIL: the gateway never listened on ${CLIENT_CERT_GATEWAY.base}; see ${path.relative(root, clientCertLogPath)}`,
      );
    } else if (clientCertGateway.exitCode !== null) {
      log(`FAIL: the client-certificate gateway exited with ${clientCertGateway.exitCode} instead of serving`);
    } else {
      const battery = runClientCertBattery(python, CLIENT_CERT_GATEWAY.base, CLIENT_CERT_OUR_CAPTURE);
      if (!battery.ok) {
        log('FAIL: the client-certificate battery could not capture the gateway');
      } else if (battery.cases < CLIENT_CERT_CASE_FLOOR) {
        log(
          `FAIL: the client-certificate battery reported ${battery.cases} cases, below the floor of ${CLIENT_CERT_CASE_FLOOR}`,
        );
      } else {
        const matched =
          spawnSync(
            python,
            [
              path.join(fixtureDir, 'compare.py'),
              '--expected',
              CLIENT_CERT_BASELINE,
              '--actual',
              CLIENT_CERT_OUR_CAPTURE,
            ],
            { cwd: root, stdio: 'inherit' },
          ).status === 0;
        log(
          matched
            ? `PASS: ${battery.cases} client-certificate cases match real nginx`
            : 'FAIL: unexpected client-certificate differences (see above)',
        );
        clientCertExit = matched ? 0 : 1;
      }
    }
  } finally {
    if (clientCertGateway.exitCode === null) {
      clientCertGateway.kill();
      await Promise.race([
        new Promise((resolve) => clientCertGateway.once('exit', resolve)),
        delay(10_000).then(() => clientCertGateway.kill('SIGKILL')),
      ]);
    }
    fs.closeSync(clientCertLogHandle);
  }
  return clientCertExit;
}

process.exit(await main());
