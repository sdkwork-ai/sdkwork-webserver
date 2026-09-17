/**
 * Locale bootstrap for the Web Server H5 root.
 *
 * The storage key is deliberately identical to the PC console's
 * (`sdkwork.webserver.locale`) so a viewer who picks a language on one surface
 * keeps it on the other when both are served from the same origin. Negotiation
 * follows `I18N_SPEC.md` §4: the **standard** language signals only — a stored
 * preference wins, then `navigator.languages` — and never a SDKWork-invented
 * request header.
 */
import type { WebserverH5LocaleTag } from "@sdkwork/webserver-h5-commons";
import { normalizeWebserverH5Locale } from "@sdkwork/webserver-h5-commons";

export const WEBSERVER_H5_LOCALE_STORAGE_KEY = "sdkwork.webserver.locale";

export interface WebserverH5LocaleStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function browserLocaleStorage(): WebserverH5LocaleStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    // Storage can be blocked (private mode, sandboxed iframe); a missing
    // preference simply falls through to browser-language negotiation.
    return undefined;
  }
}

export function readWebserverH5LocalePreference(
  storage: WebserverH5LocaleStorage | undefined = browserLocaleStorage(),
): string | undefined {
  try {
    return storage?.getItem(WEBSERVER_H5_LOCALE_STORAGE_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

export function commitWebserverH5LocalePreference(
  locale: WebserverH5LocaleTag,
  storage: WebserverH5LocaleStorage | undefined = browserLocaleStorage(),
): void {
  try {
    storage?.setItem(WEBSERVER_H5_LOCALE_STORAGE_KEY, locale);
  } catch {
    // A viewer without writable storage still gets the negotiated locale for
    // this session; refusing to boot over it would be worse than not persisting.
  }
}

/**
 * Resolve the locale to boot with. Order: stored preference, then the browser's
 * ordered language list, then the deployment default.
 */
export function resolveInitialWebserverH5Locale(options: {
  readonly supportedLocales: readonly WebserverH5LocaleTag[];
  readonly defaultLocale: WebserverH5LocaleTag;
  readonly preferredLocales?: readonly string[];
  readonly storedPreference?: string | undefined;
}): WebserverH5LocaleTag {
  const { defaultLocale, preferredLocales = [], storedPreference, supportedLocales } = options;
  const candidates = [
    ...(storedPreference ? [storedPreference] : []),
    ...preferredLocales,
  ];
  for (const candidate of candidates) {
    const locale = normalizeWebserverH5Locale(candidate, supportedLocales);
    if (locale) return locale;
  }
  return normalizeWebserverH5Locale(defaultLocale, supportedLocales) ?? defaultLocale;
}

/** Browser-reported preferences, newest signal first. */
export function currentBrowserLanguages(): readonly string[] {
  const languages = globalThis.navigator?.languages;
  if (Array.isArray(languages) && languages.length > 0) return languages;
  const primary = globalThis.navigator?.language;
  return primary ? [primary] : [];
}
