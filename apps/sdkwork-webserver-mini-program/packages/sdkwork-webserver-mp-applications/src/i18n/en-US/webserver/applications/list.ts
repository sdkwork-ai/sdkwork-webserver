/**
 * Applications list copy owned by `@sdkwork/webserver-mp-applications`.
 *
 * Scope: the mini program `deploy_app` list screen only. Shell chrome belongs to
 * `@sdkwork/webserver-mp-commons`; another capability never reads these keys.
 * Keys deliberately match the H5 root's `applications.list.*` set so both mobile
 * clients stay word-for-word aligned.
 */
export const webserverApplicationsListEnUs = {
  "applications.list.title": "Applications",
  "applications.list.description": "Publish, operate, and track this tenant's applications.",
  "applications.list.loading": "Loading applications…",
  "applications.list.loadingMore": "Loading more…",
  "applications.list.error": "Loading the application list failed.",
  "applications.list.empty": "This tenant has no applications yet.",
  "applications.list.retry": "Retry",
  "applications.list.loadMore": "Load more",
  "applications.list.row.kind": "App kind",
  "applications.list.row.release": "Latest release",
  "applications.list.row.environment": "Default environment",
  "applications.list.row.platformTargets": "Platform targets",
  "applications.list.row.open": "Open details",
  "applications.list.status.draft": "Draft",
  "applications.list.status.ready": "Ready",
  "applications.list.status.active": "Active",
  "applications.list.status.paused": "Paused",
  "applications.list.status.archived": "Archived",
  "applications.list.status.failed": "Failed",
  "applications.list.kind.static-web": "Static site",
  "applications.list.kind.spa-web": "Single-page app",
  "applications.list.kind.api-service": "API service",
  "applications.list.kind.wechat-miniprogram": "WeChat mini program",
  "applications.list.kind.douyin-miniprogram": "Douyin mini program",
  "applications.list.kind.ios-app": "iOS app",
  "applications.list.kind.android-app": "Android app",
  "applications.list.kind.desktop-app": "Desktop app",
  "applications.list.kind.harmonyos-app": "HarmonyOS app",
} as const;
