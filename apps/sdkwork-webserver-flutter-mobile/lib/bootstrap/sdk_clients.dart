/// Generated app SDK construction and capability binding for the Flutter root.
///
/// `APP_SDK_INTEGRATION_SPEC.md` section 2: generated clients are constructed at
/// the application root and injected downward; capability packages never build a
/// transport.
library;

import 'package:sdkwork_webserver_flutter_mobile_applications/sdkwork_webserver_flutter_mobile_applications.dart';
import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart';

/// Everything the root hands to capabilities.
class WebserverFlutterSdkClients {
  const WebserverFlutterSdkClients({
    required this.appClients,
    required this.deployAppCatalog,
  });

  final WebserverAppSdkClients appClients;

  /// The `deploy_app` catalog port. It is constructed unconditionally and its
  /// availability is *asked*, never assumed, so a capability renders an explicit
  /// unavailable state instead of dereferencing a null.
  final WebserverDeployAppCatalogPort deployAppCatalog;

  void dispose() {
    deployAppCatalog.dispose();
    appClients.dispose();
  }
}

WebserverFlutterSdkClients createWebserverFlutterSdkClients({
  required String appApiBaseUrl,
  String? authToken,
  String? accessToken,
}) {
  return WebserverFlutterSdkClients(
    appClients: createWebserverAppSdkClients(
      appApiBaseUrl: appApiBaseUrl,
      authToken: authToken,
      accessToken: accessToken,
    ),
    deployAppCatalog: WebserverDeployAppCatalogPort(),
  );
}

/// Bind the applications capability's reader to the root's catalog port.
///
/// The reader resolves [WebserverDeployAppCatalogPort.reader] **per call**
/// rather than at construction, so binding a generated Dart deployments client
/// later — or clearing it on logout — takes effect without rebuilding the view
/// model. When no reader is bound the port throws
/// [WebserverDeployAppCatalogUnavailableError], which the view model turns into
/// its own "no transport in this build" copy.
class WebserverFlutterPortBackedCatalogReader
    implements WebserverDeployAppCatalogReader {
  WebserverFlutterPortBackedCatalogReader(this.port);

  final WebserverDeployAppCatalogPort port;

  @override
  Future<WebserverDeployAppPage> list({
    required int page,
    required int pageSize,
  }) {
    return port.reader.list(page: page, pageSize: pageSize);
  }
}

WebserverFlutterApplicationsService createWebserverFlutterApplicationsService(
  WebserverFlutterSdkClients clients, {
  int pageSize = defaultWebserverFlutterListPageSize,
}) {
  return WebserverFlutterApplicationsService(
    reader: WebserverFlutterPortBackedCatalogReader(clients.deployAppCatalog),
    pageSize: pageSize,
  );
}
