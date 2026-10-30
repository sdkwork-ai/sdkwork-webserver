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
const CASE_FLOOR = 46;
/**
 * nginx renders its own generated error page for a memcached miss/upstream
 * failure; the status mapping is the contract under test there, while the body
 * is the same documented deployment difference the broad HTTP slice allowlists.
 */
const STATUS_ONLY_CASES = new Set([
  'memcached-miss',
  'memcached-raw-502',
  'memcached-post',
  // nginx renders a generated 504/502 page for a gRPC read timeout and for an
  // unreachable/untrusted upstream; the status mapping is the contract, the
  // body is the documented generated-error-page difference.
  'grpc-read-timeout',
  'grpc-tls-untrusted',
  'grpc-dead',
]);
const ORACLE_PORT = 21092;
const GATEWAY_PORT = 21093;
/** Second listener pair for the typed-plane unix-socket transport cases. */
const ORACLE_UNIX_PORT = ORACLE_PORT + 30;
const GATEWAY_UNIX_PORT = GATEWAY_PORT + 30;
const GATEWAY_UNIX_ROOT = path.join(GATEWAY_CONF_DIR, 'unix');
const GATEWAY_FCGI_SOCKET = path.join(GATEWAY_UNIX_ROOT, 'fcgi.sock');
const GATEWAY_UWSGI_SOCKET = path.join(GATEWAY_UNIX_ROOT, 'uwsgi.sock');
const ORACLE_FCGI_PORT = ORACLE_PORT + 2;
const ORACLE_UWSGI_PORT = ORACLE_PORT + 10;
const ORACLE_SCGI_PORT = ORACLE_PORT + 11;
const ORACLE_MEMCACHED_PORT = ORACLE_PORT + 14;
const ORACLE_GRPC_PORT = ORACLE_PORT + 20;
const ORACLE_GRPC_TLS_OK_PORT = ORACLE_PORT + 21;
const ORACLE_GRPC_TLS_UNTRUSTED_PORT = ORACLE_PORT + 22;
const GATEWAY_FCGI_PORT = GATEWAY_PORT + 2;
const GATEWAY_UWSGI_PORT = GATEWAY_PORT + 10;
const GATEWAY_SCGI_PORT = GATEWAY_PORT + 11;
const GATEWAY_MEMCACHED_PORT = GATEWAY_PORT + 14;
const GATEWAY_GRPC_PORT = GATEWAY_PORT + 20;
const GATEWAY_GRPC_TLS_OK_PORT = GATEWAY_PORT + 21;
const GATEWAY_GRPC_TLS_UNTRUSTED_PORT = GATEWAY_PORT + 22;
const GRPC_MOCK_BINARY = 'sdkwork-webserver-grpc-mock';
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

function renderTemplate(ports) {
  let text = fs.readFileSync(path.join(fixtureDir, 'nginx.conf'), 'utf8');
  text = text
    .replaceAll('@ROOT@', path.join(fixtureDir, 'public'))
    .replaceAll('@PORT@', String(ports.port))
    .replaceAll('@FCGI@', String(ports.fcgi))
    .replaceAll('@UWSGI@', String(ports.uwsgi))
    .replaceAll('@SCGI@', String(ports.scgi))
    .replaceAll('@MEMCACHED@', String(ports.memcached))
    .replaceAll('@GRPC@', String(ports.grpc))
    .replaceAll('@GRPC_TLS_OK@', String(ports.grpcTlsOk))
    .replaceAll('@GRPC_TLS_UNTRUSTED@', String(ports.grpcTlsUntrusted))
    .replaceAll('@DEAD@', String(ports.dead));
  // The `# @UNIX_SERVER_INJECT@` marker stays a comment here: the gateway
  // serves this file through the nginx compatibility plane, which fails closed
  // on `unix:` pass targets by design. The unix listener runs in a separate
  // gateway process backed by a typed JSON configuration.
  for (const marker of [
    '@ROOT@', '@PORT@', '@FCGI@', '@UWSGI@', '@SCGI@', '@MEMCACHED@',
    '@GRPC@', '@GRPC_TLS_OK@', '@GRPC_TLS_UNTRUSTED@', '@DEAD@',
  ]) {
    if (text.includes(marker)) throw new Error(`placeholder ${marker} left unrendered`);
  }
  return text;
}

