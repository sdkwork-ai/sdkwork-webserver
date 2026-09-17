/// Route assembly for the Flutter mobile root.
///
/// Capability packages contribute placement metadata; the root validates it and
/// mounts the result. Validation runs at bootstrap so a malformed route id or a
/// duplicate path fails at startup rather than at navigation time.
library;

import 'package:sdkwork_webserver_flutter_mobile_applications/sdkwork_webserver_flutter_mobile_applications.dart';
import 'package:sdkwork_webserver_flutter_mobile_shell/sdkwork_webserver_flutter_mobile_shell.dart';

List<WebserverFlutterRouteContribution> createWebserverFlutterRoutes() {
  final routes = <WebserverFlutterRouteContribution>[
    ...webserverFlutterApplicationsRouteContributions,
  ];
  final issues = validateWebserverFlutterRouteContributions(routes);
  if (issues.isNotEmpty) {
    throw StateError('route contributions are invalid: ${issues.join("; ")}');
  }
  return createWebserverFlutterRouteRegistry(routes);
}

String? resolveWebserverFlutterInitialRoute(
  List<WebserverFlutterRouteContribution> routes,
) {
  return resolveWebserverFlutterHomeRouteName(routes);
}
