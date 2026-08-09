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
      // Deployments console packages) resolve their React and router peers
      // from their own node_modules; alias the runtime singletons to this
      // application's copies so hooks and contexts never split into two
      // instances inside one renderer.
      alias: [
        { find: /^react$/, replacement: PACKAGE_REQUIRE.resolve("react") },
        { find: /^react-dom$/, replacement: PACKAGE_REQUIRE.resolve("react-dom") },
        { find: /^react-router-dom$/, replacement: PACKAGE_REQUIRE.resolve("react-router-dom") },
        { find: /^lucide-react$/, replacement: PACKAGE_REQUIRE.resolve("lucide-react") },
      ],
      dedupe: ["react", "react-dom", "react-router", "react-router-dom"],
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
      sourcemap: true,
      target: "es2022",
    },
    test: {
      // Cross-repository workspace packages and their icon peer resolve
      // React from their own node_modules; inline them so the runtime
      // singleton aliases above apply inside vitest too.
      server: {
        deps: {
          inline: [/@sdkwork\/(deployments|skills|mcp)/, "lucide-react"],
        },
      },
    },
  };
});
