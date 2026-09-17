/**
 * Shared mini program chrome copy owned by `@sdkwork/webserver-mp-commons`.
 *
 * Scope: brand and the page-level navigation label — the copy the shell renders.
 * Business copy belongs to the owning capability package fragment, never here.
 */
export const webserverCommonsChromeZhCn = {
  "chrome.brand": "SDKWork Web Server",
  "chrome.navigation.label": "主导航",
  "navigation.applications": "应用",
  "chrome.host.unsupported": "当前小程序宿主暂不支持该能力。",
} as const;

export type WebserverCommonsChromeMessages = typeof webserverCommonsChromeZhCn;
