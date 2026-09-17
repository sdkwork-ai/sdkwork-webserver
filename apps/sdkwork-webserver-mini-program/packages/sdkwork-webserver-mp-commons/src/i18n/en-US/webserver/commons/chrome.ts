/**
 * Shared mini program chrome copy owned by `@sdkwork/webserver-mp-commons`.
 *
 * Scope: brand and the page-level navigation label — the copy the shell renders.
 * Business copy belongs to the owning capability package fragment, never here.
 */
export const webserverCommonsChromeEnUs = {
  "chrome.brand": "SDKWork Web Server",
  "chrome.navigation.label": "Primary navigation",
  "navigation.applications": "Applications",
  "chrome.host.unsupported": "This mini program host does not support that capability yet.",
} as const;

export type WebserverCommonsChromeMessages = typeof webserverCommonsChromeEnUs;
