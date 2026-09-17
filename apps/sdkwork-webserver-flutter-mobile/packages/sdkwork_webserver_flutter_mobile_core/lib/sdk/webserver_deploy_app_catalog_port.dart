/// The `deploy_app` catalog port.
///
/// The applications catalog in every SDKWork client root reads the
/// **deployments** app API (`deploy.apps.list`), which is the authority the PC
/// console mounts through
/// `@sdkwork/deployments-pc-console-publishing`. The vocabulary below is
/// therefore the deployments one, re-stated here so capability packages can
/// name the values they render without deep-importing generated transport
/// modules.
library;

import 'pagination.dart';

/// `sdkwork-deployments/.../types/app-kind.ts` — the generated union, verbatim.
const List<String> webserverDeployAppKinds = <String>[
  'STATIC_WEB',
  'SPA_WEB',
  'API_SERVICE',
  'WECHAT_MINIPROGRAM',
  'DOUYIN_MINIPROGRAM',
  'IOS_APP',
  'ANDROID_APP',
  'HARMONYOS_APP',
];

/// `sdkwork-deployments/.../types/app-status.ts` — the generated union, verbatim.
const List<String> webserverDeployAppStatuses = <String>[
  'DRAFT',
  'READY',
  'ACTIVE',
  'PAUSED',
  'ARCHIVED',
  'FAILED',
];

/// One `deploy_app` record, field-for-field the generated `AppResponse`.
class WebserverDeployAppRecord {
  const WebserverDeployAppRecord({
    required this.id,
    required this.name,
    required this.slug,
    required this.appKind,
    required this.appStatus,
    required this.description,
    required this.defaultEnvironment,
    required this.latestReleaseTag,
    required this.platformTargetCount,
    required this.updatedAt,
  });

  final String id;
  final String name;
  final String slug;
  final String appKind;
  final String appStatus;
  final String description;
  final String defaultEnvironment;
  final String latestReleaseTag;
  final int platformTargetCount;
  final String updatedAt;
}

/// One page of `deploy_app` records plus the server-issued page info.
class WebserverDeployAppPage {
  const WebserverDeployAppPage({required this.items, required this.pageInfo});

  final List<WebserverDeployAppRecord> items;
  final WebserverFlutterPageInfoLike? pageInfo;
}

/// The single generated operation this root consumes.
///
/// Named as a slice rather than as a whole client so a screen cannot reach for
/// a create/update/delete operation it never declared.
///
/// NOTE (Dart is nominal, not structural): a TypeScript root can hand the
/// generated client straight through because the slice is satisfied
/// structurally. Dart requires a nominal `implements`, and generated code will
/// never declare it. When a Dart deployments SDK is generated, a thin adapter
/// in **this** package — not in a capability package — has to bridge it. The
/// port therefore marks an adapter seam, not a zero-cost passthrough.
abstract interface class WebserverDeployAppCatalogReader {
  Future<WebserverDeployAppPage> list({
    required int page,
    required int pageSize,
  });
}

/// Raised when the catalog is read without a bound reader.
///
/// Deliberately an error rather than an empty page: `resolve`-ing an empty list
/// would tell the screen "this tenant has no applications", which is a claim
/// this root cannot make. "The transport is missing" is a state the screen
/// renders explicitly.
class WebserverDeployAppCatalogUnavailableError implements Exception {
  const WebserverDeployAppCatalogUnavailableError(this.detail);

  static const String code = 'deploy-app-sdk-unavailable';

  final String detail;

  @override
  String toString() =>
      'WebserverDeployAppCatalogUnavailableError($code): $detail';
}

/// The binding capability packages receive.
class WebserverDeployAppCatalogPort {
  WebserverDeployAppCatalogPort({WebserverDeployAppCatalogReader? reader})
    : _reader = reader;

  /// Mirrors the runtime target the deployment profiles declare.
  static const String platform = 'flutter-android';

  WebserverDeployAppCatalogReader? _reader;

  bool get available => _reader != null;

  WebserverDeployAppCatalogReader get reader {
    final reader = _reader;
    if (reader == null) {
      throw const WebserverDeployAppCatalogUnavailableError(
        'no generated Dart deployments app SDK exists in this workspace, so the '
        'deploy_app catalog reader has no binding',
      );
    }
    return reader;
  }

  void dispose() {
    _reader = null;
  }
}
