import tailwindcss from "@tailwindcss/vite";
import { mergeRepoDevBootstrapAccessTokenEnv } from "@sdkwork/iam-credential-entry/node-bootstrap";
import { createSdkworkCredentialEntryBootstrapVitePlugin } from "@sdkwork/iam-credential-entry/vite";
import react from "@vitejs/plugin-react";
import { createRequire } from "node:module";
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
// The config is compiled by vitest into a cache directory, so import.meta.url
// cannot locate package roots; resolve the runtime singletons through the
// package working directory instead. The working directory is stable for
// both the vite dev server and vitest runs.
const PACKAGE_REQUIRE = createRequire(path.join(process.cwd(), "__sdkwork_vite_config__.js"));
// RUNTIME_DIRECTORY_SPEC.md section "JavaScript tool cache" /
// ADR-20260730: shared Vite caches live below `node_modules/.vite/<surface-id>/`.
// The adaptive development ingress routes every `/node_modules/.vite/<surface-id>/`
// request to the renderer that owns that surface and answers `410 Gone` for an
// unlabelled cache path (`tools/topology/lib/adaptive-web.mjs`), so the label is
// also what keeps a cold browser able to load the renderer's dependency cache
// instead of a stale-cache error page.
const VITE_CACHE_SURFACE_ID = path.basename(path.resolve(APP_ROOT));

/**
 * Resolve the private credential-entry bootstrap Access-Token for this renderer
 * (`IAM_CREDENTIAL_ENTRY_SPEC.md` section 4/5).
 *
 * The login page cannot render without a bootstrap credential: every IAM
 * runtime/verification-policy operation is access-token-only and the generated
 * SDK fails before network dispatch when the TokenManager holds none. The Vite
 * serve process is therefore the handoff point for the development renderer.
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
    // the provisioning gap, while a missing token only leaves the login page
    // reporting unavailable IAM metadata.
    process.stderr.write(
      "[webserver-pc] credential-entry bootstrap Access-Token resolution failed: "
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
      tailwindcss(),
      createSdkworkCredentialEntryBootstrapVitePlugin({
        accessToken: credentialEntryBootstrapAccessToken,
        environment: runtimeProfile.environment,
      }),
    ],
    resolve: {
      // Cross-repository workspace links (for example the SDKWork
      // Deployments and Appbase console packages) resolve their React and
      // router peers from their own node_modules; alias the runtime
      // singletons — including subpath entries such as `react/jsx-runtime` —
      // to this application's copies so hooks and contexts never split into
      // two instances inside one renderer.
      alias: [
        {
          find: /^react(?:\/(.*))?$/,
          replacement: `${path.dirname(PACKAGE_REQUIRE.resolve("react/package.json"))}/$1`,
        },
        {
          find: /^react-dom(?:\/(.*))?$/,
          replacement: `${path.dirname(PACKAGE_REQUIRE.resolve("react-dom/package.json"))}/$1`,
        },
        { find: /^react-router-dom$/, replacement: PACKAGE_REQUIRE.resolve("react-router-dom") },
        { find: /^lucide-react$/, replacement: PACKAGE_REQUIRE.resolve("lucide-react") },
        {
          find: /^@sdkwork\/utils$/,
          replacement: path.resolve(APP_ROOT, "node_modules/@sdkwork/utils"),
        },
      ],
      dedupe: ["react", "react-dom", "react-router", "react-router-dom", "@sdkwork/utils"],
    },
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
      // Cross-repository workspace packages and their icon peer resolve
      // React from their own node_modules; inline them so the runtime
      // singleton aliases above apply inside vitest too.
      server: {
        deps: {
          // Every cross-repository `@sdkwork/*` workspace package must go
          // through the Vite pipeline so the runtime-singleton aliases above
          // apply; an externalized sibling resolves React from its own
          // node_modules and splits the renderer into two instances.
          // `@testing-library/react` is inlined for the same reason: it pulls
          // its own `react-dom` peer, which must re-resolve onto the
          // application's copy.
          inline: [
            /^@sdkwork\//,
            "lucide-react",
            "@testing-library/react",
            "react",
            "react-dom",
            "react-dom/client",
          ],
        },
      },
    },
  };
});
