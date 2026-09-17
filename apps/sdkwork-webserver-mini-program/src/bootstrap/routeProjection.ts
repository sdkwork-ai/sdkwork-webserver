import {
  createWebserverMpNavigation,
  listWebserverMpRootPages,
  projectWebserverMpSubPackages,
  validateWebserverMpRouteContributions,
  type WebserverMpNavigationEntry,
  type WebserverMpPageProjectionEntry,
  type WebserverMpSubPackageProjection,
} from "@sdkwork/webserver-mp-shell";

import { createWebserverMiniProgramRoutes } from "./routes";

/**
 * Route projection for the mini program build.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §5: SDKWork packages are source
 * boundaries, platform `pages`/`subPackages` are runtime loading boundaries, and
 * the build projects one into the other. `scripts/build-runtime.mjs` bundles this
 * module and asserts the result against the authored `src/app.json`, so a route
 * that is declared but not published (or published but not declared) fails the
 * build instead of silently rendering a blank page on the device.
 */
export interface WebserverMiniProgramRouteProjection {
  readonly pages: string[];
  readonly subPackages: WebserverMpSubPackageProjection[];
  readonly entries: WebserverMpPageProjectionEntry[];
  readonly routeIds: string[];
  readonly navigation: WebserverMpNavigationEntry[];
  readonly issues: string[];
}

export function projectWebserverMiniProgramAppJson(): WebserverMiniProgramRouteProjection {
  const routes = createWebserverMiniProgramRoutes();
  return {
    pages: listWebserverMpRootPages(routes),
    subPackages: projectWebserverMpSubPackages(routes),
    entries: routes.map((route) => ({
      pagePath: route.miniProgram.pagePath,
      rootPackage: route.miniProgram.rootPackage === true,
      ...(route.miniProgram.subpackage ? { subpackage: route.miniProgram.subpackage } : {}),
    })),
    routeIds: routes.map((route) => route.id),
    navigation: createWebserverMpNavigation(routes),
    issues: validateWebserverMpRouteContributions(routes),
  };
}
