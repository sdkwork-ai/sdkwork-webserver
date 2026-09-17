/**
 * Shared mobile chrome copy owned by `@sdkwork/webserver-h5-commons`.
 *
 * Scope: brand, tab-bar landmark, and tab labels — the copy the shell renders.
 * Business copy belongs to the owning feature package fragment, never here.
 */
export const webserverCommonsChromeZhCn = {
  "chrome.brand": "SDKWork Web Server",
  "chrome.navigation.label": "主导航",
  "navigation.applications": "应用",
} as const;

export type WebserverCommonsChromeMessages = typeof webserverCommonsChromeZhCn;
