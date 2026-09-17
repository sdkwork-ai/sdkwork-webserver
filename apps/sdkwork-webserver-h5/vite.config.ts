import react from "@vitejs/plugin-react";
import path from "node:path";
import { env } from "node:process";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import {
  createCanonicalApiProxyConfig,
  resolveBrowserDevelopmentServer,
  resolveBrowserDistOutDir,
  resolveViteRuntimeProfile,
} from "./scripts/browser-topology.mjs";

const APP_ROOT = fileURLToPath(new URL(".", import.meta.url));
// RUNTIME_DIRECTORY_SPEC.md section "JavaScript tool cache" / ADR-20260730:
// shared Vite caches live below `node_modules/.vite/<surface-id>/`. The adaptive
// development ingress only routes a `/node_modules/.vite/<surface-id>/` request
// to the renderer that owns that surface and answers `410 Gone` otherwise, so
// the label also keeps the H5 renderer's dependency cache reachable from a cold
// browser.
const VITE_CACHE_SURFACE_ID = path.basename(path.resolve(APP_ROOT));

export default defineConfig(({ command, mode }) => {
  const runtimeProfile = resolveViteRuntimeProfile(mode, env);
  const developmentServer = command === "serve" && runtimeProfile.environment === "development"
    ? resolveBrowserDevelopmentServer({
        appRoot: APP_ROOT,
        deploymentProfile: runtimeProfile.deploymentProfile,
        environment: runtimeProfile.environment,
        processEnv: env,
      })
    : undefined;

  return {
    cacheDir: path.resolve(APP_ROOT, "node_modules/.vite", VITE_CACHE_SURFACE_ID),
    plugins: [react()],
    server: developmentServer ? {
      host: developmentServer.host,
      port: developmentServer.port,
      proxy: developmentServer.proxyTarget
        ? createCanonicalApiProxyConfig(developmentServer.proxyTarget)
        : undefined,
      strictPort: true,
    } : undefined,
    build: {
      outDir: resolveBrowserDistOutDir(runtimeProfile.environment, runtimeProfile.deploymentProfile),
      emptyOutDir: true,
      sourcemap: true,
      target: "es2022",
    },
    test: {
      environment: "jsdom",
      // `@testing-library/react` registers its automatic `afterEach(cleanup)`
      // against the global test API; without `globals` the hook never
      // registers and a rendered tree leaks into the next case's queries.
      globals: true,
    },
  };
});
