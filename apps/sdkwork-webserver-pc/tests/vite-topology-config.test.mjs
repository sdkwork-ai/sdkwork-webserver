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
    expect(viteConfig).toMatch(/dedupe:\s*\["react",\s*"react-dom",\s*"react-router",\s*"react-router-dom",\s*"@sdkwork\/utils"\]/u);
  });
});
