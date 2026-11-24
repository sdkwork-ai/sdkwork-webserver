// Wire-level HTTP/TLS harness for the live edge regression suites.
//
// Every request here is written to the socket by hand instead of going
// through Node's `http.request`: the regression compares the edge against
// nginx semantics, and a client that silently merges duplicate Host headers,
// rewrites request targets, or surfaces TLS alerts as uncaught exceptions
// would hide exactly the behaviors under test.
import net from 'node:net';
import tls from 'node:tls';
import http2 from 'node:http2';

const DEFAULT_TIMEOUT_MS = 15_000;

/** Resolves the deployment under test from the environment. */
export function resolveTarget() {
  const domain = process.env.SDKWORK_EDGE_DOMAIN ?? '';
  const host = process.env.SDKWORK_EDGE_HOST ?? '';
  const mode = process.env.SDKWORK_EDGE_MODE ?? 'production';
  const apiDomain = process.env.SDKWORK_EDGE_API_DOMAIN ?? '';
  if (!domain || !host) {
    return { configured: false, domain, host, mode, apiDomain };
  }
  if (mode !== 'production' && mode !== 'development') {
    throw new Error(`SDKWORK_EDGE_MODE must be production or development, got "${mode}"`);
  }
  return { configured: true, domain, host, mode, apiDomain };
}

/** Skips the current test when no live deployment is configured. */
export function skipWithoutTarget(context) {
  const target = resolveTarget();
  if (!target.configured) {
    context.skip('set SDKWORK_EDGE_DOMAIN and SDKWORK_EDGE_HOST to run against a live edge');
  }
  return target;
}

export async function openSocket({ secure, host, servername, port, timeout = DEFAULT_TIMEOUT_MS, alpn }) {
  const targetPort = port ?? (secure ? 443 : 80);
  const options = { host, port: targetPort };
  if (secure) {
    Object.assign(options, {
      servername: servername ?? host,
      rejectUnauthorized: false,
      ...(alpn ? { ALPNProtocols: alpn } : {}),
    });
  }
  const socket = secure ? tls.connect(options) : net.connect(options);
  socket.setNoDelay(true);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      socket.destroy();
      reject(new Error(`connect timeout ${host}:${targetPort}`));
    }, timeout);
    const onError = (error) => {
      clearTimeout(timer);
      reject(error);
    };
    socket.once('error', onError);
    socket.once(secure ? 'secureConnect' : 'connect', () => {
      clearTimeout(timer);
      socket.off('error', onError);
      resolve(socket);
    });
  });
}

function parseChunks(buffer) {
  const chunks = [];
  let offset = 0;
  while (offset < buffer.length) {
    const lineEnd = buffer.indexOf('\r\n', offset);
    if (lineEnd < 0) return { chunks, complete: false, consumed: offset };
    const sizeToken = buffer.subarray(offset, lineEnd).toString('latin1').split(';')[0].trim();
    const size = Number.parseInt(sizeToken, 16);
    if (!Number.isFinite(size)) throw new Error(`invalid chunk size "${sizeToken}"`);
    if (size === 0) {
      // Trailer section terminates at an empty line; consume it when present.
      const trailerEnd = buffer.indexOf('\r\n', lineEnd + 2);
      return { chunks, complete: true, consumed: trailerEnd < 0 ? buffer.length : trailerEnd + 2 };
    }
    const start = lineEnd + 2;
    if (buffer.length < start + size + 2) return { chunks, complete: false, consumed: offset };
    chunks.push(buffer.subarray(start, start + size));
    offset = start + size + 2;
  }
  return { chunks, complete: false, consumed: offset };
}

/**
 * Sends one HTTP/1.1 request over a socket and resolves with the parsed
 * response. `headers` is an ordered list so duplicate fields survive.
 */
