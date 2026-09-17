/**
 * Route contributions for the applications capability.
 *
 * Route ids follow `<surface>.<domain>.<capability>.<screen>`
 * (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §8) and deliberately match the PC,
 * H5, and mini program roots' `app.webserver.applications.list` id, so all four
 * clients gate the same screen on the same authority permission.
 *
 * Route metadata must not declare an API URL or an SDK method.
 */
import type {
  WebserverHarmonyRouteContribution,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-shell';

export const webserverHarmonyApplicationsRouteContributions: WebserverHarmonyRouteContribution[] = [
  {
    id: 'app.webserver.applications.list',
    surface: 'app',
    domain: 'webserver',
    capability: 'applications',
    screen: 'list',
    pagePath: 'pages/applications/ApplicationsCatalog',
    titleKey: 'applications.list.title',
    auth: 'required',
    permissionHint: 'deploy.apps.read',
    navigation: {
      labelKey: 'navigation.applications',
      permission: 'deploy.apps.read',
      order: 10,
    },
  },
];
