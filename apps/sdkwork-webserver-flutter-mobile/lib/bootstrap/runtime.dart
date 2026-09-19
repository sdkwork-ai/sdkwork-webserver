/// Application bootstrap for the Flutter mobile root.
///
/// `bootstrap()` resolves the runtime profile, constructs the generated clients,
/// and validates the route table **before** the first frame. A profile failure is
/// carried as [WebserverFlutterRuntime.failure] instead of thrown out of `main`:
/// a thrown bootstrap leaves the platform showing a blank window, while a carried
/// failure lets the app render what went wrong (`SOURCE_CONFIG_SPEC.md` — never
/// pretend a host default exists).
library;

import 'environment.dart';
import 'host_adapters.dart';
import 'iam_runtime.dart';
import 'routes.dart';
import 'sdk_clients.dart';
import 'package:sdkwork_webserver_flutter_mobile_shell/sdkwork_webserver_flutter_mobile_shell.dart';

class WebserverFlutterRuntime {
  const WebserverFlutterRuntime({
    required this.iam,
    required this.routes,
    this.environment,
    this.sdkClients,
    this.failure,
  });

  final WebserverFlutterEnvironment? environment;
  final WebserverFlutterSdkClients? sdkClients;
  final WebserverFlutterIamRuntime iam;
  final List<WebserverFlutterRouteContribution> routes;

  /// Non-null when the runtime profile or the route table was rejected.
  final String? failure;

  bool get ready => failure == null;
}

WebserverFlutterRuntime bootstrap({
  String? appApiBaseUrl,
  String? applicationPublicHttpUrl,
  String? deploymentProfile,
  String? environment,
  String? profileId,
  String? runtimeTarget,
  List<String> grantedPermissions = const <String>[],
}) {
  final iam = createWebserverFlutterIamRuntime(grantedPermissions);
  registerWebserverFlutterHostAdapters();
  try {
    final resolvedEnvironment = createWebserverFlutterEnvironment(
      appApiBaseUrl: appApiBaseUrl,
      applicationPublicHttpUrl: applicationPublicHttpUrl,
      deploymentProfile: deploymentProfile,
      environment: environment,
      profileId: profileId,
      runtimeTarget: runtimeTarget,
    );
    final sdkClients = createWebserverFlutterSdkClients();
    return WebserverFlutterRuntime(
      environment: resolvedEnvironment,
      sdkClients: sdkClients,
      iam: iam,
      routes: createWebserverFlutterRoutes(),
    );
  } catch (error) {
    return WebserverFlutterRuntime(
      iam: iam,
      routes: const <WebserverFlutterRouteContribution>[],
      failure: error.toString(),
    );
  }
}
