/**
 * Shared mobile chrome copy owned by `@sdkwork/webserver-h5-commons`.
 *
 * Scope: brand, tab-bar landmark, and tab labels — the copy the shell renders.
 * Business copy belongs to the owning feature package fragment, never here.
 */
export const webserverCommonsChromeEnUs = {
  "chrome.brand": "SDKWork Web Server",
  "chrome.navigation.label": "Primary navigation",
  "navigation.applications": "Applications",
} as const;

export type WebserverCommonsChromeMessages = typeof webserverCommonsChromeEnUs;
