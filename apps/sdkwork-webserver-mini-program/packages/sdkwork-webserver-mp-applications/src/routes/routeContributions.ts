import type { WebserverMpRouteContribution } from "@sdkwork/webserver-mp-shell";

/**
 * Route contributions for the applications capability.
 *
 * Route ids follow `<surface>.<domain>.<capability>.<screen>`
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §9). The physical path is a root-package
 * page because the console's first screen must render before any subpackage has
 * finished downloading; it must never declare an API URL or SDK method.
 *
 * `permissionHint` mirrors the PC console's `deploy.apps.read` entry, so both
 * clients gate the same screen on the same authority permission.
 */
export const webserverMpApplicationsRouteContributions: WebserverMpRouteContribution[] = [
  {
    id: "app.webserver.applications.list",
    surface: "app",
    domain: "webserver",
    capability: "applications",
    screen: "list",
    titleKey: "applications.list.title",
    auth: "required",
    permissionHint: "deploy.apps.read",
    navigation: {
      labelKey: "navigation.applications",
      permission: "deploy.apps.read",
      order: 10,
    },
    miniProgram: { rootPackage: true, pagePath: "pages/applications/index" },
  },
];
