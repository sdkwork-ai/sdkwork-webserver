import { mergeRepoDevBootstrapAccessTokenEnv } from "@sdkwork/iam-credential-entry/node-bootstrap";
import { createSdkworkCredentialEntryBootstrapVitePlugin } from "@sdkwork/iam-credential-entry/vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { env } from "node:process";
import { fileURLToPath } from "node:url";
import { loadEnv } from "vite";
import { defineConfig } from "vitest/config";
import {
  createCanonicalApiProxyConfig,
  resolveBrowserDevelopmentServer,
  resolveBrowserDistOutDir,
  resolveViteRuntimeProfile,
} from "./scripts/browser-topology.mjs";

const APP_ROOT = fileURLToPath(new URL(".", import.meta.url));
/** Repository root that owns `.sdkwork.local.env` and the surface manifests. */
const REPO_ROOT = path.resolve(APP_ROOT, "../..");
/** Surface manifest that owns the credential-entry bootstrap identity. */
const APP_MANIFEST_PATH = path.join(APP_ROOT, "sdkwork.app.config.json");
// RUNTIME_DIRECTORY_SPEC.md section "JavaScript tool cache" / ADR-20260730:
// shared Vite caches live below `node_modules/.vite/<surface-id>/`. The adaptive
// development ingress only routes a `/node_modules/.vite/<surface-id>/` request
// to the renderer that owns that surface and answers `410 Gone` otherwise, so
// the label also keeps the H5 renderer's dependency cache reachable from a cold
// browser.
const VITE_CACHE_SURFACE_ID = path.basename(path.resolve(APP_ROOT));

/**
 * Resolve the private credential-entry bootstrap Access-Token for this renderer
 * (`IAM_CREDENTIAL_ENTRY_SPEC.md` section 4/5).
 *
 * The applications screen dispatches protected operations, and the generated SDK
 * fails before network dispatch when the TokenManager holds no access token, so
 * an unbootstrapped renderer can only report that the list could not be loaded.
 * The Vite serve process is the handoff point for the development renderer,
 * exactly as it is for the PC renderer.
 *
 * Sources, in precedence order:
 * 1. an explicitly provisioned `SDKWORK_ACCESS_TOKEN` (process env, then the
 *    private app env files `.env`, `.env.local`, `.env.<mode>`,
 *    `.env.<environment>`, and their `.local` variants — all uncommitted);
 * 2. the canonical repository bootstrap resolution shared with the dev
 *    lifecycle, which may generate a disposable local bootstrap JWT from the
 *    surface manifest identity.
 *
 * Generation is development-only. Test, staging, demo, and production
 * environments never generate and never embed a token: an absent credential
 * simply leaves the renderer unbootstrapped instead of leaking into browser
 * artifacts.
 */
function resolveCredentialEntryBootstrapAccessToken({
  command,
  mode,
  environment,
  deploymentProfile,
}: {
  command: string;
  mode: string;
  environment: string;
  deploymentProfile: string;
}): string | undefined {
  const privateEnv = {
    ...loadEnv(mode, APP_ROOT, ""),
    ...loadEnv(environment, APP_ROOT, ""),
  };
  const mergedEnv = { ...env, ...privateEnv };
  if (command !== "serve" || environment !== "development") {
    return mergedEnv.SDKWORK_ACCESS_TOKEN;
  }
  try {
    return mergeRepoDevBootstrapAccessTokenEnv({
      deploymentMode: deploymentProfile === "cloud" ? "saas" : "local",
      env: mergedEnv,
      manifestPath: APP_MANIFEST_PATH,
      repoRoot: REPO_ROOT,
      runtimeTarget: "browser",
    }).SDKWORK_ACCESS_TOKEN;
  } catch (error) {
    // Fail closed with an actionable warning: breaking the renderer would hide
    // the provisioning gap, while a missing token only leaves the screens
    // reporting unavailable protected data.
    process.stderr.write(
      "[webserver-h5] credential-entry bootstrap Access-Token resolution failed: "
      + `${error instanceof Error ? error.message : String(error)}\n`,
    );
    return mergedEnv.SDKWORK_ACCESS_TOKEN;
  }
}

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
  const credentialEntryBootstrapAccessToken = resolveCredentialEntryBootstrapAccessToken({
    command,
    deploymentProfile: runtimeProfile.deploymentProfile,
    environment: runtimeProfile.environment,
    mode,
  });

  return {
    cacheDir: path.resolve(APP_ROOT, "node_modules/.vite", VITE_CACHE_SURFACE_ID),
    plugins: [
      react(),
      createSdkworkCredentialEntryBootstrapVitePlugin({
        accessToken: credentialEntryBootstrapAccessToken,
        environment: runtimeProfile.environment,
      }),
    ],
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
