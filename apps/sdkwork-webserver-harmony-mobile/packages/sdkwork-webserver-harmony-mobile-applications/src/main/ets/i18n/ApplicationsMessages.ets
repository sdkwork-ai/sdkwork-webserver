/**
 * Package-local locale fragments for the applications capability.
 *
 * Authority: `I18N_SPEC.md` §6.1 — authored copy is split by locale, domain,
 * capability, and screen fragment, and platform monolithic resources are
 * assembled from these fragments rather than hand-authored as a whole-root
 * catalog. Keys deliberately match the H5, mini program, and HarmonyOS roots'
 * `applications.list.*` set so all four clients stay word-for-word aligned.
 */

/** One authored `value -> messageKey` pair from a closed server enum. */
export interface WebserverHarmonyEnumLabel {
  readonly value: string;
  readonly messageKey: string;
}

/**
 * Closed `deploy_app` kind labels.
 *
 * Each entry is one line and one pair, so
 * `tests/harmony-surface-contract.test.mjs` can re-derive the covered value set
 * from this file and assert it equals the generated SDK's `AppKind` union. A new
 * server enum member therefore cannot reach the UI as a raw token.
 */
export const WEBSERVER_HARMONY_APPLICATION_KIND_LABELS: WebserverHarmonyEnumLabel[] = [
  { value: 'STATIC_WEB', messageKey: 'applications.list.kind.static-web' },
  { value: 'SPA_WEB', messageKey: 'applications.list.kind.spa-web' },
  { value: 'API_SERVICE', messageKey: 'applications.list.kind.api-service' },
  { value: 'WECHAT_MINIPROGRAM', messageKey: 'applications.list.kind.wechat-miniprogram' },
  { value: 'DOUYIN_MINIPROGRAM', messageKey: 'applications.list.kind.douyin-miniprogram' },
  { value: 'IOS_APP', messageKey: 'applications.list.kind.ios-app' },
  { value: 'ANDROID_APP', messageKey: 'applications.list.kind.android-app' },
  { value: 'HARMONYOS_APP', messageKey: 'applications.list.kind.harmonyos-app' },
  { value: 'DESKTOP_APP', messageKey: 'applications.list.kind.desktop-app' },
];

/** Closed `deploy_app` status labels. */
export const WEBSERVER_HARMONY_APPLICATION_STATUS_LABELS: WebserverHarmonyEnumLabel[] = [
  { value: 'DRAFT', messageKey: 'applications.list.status.draft' },
  { value: 'READY', messageKey: 'applications.list.status.ready' },
  { value: 'ACTIVE', messageKey: 'applications.list.status.active' },
  { value: 'PAUSED', messageKey: 'applications.list.status.paused' },
  { value: 'ARCHIVED', messageKey: 'applications.list.status.archived' },
  { value: 'FAILED', messageKey: 'applications.list.status.failed' },
];

/**
 * Resolve a closed-set token onto its message key.
 *
 * Lives in `models/ApplicationRecordMapping.ets`, which receives these label
 * tables as arguments — that keeps the mapping a pure, import-free function the
 * repository's Node-side contract test can execute.
 */

export const webserverHarmonyApplicationsMessagesEnUs: Map<string, string> = new Map<
  string,
  string
>([
  ['applications.list.title', 'Applications'],
  ['applications.list.description', "Publish, operate, and track this tenant's applications."],
  ['applications.list.loading', 'Loading applications…'],
  ['applications.list.loadingMore', 'Loading more…'],
  ['applications.list.error', 'Loading the application list failed.'],
  ['applications.list.empty', 'This tenant has no applications yet.'],
  ['applications.list.retry', 'Retry'],
  ['applications.list.loadMore', 'Load more'],
  ['applications.list.row.kind', 'App kind'],
  ['applications.list.row.release', 'Latest release'],
  ['applications.list.row.environment', 'Default environment'],
  ['applications.list.row.platformTargets', 'Platform targets'],
  ['applications.list.row.unknown', 'Unknown'],
  ['applications.list.kind.static-web', 'Static site'],
  ['applications.list.kind.spa-web', 'Single-page app'],
  ['applications.list.kind.api-service', 'API service'],
  ['applications.list.kind.wechat-miniprogram', 'WeChat mini program'],
  ['applications.list.kind.douyin-miniprogram', 'Douyin mini program'],
  ['applications.list.kind.ios-app', 'iOS app'],
  ['applications.list.kind.android-app', 'Android app'],
  ['applications.list.kind.harmonyos-app', 'HarmonyOS app'],
  ['applications.list.kind.desktop-app', 'Desktop app'],
  ['applications.list.status.draft', 'Draft'],
  ['applications.list.status.ready', 'Ready'],
  ['applications.list.status.active', 'Active'],
  ['applications.list.status.paused', 'Paused'],
  ['applications.list.status.archived', 'Archived'],
  ['applications.list.status.failed', 'Failed'],
  ['navigation.applications', 'Applications'],
]);

export const webserverHarmonyApplicationsMessagesZhCn: Map<string, string> = new Map<
  string,
  string
>([
  ['applications.list.title', '应用'],
  ['applications.list.description', '发布、运维并追踪本租户的应用。'],
  ['applications.list.loading', '正在加载应用…'],
  ['applications.list.loadingMore', '正在加载更多…'],
  ['applications.list.error', '应用列表加载失败。'],
  ['applications.list.empty', '当前租户还没有应用。'],
  ['applications.list.retry', '重试'],
  ['applications.list.loadMore', '加载更多'],
  ['applications.list.row.kind', '应用类型'],
  ['applications.list.row.release', '最新版本'],
  ['applications.list.row.environment', '默认环境'],
  ['applications.list.row.platformTargets', '平台目标数量'],
  ['applications.list.row.unknown', '未知'],
  ['applications.list.kind.static-web', '静态站点'],
  ['applications.list.kind.spa-web', '单页应用'],
  ['applications.list.kind.api-service', 'API 服务'],
  ['applications.list.kind.wechat-miniprogram', '微信小程序'],
  ['applications.list.kind.douyin-miniprogram', '抖音小程序'],
  ['applications.list.kind.ios-app', 'iOS 应用'],
  ['applications.list.kind.android-app', 'Android 应用'],
  ['applications.list.kind.harmonyos-app', '鸿蒙应用'],
  ['applications.list.kind.desktop-app', '桌面应用'],
  ['applications.list.status.draft', '草稿'],
  ['applications.list.status.ready', '待发布'],
  ['applications.list.status.active', '运行中'],
  ['applications.list.status.paused', '已暂停'],
  ['applications.list.status.archived', '已归档'],
  ['applications.list.status.failed', '失败'],
  ['navigation.applications', '应用'],
]);
