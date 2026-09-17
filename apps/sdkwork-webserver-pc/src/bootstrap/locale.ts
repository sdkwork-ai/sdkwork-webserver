import {
  commitWebserverLocalePreference,
  readWebserverLocalePreference,
  resolveInitialWebserverLocale,
  toWebserverLocale,
  type WebserverLocale,
  type WebserverLocaleStorage,
  type WebserverPcRuntimeConfig,
} from "@sdkwork/webserver-pc-core";

/**
 * Browser locale host adapter.
 *
 * The application shell owns runtime locale selection; this module is the only
 * place that touches `localStorage` and `navigator`, so nothing below the shell
 * reads browser language, storage, or a locale cookie on its own
 * (`I18N_SPEC.md` sections 2 and 7).
 */

/** Storage access can throw in privacy-restricted contexts, so it is probed once and may be absent. */
export function resolveBrowserLocaleStorage(): WebserverLocaleStorage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

export function resolveBrowserPreferredLocales(): readonly string[] {
  if (typeof navigator === "undefined") {
    return [];
  }
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

export function resolveBrowserInitialLocale(config: WebserverPcRuntimeConfig): WebserverLocale {
  return resolveInitialWebserverLocale(
    config,
    resolveBrowserPreferredLocales(),
    resolveBrowserLocaleStorage(),
  );
}

/**
 * Narrows the provider's live locale tag back onto a deployment locale.
 * The provider only ever resolves to an active locale, so this is a total
 * function over the runtime config's own locale set.
 */
export function narrowWebserverLocale(
  localeTag: string | null | undefined,
  config: WebserverPcRuntimeConfig,
): WebserverLocale {
  return toWebserverLocale(localeTag, config);
}

/** Records the explicit language choice so it outranks negotiation on the next load. */
export function commitBrowserLocalePreference(locale: WebserverLocale): WebserverLocale {
  return commitWebserverLocalePreference(locale, resolveBrowserLocaleStorage());
}

/**
 * Locale hint for shell copy rendered before the runtime configuration exists.
 *
 * It deliberately stops at the stored preference and the browser languages
 * rather than inventing a default: the deployment default lives in runtime
 * configuration, and the message catalog already falls back on its own.
 */
export function resolveBootstrapShellLocale(): string | undefined {
  return readWebserverLocalePreference(resolveBrowserLocaleStorage())
    ?? resolveBrowserPreferredLocales()[0];
}
