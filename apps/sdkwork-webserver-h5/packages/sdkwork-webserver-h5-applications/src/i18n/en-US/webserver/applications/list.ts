/**
 * Applications list copy owned by `@sdkwork/webserver-h5-applications`.
 *
 * Scope: the mobile `deploy_app` list screen only. Shell chrome belongs to
 * `@sdkwork/webserver-h5-commons`; another feature never reads these keys.
 */
export const webserverApplicationsListEnUs = {
  "applications.list.title": "Applications",
  "applications.list.description": "Publish, operate, and track this tenant's applications.",
  "applications.list.loading": "Loading applications…",
  "applications.list.loadingMore": "Loading more…",
  "applications.list.error": "The application list could not be loaded.",
  "applications.list.empty": "This tenant has no applications yet.",
  "applications.list.retry": "Retry",
  "applications.list.loadMore": "Load more",
  "applications.list.row.kind": "Kind",
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
  "applications.list.kind.harmonyos-app": "HarmonyOS app",
} as const;
