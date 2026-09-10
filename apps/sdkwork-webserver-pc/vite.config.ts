import tailwindcss from "@tailwindcss/vite";
import { createSdkworkCredentialEntryBootstrapVitePlugin } from "@sdkwork/iam-credential-entry/vite";
import react from "@vitejs/plugin-react";
import { createRequire } from "node:module";
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
// The config is compiled by vitest into a cache directory, so import.meta.url
// cannot locate package roots; resolve the runtime singletons through the
// package working directory instead. The working directory is stable for
// both the vite dev server and vitest runs.
const PACKAGE_REQUIRE = createRequire(path.join(process.cwd(), "__sdkwork_vite_config__.js"));

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
    plugins: [
      react(),
      tailwindcss(),
      createSdkworkCredentialEntryBootstrapVitePlugin({
        accessToken: env.SDKWORK_ACCESS_TOKEN,
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