export function exchangeOn(socket, { method = 'GET', path = '/', headers = [], body = null, requestTarget, host, timeout = DEFAULT_TIMEOUT_MS, keepOpen = false }) {
  const targetLine = requestTarget ?? path;
  const lines = [`${method} ${targetLine} HTTP/1.1`];
  if (host !== null && !headers.some(([name]) => name.toLowerCase() === 'host')) {
    lines.push(`Host: ${host}`);
  }
  for (const [name, value] of headers) lines.push(`${name}: ${value}`);
  if (body !== null && !headers.some(([name]) => name.toLowerCase() === 'content-length') && !headers.some(([name]) => name.toLowerCase() === 'transfer-encoding')) {
    lines.push(`Content-Length: ${Buffer.byteLength(body)}`);
  }
  const payload = `${lines.join('\r\n')}\r\n\r\n${body ?? ''}`;
  return new Promise((resolve, reject) => {
    const received = [];
    let settled = false;
    const finish = (error, result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.off('data', onData);
      socket.off('error', onSocketError);
      socket.off('close', onClose);
      if (error) {
        socket.destroy();
        reject(error);
        return;
      }
      if (!keepOpen) socket.destroy();
      resolve(result);
    };
    let headerEnd = -1;
    const parse = () => {
      const buffer = Buffer.concat(received);
      if (headerEnd < 0) {
        headerEnd = buffer.indexOf('\r\n\r\n');
        if (headerEnd < 0) return null;
      }
      const head = buffer.subarray(0, headerEnd).toString('latin1');
      const [statusLine, ...headerLines] = head.split('\r\n');
      const match = /^HTTP\/(\d\.\d) (\d{3})(?: (.*))?$/.exec(statusLine);
      if (!match) throw new Error(`unparsable status line "${statusLine}"`);
      const headers = headerLines.map((line) => {
        const index = line.indexOf(':');
        return index < 0 ? [line, ''] : [line.slice(0, index).trim(), line.slice(index + 1).trim()];
      });
      const header = (name) => headers.find(([key]) => key.toLowerCase() === name)?.[1];
      const statuses = [204, 304];
      const status = Number(match[2]);
      const noBody = method === 'HEAD' || statuses.includes(status) || status < 200;
      let rest = buffer.subarray(headerEnd + 4);
      let bodyBuffer = Buffer.alloc(0);
      if (!noBody) {
        const contentLength = header('content-length');
        const transferEncoding = header('transfer-encoding');
        if (transferEncoding && /chunked/i.test(transferEncoding)) {
          const decoded = parseChunks(rest);
          if (!decoded.complete) return null;
          bodyBuffer = Buffer.concat(decoded.chunks);
          rest = rest.subarray(decoded.consumed);
        } else if (contentLength !== undefined) {
          const length = Number.parseInt(contentLength, 10);
          if (rest.length < length) return null;
          bodyBuffer = rest.subarray(0, length);
          rest = rest.subarray(length);
        } else if (header('connection')?.toLowerCase() === 'close') {
          return null; // read until the peer closes
        } else {
          return null;
        }
      }
      return { status, reason: match[3] ?? '', httpVersion: match[1], headers, header, body: bodyBuffer, rest, text: bodyBuffer.toString('utf8') };
    };
    const onData = (chunk) => {
      received.push(chunk);
      let result;
      try {
        result = parse();
      } catch (error) {
        finish(error);
        return;
      }
      if (result) finish(null, { ...result, socket, closedByPeer: false });
    };
    const onSocketError = (error) => finish(error);
    const onClose = () => {
      if (settled) return;
      // A response without length information ends with the peer's FIN.
      try {
        const result = parse();
        if (result) {
          settled = true;
          clearTimeout(timer);
          resolve({ ...result, socket, closedByPeer: true });
          return;
        }
      } catch (error) {
        finish(error);
        return;
      }
      finish(new Error('connection closed before a complete response'));
    };
    const timer = setTimeout(() => finish(new Error('response timeout')), timeout);
    socket.on('data', onData);
    socket.on('error', onSocketError);
    socket.on('close', onClose);
    socket.write(payload);
  });
}

/** Opens a socket, sends one request, closes it. */
export async function exchange({ secure, host, servername, port, alpn, ...request }) {
  const socket = await openSocket({ secure, host, servername: servername ?? host, port, alpn });
  try {
    return await exchangeOn(socket, { host: servername ?? host, ...request });
  } finally {
    socket.destroy();
  }
}

/** Sends raw bytes and resolves with everything the peer sent before closing (bounded). */
export async function rawBytes({ secure, host, servername, payload, port, timeout = DEFAULT_TIMEOUT_MS, maxBytes = 1 << 20 }) {
  const socket = await openSocket({ secure, host, servername: servername ?? host, port, timeout });
  return new Promise((resolve, reject) => {
    const chunks = [];
    let settled = false;
    const settle = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      if (error) reject(error);
      else resolve(Buffer.concat(chunks));
    };
    const timer = setTimeout(() => settle(null), timeout);
    socket.on('data', (chunk) => {
      chunks.push(chunk);
      if (Buffer.concat(chunks).length >= maxBytes) settle(null);
    });
    socket.on('error', settle);
    socket.on('close', () => settle(null));
    socket.write(payload, (error) => { if (error) settle(null); });
  });
}

/** Performs one HTTP/2 request against the TLS listener. */
export async function h2Request({ host, servername, method = 'GET', path = '/', headers = {}, timeout = DEFAULT_TIMEOUT_MS }) {
  const session = http2.connect(`https://${host}:443`, { rejectUnauthorized: false, servername: servername ?? host });
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { session.close(); reject(new Error('h2 timeout')); }, timeout);
    session.on('error', (error) => { clearTimeout(timer); reject(error); });
    const stream = session.request({ ':method': method, ':path': path, ...headers });
    const chunks = [];
    let responseHeaders;
    stream.on('response', (hdrs) => { responseHeaders = hdrs; });
    stream.on('data', (chunk) => chunks.push(chunk));
    stream.on('end', () => {
      clearTimeout(timer);
      const alpn = session.socket?.alpnProtocol;
      session.close();
      const flat = Object.fromEntries(Object.entries(responseHeaders).filter(([key]) => !key.startsWith(':')).map(([key, value]) => [key.toLowerCase(), Array.isArray(value) ? value.join(', ') : value]));
      resolve({ status: responseHeaders[':status'], headers: flat, header: (name) => flat[name.toLowerCase()], body: Buffer.concat(chunks), alpn });
    });
    stream.on('error', (error) => { clearTimeout(timer); session.close(); reject(error); });
    stream.end();
  });
}

export const rfc7231Date = /^[A-Z][a-z]{2}, \d{2} [A-Z][a-z]{2} \d{4} \d{2}:\d{2}:\d{2} GMT$/;

export function percentile(values, fraction) {
  if (values.length === 0) return Number.NaN;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.ceil(fraction * sorted.length) - 1);
  return sorted[Math.max(0, index)];
}
