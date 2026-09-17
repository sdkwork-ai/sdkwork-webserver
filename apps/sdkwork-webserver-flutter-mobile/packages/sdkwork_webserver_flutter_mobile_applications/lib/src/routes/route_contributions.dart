/// Route contributions for the applications capability.
///
/// Route ids follow `<surface>.<domain>.<capability>.<screen>` and stay aligned
/// with the PC, H5, mini program, and HarmonyOS roots. Route metadata must not
/// declare HTTP API paths, SDK methods, or transport details.
///
/// The permission hint is the code the owning module derives from its own
/// operationId: `deploy.apps.list` -> `[resource, action] = ("apps", "list")` ->
/// `read` -> `deploy.apps.read`.
library;

import 'package:sdkwork_webserver_flutter_mobile_shell/sdkwork_webserver_flutter_mobile_shell.dart';

const List<WebserverFlutterRouteContribution>
webserverFlutterApplicationsRouteContributions =
    <WebserverFlutterRouteContribution>[
  WebserverFlutterRouteContribution(
    id: 'app.webserver.applications.list',
    domain: 'webserver',
    capability: 'applications',
    screen: 'list',
    routeName: '/applications',
    titleKey: 'applications.list.title',
    auth: WebserverFlutterRouteAuth.requiredAccess,
    permissionHint: 'deploy.apps.read',
    navigation: WebserverFlutterNavigationContribution(
      labelKey: 'navigation.applications',
      permission: 'deploy.apps.read',
      order: 10,
    ),
  ),
];