/** nginx http-level CGI parameters mirrored into the typed unix listener. */
const CGI_PARAMS = [
  ['SCRIPT_FILENAME', '$document_root$fastcgi_script_name'],
  ['SCRIPT_NAME', '$fastcgi_script_name'],
  ['PATH_INFO', '$fastcgi_path_info'],
  ['DOCUMENT_ROOT', '$document_root'],
  ['REQUEST_METHOD', '$request_method'],
  ['REQUEST_URI', '$request_uri'],
  ['QUERY_STRING', '$query_string'],
  ['CONTENT_LENGTH', '$content_length'],
  ['CONTENT_TYPE', '$content_type'],
  ['SERVER_PROTOCOL', '$server_protocol'],
  ['HTTP_X_SENT', '$http_x_sent'],
];

function typedParams() {
  return CGI_PARAMS.map(([name, value]) => ({ name, value, ifNotEmpty: false }));
}

function writeTypedUnixConfig(directory, port, publicRoot) {
  const config = {
    schemaVersion: 1,
    kind: 'sdkwork.webserver.app',
    appKey: 'sdkwork-differential-unix',
    nginx: {
      enabled: true,
      profile: 'http-core-v1',
      unknownDirectivePolicy: 'error',
      unixSocketRoot: GATEWAY_UNIX_ROOT,
    },
    listeners: [
      {
        id: 'unix-http',
        bind: '127.0.0.1',
        port,
        protocols: ['http1'],
        defaultVirtualHostRef: 'unix-host',
      },
    ],
    upstreams: [
      { id: 'unix-fcgi-target', targets: [{ url: `unix:${GATEWAY_FCGI_SOCKET}` }] },
      { id: 'unix-uwsgi-target', targets: [{ url: `unix:${GATEWAY_UWSGI_SOCKET}` }] },
    ],
    resources: [
      {
        id: 'unix-fastcgi',
        type: 'fast-cgi',
        upstreamRef: 'unix-fcgi-target',
        params: typedParams(),
        documentRoot: publicRoot,
        passRequestHeaders: true,
      },
      {
        id: 'unix-uwsgi',
        type: 'cgi-upstream',
        protocol: 'uwsgi',
        upstreamRef: 'unix-uwsgi-target',
        params: typedParams(),
        documentRoot: publicRoot,
        passRequestHeaders: true,
      },
    ],
    virtualHosts: [
      {
        id: 'unix-host',
        listenerRefs: ['unix-http'],
        serverNames: ['unix.example.com'],
        routes: [
          {
            id: 'unix-fastcgi-route',
            match: { pathType: 'prefix', path: '/unix-fcgi/' },
            resourceRef: 'unix-fastcgi',
          },
          {
            id: 'unix-uwsgi-route',
            match: { pathType: 'prefix', path: '/unix-uwsgi/' },
            resourceRef: 'unix-uwsgi',
          },
        ],
      },
    ],
    metadata: { source: 'differential-protocols typed unix listener' },
  };
  const typedPath = path.join(directory, 'unix-typed.json');
  fs.writeFileSync(typedPath, JSON.stringify(config, null, 1));
  return typedPath;
}

