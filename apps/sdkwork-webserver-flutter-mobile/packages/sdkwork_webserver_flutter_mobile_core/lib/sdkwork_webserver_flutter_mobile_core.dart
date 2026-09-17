/// SDKWork Web Server Flutter mobile core.
///
/// Owns composition, generated-SDK construction, the injected catalog port, and
/// the session record. Capability packages never construct a transport
/// (`FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` section 4,
/// `APP_SDK_INTEGRATION_SPEC.md` section 2).
library sdkwork_webserver_flutter_mobile_core;

export 'composition/composition.dart';
export 'sdk/pagination.dart';
export 'sdk/webserver_app_sdk_clients.dart';
export 'sdk/webserver_deploy_app_catalog_port.dart';
export 'session/session_store.dart';
