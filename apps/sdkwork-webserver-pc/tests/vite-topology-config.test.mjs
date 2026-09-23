import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  CANONICAL_API_PROXY_PATHS,
  createCanonicalApiProxyConfig,
  resolveBrowserDevelopmentServer,
} from '../scripts/browser-topology.mjs';

const appRoot = path.resolve(import.meta.dirname, '..');

describe('Vite browser topology', () => {
  it('binds the private adaptive renderer port, not the public ingress', () => {
    const rendererPort = 54321;
    const ingressPort = 54322;
    const gatewayPort = 54323;
    const developmentServer = resolveBrowserDevelopmentServer({
      appRoot,
      deploymentProfile: 'standalone',
      environment: 'development',
      processEnv: {},
      readText(file) {
        const source = readFileSync(file, 'utf8');
        if (!file.endsWith('standalone.development.env')) return source;
        return source
          .replace(
            /^SDKWORK_WEBSERVER_PC_INTERNAL_DEV_PORT=.*$/mu,
            `SDKWORK_WEBSERVER_PC_INTERNAL_DEV_PORT=${rendererPort}`,
          )
          .replace(
            /^SDKWORK_WEBSERVER_WEB_DEV_INGRESS_BIND=.*$/mu,
            `SDKWORK_WEBSERVER_WEB_DEV_INGRESS_BIND=127.0.0.1:${ingressPort}`,
          )
          .replace(
            /^SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL=.*$/mu,
            `SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL=http://127.0.0.1:${gatewayPort}`,
          );
      },
    });

    expect(developmentServer).toMatchObject({
      host: '127.0.0.1',
      port: rendererPort,
      profileId: 'standalone.development',
      adaptive: true,
      architecture: 'pc-web',
      proxyTarget: `http://127.0.0.1:${gatewayPort}`,
    });
    expect(developmentServer.port).not.toBe(ingressPort);
  });

  it('proxies only canonical paths without rewriting client-visible URIs', () => {
    const proxy = createCanonicalApiProxyConfig('http://127.0.0.1:49111');

    expect(CANONICAL_API_PROXY_PATHS).toEqual([
      '/app/v3/api',
      '/backend/v3/api',
      '/openapi.json',
      '/healthz',
      '/readyz',
      '/livez',
      '/metrics',
    ]);
    expect(Object.keys(proxy)).toEqual(CANONICAL_API_PROXY_PATHS);
    for (const options of Object.values(proxy)) {
      expect(options.target).toBe('http://127.0.0.1:49111');
      expect(options).not.toHaveProperty('rewrite');
    }
  });

  it('rejects standalone development without canonical same-origin delivery evidence', () => {
    expect(() => resolveBrowserDevelopmentServer({
      appRoot,
      deploymentProfile: 'standalone',
      environment: 'development',
      processEnv: {},
      readText(file) {
        const source = readFileSync(file, 'utf8');
        if (!file.endsWith('topology.spec.json')) return source;
        const topology = JSON.parse(source);
        topology.orchestration.profiles['standalone.development']
          .browserDeliveries[0].preserveCanonicalPaths = false;
        return JSON.stringify(topology);
      },
    })).toThrow(/canonical-path same-origin dev-server proxy/u);
  });

  it('keeps React workspace deduplication enabled', () => {
    const viteConfig = readFileSync(path.join(appRoot, 'vite.config.ts'), 'utf8');
    // The guard is about the property - these packages resolve to a single copy,
    // because a second React breaks hooks and a stale nested `@sdkwork/*` copy
    // silently drops exports - not about how the list is laid out. It used to
    // match the one-line literal, so it failed the moment a sixth entry was
    // added above it on its own line.
    const declared = viteConfig.match(/dedupe:\s*\[([^\]]*)\]/u);
    expect(declared, 'vite.config.ts declares no resolve.dedupe list').not.toBeNull();
    const deduped = [...declared[1].matchAll(/"([^"]+)"/gu)].map((entry) => entry[1]);
    expect(deduped).toEqual(expect.arrayContaining([
      'react',
      'react-dom',
      'react-router',
      'react-router-dom',
      '@sdkwork/utils',
    ]));
  });
});

describe('Credential-entry bootstrap handoff', () => {
  const BOOTSTRAP_GLOBAL = '__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__';

  async function resolvePlugin(options) {
    const viteConfig = (await import('../vite.config.ts')).default;
    const config = typeof viteConfig === 'function' ? await viteConfig(options) : viteConfig;
    const plugins = (Array.isArray(config.plugins) ? config.plugins : [config.plugins]).flat();
    return plugins.find((plugin) => (
      plugin && plugin.name === 'sdkwork-iam-credential-entry-bootstrap'
    ));
  }

  function readInjectedToken(plugin) {
    const transformed = plugin.transformIndexHtml.handler(
      '<!doctype html><html><head><title>x</title></head><body></body></html>',
    );
    const script = transformed.tags.find((tag) => tag.tag === 'script');
    return script ? script.children : '';
  }

  it('injects a development bootstrap Access-Token for the IAM login renderer', async () => {
    const restore = process.env.SDKWORK_ACCESS_TOKEN;
    delete process.env.SDKWORK_ACCESS_TOKEN;
    try {
      const plugin = await resolvePlugin({ command: 'serve', mode: 'standalone.development' });
      expect(plugin).toBeDefined();

      const script = readInjectedToken(plugin);
      expect(script).toContain(`globalThis.${BOOTSTRAP_GLOBAL} =`);
      const token = script.match(/= "([^"]+)";/u)?.[1] ?? '';
      const claims = JSON.parse(Buffer.from(token.split('.')[1], 'base64url').toString('utf8'));
      expect(claims.app_id).toBe('sdkwork-webserver-pc');
      expect(claims.environment).toBe('development');
      expect(claims.token_type).toBe('access');
    } finally {
      if (restore === undefined) delete process.env.SDKWORK_ACCESS_TOKEN;
      else process.env.SDKWORK_ACCESS_TOKEN = restore;
    }
  });

  it('never installs the injection plugin outside development', async () => {
    expect(await resolvePlugin({ command: 'serve', mode: 'standalone.production' })).toBeUndefined();
    expect(await resolvePlugin({ command: 'build', mode: 'standalone.development' })).toBeUndefined();
  });
});