/** Generate the gRPC mock's CA, an unrelated CA, and the CA-signed server cert. */
function generateGrpcCertificates(directory) {
  fs.mkdirSync(directory, { recursive: true });
  const ca = path.join(directory, 'grpc-ca.pem');
  const otherCa = path.join(directory, 'grpc-other-ca.pem');
  const serverCert = path.join(directory, 'grpc-server.pem');
  const serverKey = path.join(directory, 'grpc-server.key');
  const extensions = path.join(directory, 'grpc-server.ext');
  fs.writeFileSync(
    extensions,
    'subjectAltName=DNS:localhost,IP:127.0.0.1\n' +
      'basicConstraints=critical,CA:FALSE\n' +
      'keyUsage=critical,digitalSignature,keyEncipherment\n' +
      'extendedKeyUsage=serverAuth\n',
  );
  const run = (...args) => {
    const result = spawnSync('openssl', args, { encoding: 'utf8' });
    if (result.error || result.status !== 0) {
      throw new Error(`openssl ${args.join(' ')} failed: ${result.stderr ?? result.error}`);
    }
  };
  for (const [cert, key, subject] of [
    [ca, path.join(directory, 'grpc-ca.key'), '/CN=grpc-test-ca'],
    [otherCa, path.join(directory, 'grpc-other-ca.key'), '/CN=grpc-other-ca'],
  ]) {
    run('req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert,
      '-days', '3', '-subj', subject);
  }
  const request = path.join(directory, 'grpc-server.csr');
  run('req', '-newkey', 'rsa:2048', '-nodes', '-keyout', serverKey, '-out', request,
    '-subj', '/CN=localhost');
  run('x509', '-req', '-in', request, '-CA', ca,
    '-CAkey', path.join(directory, 'grpc-ca.key'), '-CAcreateserial', '-days', '3',
    '-out', serverCert, '-extfile', extensions);
  return { ca, otherCa, serverCert, serverKey };
}

