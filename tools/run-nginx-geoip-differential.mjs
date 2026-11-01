#!/usr/bin/env node
/**
 * GeoIP variable-plumbing proxy differential.
 *
 * The pinned nginx 1.29.6 oracle (`/tmp/nginx-deb/root/usr/sbin/nginx` or
 * `$NGINX_BIN`) is **not** built with `--with-http_geoip_module`, so a real
 * GeoIP lookup differential is impossible and is not faked here. Instead:
 *
 * - the oracle fixture (`nginx.conf`) fixes the sixteen `$geoip_*` variables
 *   with `set` and exercises them through `return` bodies, a `map` result and
 *   `add_header` — nginx's own variable-expansion contract;
 * - the gateway fixture (`gateway.conf`) declares the real nginx GeoIP
 *   directives against the synthetic MMDB fixtures under `tests/nginx/geoip`
 *   and exercises the same template;
 * - the same case matrix (full US record, full DE record, empty record,
 *   recursive trusted-hop walk) must produce byte-identical bodies and
 *   headers on both sides.
 *
 * What is proven: the GeoIP variables are wired into the runtime's
 * `map` / `return` / `add_header` vocabulary with nginx-compatible empty-string
 * rendering, and `geoip_proxy`/`geoip_proxy_recursive` select the same address
 * the fixture values describe.
 *
 * What is NOT proven here: the MMDB field-to-variable mapping itself against a
 * real nginx GeoIP build (unavailable); that is covered by the Rust unit and
 * integration tests and recorded in the wave report.
 *
 * Exit codes: 0 comparison passed, 1 failure, 3 oracle unavailable (unless
 * SDKWORK_REQUIRE_NGINX_EVIDENCE=1, which turns the skip into a failure).
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  CASES,
  capture,
  captureStream,
} from '../tests/nginx/geoip-differential/probe.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sliceDir = path.join(root, 'tests', 'nginx', 'geoip-differential');
const fixtureDir = path.join(root, 'tests', 'nginx', 'geoip');
const workDir = path.join(root, 'tmp', 'nginx-geoip-differential');
const ORACLE = path.join(sliceDir, 'nginx-oracle.json');
const REFERENCE_NGINX = 'nginx/1.29.6';
const BINARY_STEM = 'sdkwork-api-webserver-standalone-gateway';
const ORACLE_PORT = 21096;
const GATEWAY_PORT = 21097;
const STREAM_ORACLE_PORT = 21098;
const STREAM_GATEWAY_PORT = 21099;

const log = (message) => process.stderr.write(`[nginx-geoip-differential] ${message}\n`);

const requireEvidence = process.env.SDKWORK_REQUIRE_NGINX_EVIDENCE === '1';

function findGatewayBinary() {
  const candidates = [];
  if (process.env.SDKWORK_WEBSERVER_GATEWAY_BIN) {
    candidates.push(process.env.SDKWORK_WEBSERVER_GATEWAY_BIN);
  }
  for (const targetRoot of [
    path.join(root, 'target'),
    process.env.CARGO_TARGET_DIR ? path.resolve(root, process.env.CARGO_TARGET_DIR) : null,
  ].filter(Boolean)) {
    for (const profile of ['debug', 'release']) {
      for (const suffix of ['.exe', '']) {
        candidates.push(path.join(targetRoot, profile, `${BINARY_STEM}${suffix}`));
      }
    }
  }
  let best = null;
  for (const candidate of candidates) {
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
  return best?.path ?? null;
}

function findOracleBinary() {
  const candidates = [];
  if (process.env.NGINX_BIN) candidates.push(process.env.NGINX_BIN);
  candidates.push('/tmp/nginx-deb/root/usr/sbin/nginx');
  const which = spawnSync('which', ['nginx'], { encoding: 'utf8' });
  if (which.status === 0) candidates.push(which.stdout.trim());
  for (const candidate of candidates) {
    if (candidate && fs.existsSync(candidate)) {
      const probe = spawnSync(candidate, ['-v'], { encoding: 'utf8' });
      if (probe.status === 0) return candidate;
    }
  }
  return null;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
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
    await delay(100);
  }
  return false;
}

function render(templatePath, replacements) {
  let text = fs.readFileSync(templatePath, 'utf8');
  for (const [placeholder, value] of Object.entries(replacements)) {
    text = text.replaceAll(placeholder, value);
  }
  const leftover = text.match(/@[A-Z_]+@/u);
  if (leftover) throw new Error(`unrendered placeholder ${leftover[0]} in ${templatePath}`);
  return text;
}

function writeConfig(name, text) {
  const dir = path.join(workDir, name);
  fs.mkdirSync(path.join(dir, 'logs'), { recursive: true });
  const file = path.join(dir, 'nginx.conf');
  fs.writeFileSync(file, text);
  return { dir, file };
}

async function captureSide(label, port, start, stop, captureFunction = capture) {
  try {
    start();
  } catch (error) {
    throw new Error(`${label} failed to start: ${error.message}`);
  }
  if (!(await waitForPort(port, 20_000))) {
    stop();
    throw new Error(`${label} never listened on ${port}`);
  }
  try {
    return await captureFunction(port);
  } finally {
    stop();
  }
}

function compare(expected, actual) {
  const problems = [];
  for (const testCase of CASES) {
    const oracle = expected[testCase.id];
    const gateway = actual[testCase.id];
    if (!oracle || !gateway) {
      problems.push(`${testCase.id}: missing capture`);
      continue;
    }
    if (oracle.status !== gateway.status) {
      problems.push(`${testCase.id}: status oracle=${oracle.status} gateway=${gateway.status}`);
    }
    if (oracle.body !== gateway.body) {
      problems.push(
        `${testCase.id}: body differs\n  oracle:  ${JSON.stringify(oracle.body)}\n  gateway: ${JSON.stringify(gateway.body)}`,
      );
    }
    for (const name of Object.keys(oracle.headers)) {
      if (oracle.headers[name] !== gateway.headers[name]) {
        problems.push(
          `${testCase.id}: header ${name} oracle=${JSON.stringify(oracle.headers[name])} gateway=${JSON.stringify(gateway.headers[name])}`,
        );
      }
    }
  }
  return problems;
}

function loadOracleCapture() {
  if (!fs.existsSync(ORACLE)) return null;
  let capture;
  try {
    capture = JSON.parse(fs.readFileSync(ORACLE, 'utf8'));
  } catch (error) {
    throw new Error(`committed oracle ${ORACLE} is not valid JSON: ${error.message}`);
  }
  if (typeof capture._server !== 'string' || !capture._server.startsWith('nginx/')) {
    throw new Error(`committed oracle is not a real nginx capture (_server=${capture._server})`);
  }
  if (capture._server !== REFERENCE_NGINX) {
    throw new Error(
      `committed oracle was captured from ${capture._server}, but this gate pins ${REFERENCE_NGINX}`,
    );
  }
  if (!capture.http || !capture.stream) {
    throw new Error('committed oracle is missing the http/stream capture groups');
  }
  return capture;
}

function oracleVersion(binary) {
  const probe = spawnSync(binary, ['-v'], { encoding: 'utf8' });
  const text = `${probe.stderr ?? ''}${probe.stdout ?? ''}`;
  const match = /nginx\/(\d+\.\d+\.\d+)/u.exec(text);
  return match ? `nginx/${match[1]}` : null;
}

async function captureOracleLive(oracleBinary, oracleConfig, streamOracleConfig) {
  for (const config of [oracleConfig, streamOracleConfig]) {
    const check = spawnSync(oracleBinary, ['-p', config.dir, '-c', config.file, '-t'], {
      encoding: 'utf8',
    });
    if (check.status !== 0) {
      throw new Error(
        `the oracle rejected the rendered fixture: ${check.stderr || check.stdout}`,
      );
    }
  }
  let oracleProcess = null;
  const http = await captureSide(
    'oracle nginx',
    ORACLE_PORT,
    () => {
      oracleProcess = spawn(oracleBinary, ['-p', oracleConfig.dir, '-c', oracleConfig.file], {
        cwd: root,
        stdio: ['ignore', 'ignore', 'inherit'],
      });
      oracleProcess.on('error', (error) => log(`oracle process error: ${error.message}`));
    },
    () => {
      if (oracleProcess) {
        spawnSync(oracleBinary, ['-p', oracleConfig.dir, '-c', oracleConfig.file, '-s', 'stop']);
        oracleProcess.kill('SIGTERM');
      }
    },
  );
  let streamOracleProcess = null;
  const stream = await captureSide(
    'oracle stream nginx',
    STREAM_ORACLE_PORT,
    () => {
      streamOracleProcess = spawn(
        oracleBinary,
        ['-p', streamOracleConfig.dir, '-c', streamOracleConfig.file],
        { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] },
      );
      streamOracleProcess.on('error', (error) =>
        log(`stream oracle process error: ${error.message}`),
      );
    },
    () => {
      if (streamOracleProcess) {
        spawnSync(oracleBinary, [
          '-p',
          streamOracleConfig.dir,
          '-c',
          streamOracleConfig.file,
          '-s',
          'stop',
        ]);
        streamOracleProcess.kill('SIGTERM');
      }
    },
    captureStream,
  );
  const version = oracleVersion(oracleBinary);
  if (version !== REFERENCE_NGINX) {
    throw new Error(`oracle binary reports ${version}, but this gate pins ${REFERENCE_NGINX}`);
  }
  return { _server: version, _cases: CASES.length + 1, http, stream };
}

async function main() {
  const refreshOracle = process.argv.includes('--refresh-oracle');
  const oracleBinary = findOracleBinary();
  const gatewayBinary = findGatewayBinary();
  if (!gatewayBinary) {
    log('FAIL: no gateway binary found; run cargo build -p sdkwork-api-webserver-standalone-gateway --bins');
    return 1;
  }
  for (const port of [ORACLE_PORT, GATEWAY_PORT, STREAM_ORACLE_PORT, STREAM_GATEWAY_PORT]) {
    if (!(await portFree(port))) {
      log(`FAIL: 127.0.0.1:${port} is already in use`);
      return 1;
    }
  }
  fs.mkdirSync(workDir, { recursive: true });

  const countryDb = path.join(fixtureDir, 'GeoIP2-Country-Test.mmdb');
  const cityDb = path.join(fixtureDir, 'GeoIP2-City-Test.mmdb');
  const orgDb = path.join(fixtureDir, 'GeoIP2-ASN-Test.mmdb');
  for (const database of [countryDb, cityDb, orgDb]) {
    if (!fs.existsSync(database)) {
      log(`FAIL: missing MMDB fixture ${path.relative(root, database)}`);
      return 1;
    }
  }
  const gatewayConfig = writeConfig(
    'gateway',
    render(path.join(sliceDir, 'gateway.conf'), {
      '@PORT@': String(GATEWAY_PORT),
      '@COUNTRY_DB@': countryDb,
      '@CITY_DB@': cityDb,
      '@ORG_DB@': orgDb,
    }),
  );
  const streamGatewayConfig = writeConfig(
    'gateway-stream',
    render(path.join(sliceDir, 'stream-gateway.conf'), {
      '@STREAM_PORT@': String(STREAM_GATEWAY_PORT),
      '@COUNTRY_DB@': countryDb,
      '@CITY_DB@': cityDb,
      '@ORG_DB@': orgDb,
    }),
  );

  let oracleCapture = refreshOracle ? null : loadOracleCapture();
  if (oracleCapture === null) {
    if (!oracleBinary) {
      const message =
        'no committed GeoIP oracle baseline exists and no real nginx binary is available';
      if (requireEvidence || refreshOracle) {
        log(`FAIL: ${message}`);
        return 1;
      }
      log(`SKIP: ${message}`);
      return 3;
    }
    const oracleConfig = writeConfig(
      'oracle',
      render(path.join(sliceDir, 'nginx.conf'), { '@PORT@': String(ORACLE_PORT) }),
    );
    const streamOracleConfig = writeConfig(
      'oracle-stream',
      render(path.join(sliceDir, 'stream-nginx.conf'), {
        '@STREAM_PORT@': String(STREAM_ORACLE_PORT),
      }),
    );
    try {
      oracleCapture = await captureOracleLive(oracleBinary, oracleConfig, streamOracleConfig);
    } catch (error) {
      log(`FAIL: ${error.message}`);
      return 1;
    }
    fs.writeFileSync(ORACLE, `${JSON.stringify(oracleCapture, null, 2)}\n`);
    log(`oracle baseline written to ${path.relative(root, ORACLE)}`);
  }

  const gatewayLog = fs.openSync(path.join(workDir, 'gateway.log'), 'w');
  let gatewayProcess = null;
  const capturedGateway = await captureSide(
    'gateway',
    GATEWAY_PORT,
    () => {
      gatewayProcess = spawn(gatewayBinary, ['serve-nginx', gatewayConfig.file], {
        cwd: root,
        stdio: ['ignore', gatewayLog, gatewayLog],
        env: { ...process.env, TZ: 'UTC' },
      });
      gatewayProcess.on('error', (error) => log(`gateway process error: ${error.message}`));
    },
    () => {
      if (gatewayProcess) {
        gatewayProcess.kill('SIGTERM');
        gatewayProcess = null;
      }
    },
  );
  const streamGatewayLog = fs.openSync(path.join(workDir, 'gateway-stream.log'), 'w');
  let streamGatewayProcess = null;
  const capturedStreamGateway = await captureSide(
    'gateway stream',
    STREAM_GATEWAY_PORT,
    () => {
      streamGatewayProcess = spawn(gatewayBinary, ['serve-nginx', streamGatewayConfig.file], {
        cwd: root,
        stdio: ['ignore', streamGatewayLog, streamGatewayLog],
        env: { ...process.env, TZ: 'UTC' },
      });
      streamGatewayProcess.on('error', (error) =>
        log(`stream gateway process error: ${error.message}`),
      );
    },
    () => {
      if (streamGatewayProcess) {
        streamGatewayProcess.kill('SIGTERM');
        streamGatewayProcess = null;
      }
    },
    captureStream,
  );

  const problems = compare(oracleCapture.http, capturedGateway);
  if (oracleCapture.stream.body !== capturedStreamGateway.body) {
    problems.push(
      `stream: body differs\n  oracle:  ${JSON.stringify(oracleCapture.stream.body)}\n  gateway: ${JSON.stringify(capturedStreamGateway.body)}`,
    );
  }
  if (problems.length > 0) {
    log(`FAIL: ${problems.length} difference(s):`);
    for (const problem of problems) log(`  - ${problem}`);
    log(`gateway log: ${path.relative(root, path.join(workDir, 'gateway.log'))}`);
    return 1;
  }
  log(
    `PASS: ${CASES.length} HTTP cases and 1 stream case byte-identical between real nginx ` +
      `${oracleCapture._server} (fixed values) and the gateway (MMDB lookup)`,
  );
  log(
    'NOTE: the oracle lacks --with-http_geoip_module/--with-stream_geoip_module; this is a ' +
      'variable-plumbing proxy differential, not a GeoIP lookup differential.',
  );
  return 0;
}

main()
  .then((code) => process.exit(code))
  .catch((error) => {
    log(`FAIL: ${error.stack ?? error.message}`);
    process.exit(1);
  });
