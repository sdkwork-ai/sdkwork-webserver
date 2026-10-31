//! Probe for the GeoIP variable-plumbing proxy differential.
//!
//! The same case matrix is measured against real nginx 1.29.6 (where the GeoIP
//! module is absent and `set` fixes the variable values) and against the Rust
//! gateway (where the variables come from synthetic MMDB databases). The
//! comparison proves nginx-compatible expansion in `return` bodies, `map`
//! results and `add_header`; it does not claim a GeoIP lookup differential
//! against the oracle, because the oracle was not built with
//! `--with-http_geoip_module`.
import http from 'node:http';
import net from 'node:net';

/** Cases: `xff` is the `X-Forwarded-For` value sent with the request. */
export const CASES = [
  {
    id: 'us-record',
    path: '/us',
    xff: '198.51.100.7',
    note: '198.51.100.0/24 -> US full city/org record',
  },
  {
    id: 'de-record',
    path: '/de',
    xff: '203.0.113.7',
    note: '203.0.113.0/24 -> DE full city/org record',
  },
  {
    id: 'empty-record',
    path: '/empty',
    xff: '192.0.2.99',
    note: '192.0.2.99 has no database record -> every variable is the nginx not_found empty string',
  },
  {
    id: 'recursive-trusted-hop',
    path: '/us',
    xff: '198.51.100.7, 127.0.0.1',
    note: 'geoip_proxy_recursive walks the trusted 127.0.0.1 hop back to 198.51.100.7',
  },
];

const HEADERS_OF_INTEREST = ['x-geoip-country', 'x-geoip-zone'];

/** Capture one response: status, body, and the GeoIP headers. */
export function request(port, testCase) {
  return new Promise((resolve, reject) => {
    const headers = {};
    if (testCase.xff !== null) headers['x-forwarded-for'] = testCase.xff;
    const request = http.request(
      { host: '127.0.0.1', port, path: testCase.path, method: 'GET', headers },
      (response) => {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          const captured = {};
          for (const name of HEADERS_OF_INTEREST) {
            captured[name] = response.headers[name] ?? null;
          }
          resolve({
            status: response.statusCode,
            body: Buffer.concat(chunks).toString('utf8'),
            headers: captured,
          });
        });
      },
    );
    request.on('error', reject);
    request.setTimeout(10_000, () => request.destroy(new Error('probe timeout')));
    request.end();
  });
}

/** Capture every case against one port. */
export async function capture(port) {
  const results = {};
  for (const testCase of CASES) {
    results[testCase.id] = await request(port, testCase);
  }
  return results;
}

/** Capture the bytes one stream `return` handler writes before closing. */
export function requestStream(port) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection({ host: '127.0.0.1', port });
    const chunks = [];
    socket.setTimeout(10_000, () => socket.destroy(new Error('stream probe timeout')));
    socket.on('connect', () => socket.resume());
    socket.on('data', (chunk) => chunks.push(chunk));
    socket.on('end', () => resolve({ body: Buffer.concat(chunks).toString('utf8') }));
    socket.on('error', reject);
  });
}

/** Capture the stream GeoIP case against one port. */
export async function captureStream(port) {
  return { body: (await requestStream(port)).body };
}
