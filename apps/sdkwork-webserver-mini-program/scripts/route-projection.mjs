/**
 * Route projection loader.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §5: SDKWork packages are the source
 * boundary and the platform `pages`/`subPackages` lists are runtime loading
 * boundaries, so the build derives one from the other instead of maintaining both
 * by hand. The projection itself is TypeScript (it imports the capability
 * packages), so it is bundled with esbuild and required back in-process — which is
 * what lets `scripts/build-runtime.mjs` fail the build when `src/app.json` and the
 * route contributions disagree, and lets the contract tests assert the projected
 * result without a device.
 */
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const requireFromHere = createRequire(import.meta.url);

/**
 * Bundle + load the route projection. Exported so verification can call it
 * directly rather than re-deriving the projection from source text.
 *
 * @param {string} appRoot absolute mini program application root
 * @param {{ logLevel?: "silent" | "info" }} [options]
 */
export async function projectWebserverMiniProgramRoutes(appRoot, options = {}) {
  const cacheDir = path.join(appRoot, ".cache");
  mkdirSync(cacheDir, { recursive: true });
  const outfile = path.join(cacheDir, "route-projection.cjs");

  await esbuild.build({
    entryPoints: [path.join(appRoot, "src", "bootstrap", "routeProjection.ts")],
    outfile,
    bundle: true,
    platform: "node",
    format: "cjs",
    target: "node20",
    logLevel: options.logLevel ?? "silent",
    legalComments: "none",
  });

  // The projection is re-read on every call, so drop the previous module instance
  // instead of asserting against a stale one.
  delete requireFromHere.cache[outfile];
  return requireFromHere(outfile).projectWebserverMiniProgramAppJson();
}

export function resolveMiniProgramAppRoot(scriptUrl) {
  return path.resolve(path.dirname(fileURLToPath(scriptUrl)), "..");
}
