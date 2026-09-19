/// Package-local default copy for the applications capability.
///
/// Authority: `I18N_SPEC.md` section 6.1 — authored copy is split by locale,
/// domain, capability, and screen fragment, and platform monolithic resources
/// are assembled from these fragments rather than hand-authored as a whole-root
/// catalog.
///
/// Placement note: Flutter authored fragments use `.arb`/`.json` under
/// `lib/src/i18n/<locale>/<domain>/<capability>/`. Dart `const` maps are not an
/// authored fragment format, so they are classified as code-level defaults and
/// live outside `lib/src/i18n/` until the `gen_l10n` projection (which needs the
/// Flutter toolchain) replaces them.
///
/// Keys deliberately match the H5, mini program, and HarmonyOS roots'
/// `applications.list.*` set so all four clients stay word-for-word aligned.
library;

/// One authored `value -> messageKey` pair from a closed server enum.
class WebserverFlutterEnumLabel {
  const WebserverFlutterEnumLabel({
    required this.value,
    required this.messageKey,
  });

  final String value;
  final String messageKey;
}

/// Closed `deploy_app` kind labels.
///
/// One entry per line and per pair, so
/// `tests/flutter-surface-contract.test.mjs` can re-derive the covered value set
/// from this file and assert it equals the generated deployments SDK's `AppKind`
/// union. A new server enum member therefore cannot reach the UI as a raw token.
const List<WebserverFlutterEnumLabel> webserverFlutterApplicationKindLabels =
    <WebserverFlutterEnumLabel>[
  WebserverFlutterEnumLabel(
    value: 'STATIC_WEB',
    messageKey: 'applications.list.kind.static-web',
  ),
  WebserverFlutterEnumLabel(
    value: 'SPA_WEB',
    messageKey: 'applications.list.kind.spa-web',
  ),
  WebserverFlutterEnumLabel(
    value: 'API_SERVICE',
    messageKey: 'applications.list.kind.api-service',
  ),
  WebserverFlutterEnumLabel(
    value: 'WECHAT_MINIPROGRAM',
    messageKey: 'applications.list.kind.wechat-miniprogram',
  ),
  WebserverFlutterEnumLabel(
    value: 'DOUYIN_MINIPROGRAM',
    messageKey: 'applications.list.kind.douyin-miniprogram',
  ),
  WebserverFlutterEnumLabel(
    value: 'IOS_APP',
    messageKey: 'applications.list.kind.ios-app',
  ),
  WebserverFlutterEnumLabel(
    value: 'ANDROID_APP',
    messageKey: 'applications.list.kind.android-app',
  ),
  WebserverFlutterEnumLabel(
    value: 'HARMONYOS_APP',
    messageKey: 'applications.list.kind.harmonyos-app',
  ),
  WebserverFlutterEnumLabel(
    value: 'DESKTOP_APP',
    messageKey: 'applications.list.kind.desktop-app',
  ),
];

/// Closed `deploy_app` status labels.
const List<WebserverFlutterEnumLabel> webserverFlutterApplicationStatusLabels =
    <WebserverFlutterEnumLabel>[
  WebserverFlutterEnumLabel(
    value: 'DRAFT',
    messageKey: 'applications.list.status.draft',
  ),
  WebserverFlutterEnumLabel(
    value: 'READY',
    messageKey: 'applications.list.status.ready',
  ),
  WebserverFlutterEnumLabel(
    value: 'ACTIVE',
    messageKey: 'applications.list.status.active',
  ),
  WebserverFlutterEnumLabel(
    value: 'PAUSED',
    messageKey: 'applications.list.status.paused',
  ),
  WebserverFlutterEnumLabel(
    value: 'ARCHIVED',
    messageKey: 'applications.list.status.archived',
  ),
  WebserverFlutterEnumLabel(
    value: 'FAILED',
    messageKey: 'applications.list.status.failed',
  ),
];

const Map<String, String> webserverFlutterApplicationsMessagesEnUs =
    <String, String>{
  'applications.list.title': 'Applications',
  'applications.list.description':
      "Publish, operate, and track this tenant's applications.",
  'applications.list.loading': 'Loading applications…',
  'applications.list.loadingMore': 'Loading more…',
  'applications.list.error': 'Loading the application list failed.',
  'applications.list.unavailable':
      'This build has no transport for the application catalog yet.',
  'applications.list.empty': 'This tenant has no applications yet.',
  'applications.list.retry': 'Retry',
  'applications.list.loadMore': 'Load more',
  'applications.list.row.kind': 'App kind',
  'applications.list.row.release': 'Latest release',
  'applications.list.row.environment': 'Default environment',
  'applications.list.row.platformTargets': 'Platform targets',
  'applications.list.row.unknown': 'Unknown',
  'applications.list.kind.static-web': 'Static site',
  'applications.list.kind.spa-web': 'Single-page app',
  'applications.list.kind.api-service': 'API service',
  'applications.list.kind.wechat-miniprogram': 'WeChat mini program',
  'applications.list.kind.douyin-miniprogram': 'Douyin mini program',
  'applications.list.kind.ios-app': 'iOS app',
  'applications.list.kind.android-app': 'Android app',
  'applications.list.kind.harmonyos-app': 'HarmonyOS app',
  'applications.list.kind.desktop-app': 'Desktop app',
  'applications.list.status.draft': 'Draft',
  'applications.list.status.ready': 'Ready',
  'applications.list.status.active': 'Active',
  'applications.list.status.paused': 'Paused',
  'applications.list.status.archived': 'Archived',
  'applications.list.status.failed': 'Failed',
  'navigation.applications': 'Applications',
};

const Map<String, String> webserverFlutterApplicationsMessagesZhCn =
    <String, String>{
  'applications.list.title': '应用',
  'applications.list.description': '发布、运维并追踪本租户的应用。',
  'applications.list.loading': '正在加载应用…',
  'applications.list.loadingMore': '正在加载更多…',
  'applications.list.error': '应用列表加载失败。',
  'applications.list.unavailable': '此构建尚未接入应用目录的传输通道。',
  'applications.list.empty': '当前租户还没有应用。',
  'applications.list.retry': '重试',
  'applications.list.loadMore': '加载更多',
  'applications.list.row.kind': '应用类型',
  'applications.list.row.release': '最新版本',
  'applications.list.row.environment': '默认环境',
  'applications.list.row.platformTargets': '平台目标数量',
  'applications.list.row.unknown': '未知',
  'applications.list.kind.static-web': '静态站点',
  'applications.list.kind.spa-web': '单页应用',
  'applications.list.kind.api-service': 'API 服务',
  'applications.list.kind.wechat-miniprogram': '微信小程序',
  'applications.list.kind.douyin-miniprogram': '抖音小程序',
  'applications.list.kind.ios-app': 'iOS 应用',
  'applications.list.kind.android-app': 'Android 应用',
  'applications.list.kind.harmonyos-app': '鸿蒙应用',
  'applications.list.kind.desktop-app': '桌面应用',
  'applications.list.status.draft': '草稿',
  'applications.list.status.ready': '待发布',
  'applications.list.status.active': '运行中',
  'applications.list.status.paused': '已暂停',
  'applications.list.status.archived': '已归档',
  'applications.list.status.failed': '失败',
  'navigation.applications': '应用',
};
