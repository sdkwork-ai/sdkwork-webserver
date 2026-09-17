/**
 * Applications list copy owned by `@sdkwork/webserver-h5-applications`.
 *
 * Scope: the mobile `deploy_app` list screen only. Shell chrome belongs to
 * `@sdkwork/webserver-h5-commons`; another feature never reads these keys.
 */
export const webserverApplicationsListZhCn = {
  "applications.list.title": "应用",
  "applications.list.description": "发布、运维并追踪本租户的应用。",
  "applications.list.loading": "正在加载应用…",
  "applications.list.loadingMore": "正在加载更多…",
  "applications.list.error": "应用列表加载失败。",
  "applications.list.empty": "当前租户还没有应用。",
  "applications.list.retry": "重试",
  "applications.list.loadMore": "加载更多",
  "applications.list.row.kind": "应用类型",
  "applications.list.row.release": "最新版本",
  "applications.list.row.environment": "默认环境",
  "applications.list.row.platformTargets": "平台目标数量",
  "applications.list.row.open": "查看详情",
  "applications.list.status.draft": "草稿",
  "applications.list.status.ready": "待发布",
  "applications.list.status.active": "运行中",
  "applications.list.status.paused": "已暂停",
  "applications.list.status.archived": "已归档",
  "applications.list.status.failed": "失败",
  "applications.list.kind.static-web": "静态站点",
  "applications.list.kind.spa-web": "单页应用",
  "applications.list.kind.api-service": "API 服务",
  "applications.list.kind.wechat-miniprogram": "微信小程序",
  "applications.list.kind.douyin-miniprogram": "抖音小程序",
  "applications.list.kind.ios-app": "iOS 应用",
  "applications.list.kind.android-app": "Android 应用",
  "applications.list.kind.harmonyos-app": "鸿蒙应用",
} as const;