/** Build (or locate) the test-only Rust HTTP/2 gRPC responder. */
function grpcMockBinaryPath() {
  const candidate = path.join(root, 'target', 'debug', GRPC_MOCK_BINARY);
  const build = spawnSync(
    'cargo',
    ['build', '-p', GRPC_MOCK_BINARY],
    { cwd: root, stdio: 'inherit', env: process.env },
  );
  if (build.error || build.status !== 0) {
    throw new Error('cannot build the gRPC mock responder');
  }
  if (!fs.existsSync(candidate)) {
    throw new Error(`gRPC mock binary not found at ${candidate}`);
  }
  return candidate;
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

function runBattery(python, port, unixPort) {
  const result = spawnSync(
    python,
    [
      path.join(fixtureDir, 'battery.py'),
      '--port',
      String(port),
      '--unix-port',
      String(unixPort),
      '--out',
      OUR_CAPTURE,
    ],
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
    const wantTrailers = JSON.stringify(want.trailers ?? {});
    const gotTrailers = JSON.stringify(got.trailers ?? {});
    if (wantTrailers !== gotTrailers) {
      differences.push(`${name}: trailers ${wantTrailers} != ${gotTrailers}`);
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
    ['gateway gRPC responder', GATEWAY_GRPC_PORT],
    ['gateway gRPC TLS responder', GATEWAY_GRPC_TLS_OK_PORT],
    ['gateway gRPC TLS (untrusted) responder', GATEWAY_GRPC_TLS_UNTRUSTED_PORT],
    ['gateway dead upstream', GATEWAY_DEAD_PORT],
    ['gateway unix listener', GATEWAY_UNIX_PORT],
    ['oracle unix listener', ORACLE_UNIX_PORT],
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
  let grpcCertificates;
  try {
    grpcCertificates = generateGrpcCertificates(GATEWAY_CONF_DIR);
  } catch (error) {
    log(`FAIL: cannot generate the gRPC mock certificates: ${error.message}`);
    return 1;
  }
  fs.writeFileSync(
    GATEWAY_CONF,
    renderTemplate({
      port: GATEWAY_PORT,
      fcgi: GATEWAY_FCGI_PORT,
      uwsgi: GATEWAY_UWSGI_PORT,
      scgi: GATEWAY_SCGI_PORT,
      memcached: GATEWAY_MEMCACHED_PORT,
      grpc: GATEWAY_GRPC_PORT,
      grpcTlsOk: GATEWAY_GRPC_TLS_OK_PORT,
      grpcTlsUntrusted: GATEWAY_GRPC_TLS_UNTRUSTED_PORT,
      dead: GATEWAY_DEAD_PORT,
    }),
  );

  fs.mkdirSync(GATEWAY_UNIX_ROOT, { recursive: true });
  fs.rmSync(GATEWAY_FCGI_SOCKET, { force: true });
  fs.rmSync(GATEWAY_UWSGI_SOCKET, { force: true });
  const typedUnixConfig = writeTypedUnixConfig(
    GATEWAY_CONF_DIR,
    GATEWAY_UNIX_PORT,
    path.join(fixtureDir, 'public'),
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
  const mockFastcgiUnix = spawn(
    python,
    [path.join(fixtureDir, 'mock_fastcgi.py'), '--unix-socket', GATEWAY_FCGI_SOCKET],
    { cwd: root, stdio: ['ignore', mockLog, mockLog] },
  );
  const mockUwsgiUnix = spawn(
    python,
    [path.join(fixtureDir, 'mock_uwsgi.py'), '--unix-socket', GATEWAY_UWSGI_SOCKET],
    { cwd: root, stdio: ['ignore', mockLog, mockLog] },
  );
  let grpcMockBinary;
  try {
    grpcMockBinary = grpcMockBinaryPath();
  } catch (error) {
    log(`FAIL: ${error.message}`);
    for (const child of [mock, mockUwsgi, mockScgi, mockMemcached, mockFastcgiUnix, mockUwsgiUnix]) child.kill();
    return 1;
  }
  const mockGrpc = spawn(
    grpcMockBinary,
    [
      '--plain-port', String(GATEWAY_GRPC_PORT),
      '--tls-port', String(GATEWAY_GRPC_TLS_OK_PORT),
      '--tls-port', String(GATEWAY_GRPC_TLS_UNTRUSTED_PORT),
      '--cert', grpcCertificates.serverCert,
      '--key', grpcCertificates.serverKey,
    ],
    { cwd: root, stdio: ['ignore', mockLog, mockLog] },
  );
  const gatewayLog = fs.openSync(path.join(root, 'tmp', 'nginx-differential-protocols-gateway.log'), 'w');
  const gateway = spawn(binary.path, ['serve-nginx', GATEWAY_CONF], {
    cwd: root,
    stdio: ['ignore', gatewayLog, gatewayLog],
    env: { ...process.env, TZ: 'UTC' },
  });
  // The typed plane carries the unix-socket transport: the nginx compatibility
  // plane intentionally fails closed on `unix:` pass targets (decision: unix
  // sockets are configured only through `[http] unixSocketRoot`).
  const typedGateway = spawn(
    binary.path,
    ['data-plane', typedUnixConfig, '--format', 'json'],
    { cwd: root, stdio: ['ignore', gatewayLog, gatewayLog], env: { ...process.env, TZ: 'UTC' } },
  );

  try {
    if (!(await waitForPort(GATEWAY_PORT, 20_000))) {
      log(`FAIL: the gateway never listened on ${GATEWAY_PORT}; see tmp/nginx-differential-protocols-gateway.log`);
      return 1;
    }
    if (gateway.exitCode !== null) {
      log(`FAIL: the gateway exited with ${gateway.exitCode}`);
      return 1;
    }
    if (!(await waitForPort(GATEWAY_UNIX_PORT, 20_000))) {
      log(`FAIL: the typed unix gateway never listened on ${GATEWAY_UNIX_PORT}; see tmp/nginx-differential-protocols-gateway.log`);
      return 1;
    }
    if (typedGateway.exitCode !== null) {
      log(`FAIL: the typed unix gateway exited with ${typedGateway.exitCode}`);
      return 1;
    }
    if (!runBattery(python, GATEWAY_PORT, GATEWAY_UNIX_PORT)) {
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
    for (const child of [gateway, typedGateway, mock, mockUwsgi, mockScgi, mockMemcached, mockFastcgiUnix, mockUwsgiUnix, mockGrpc]) {
      if (child.exitCode === null) child.kill();
    }
    await delay(500);
    for (const child of [gateway, typedGateway, mock, mockUwsgi, mockScgi, mockMemcached, mockFastcgiUnix, mockUwsgiUnix, mockGrpc]) {
      if (child.exitCode === null) child.kill('SIGKILL');
    }
    fs.closeSync(mockLog);
    fs.closeSync(gatewayLog);
  }
}

process.exit(await main());
