/**
 * Cross-package message catalog for the Web Server H5 root.
 *
 * The H5 root has no React i18n runtime dependency (`@sdkwork/i18n-pc-react` is
 * a PC-only package), so commons owns a deliberately small catalog: fragments of
 * `key -> copy` records contributed per locale, merged in contribution order,
 * and resolved with an explicit fallback chain. `I18N_SPEC.md` §6.1 fixes only
 * *where* authored fragments live — `<locale>/<domain>/<capability>/<fragment>`
 * — which is exactly how the fragments below are laid out.
 *
 * This module is a thin registry boundary: it holds no authored copy, only the
 * lookup machinery every package's fragment feeds into.
 */
import { webserverCommonsChromeEnUs } from "./en-US/webserver/commons/chrome.ts";
import { webserverCommonsChromeZhCn } from "./zh-CN/webserver/commons/chrome.ts";

/** Locales the Web Server H5 surface is allowed to ship. */
export type WebserverH5LocaleTag = "en-US" | "zh-CN";

export const WEBSERVER_H5_LOCALE_TAGS: readonly WebserverH5LocaleTag[] = ["en-US", "zh-CN"];

/** One package's authored copy for one locale. */
export type WebserverH5MessageFragment = Readonly<Record<string, string>>;

/** One package's authored copy across the locales it ships. */
export type WebserverH5MessageSources = Readonly<
  Partial<Record<WebserverH5LocaleTag, WebserverH5MessageFragment>>
>;

export interface CreateWebserverH5MessageCatalogOptions {
  /** Later entries win, so the application root contributes last. */
  readonly sources: readonly WebserverH5MessageSources[];
  readonly defaultLocale?: WebserverH5LocaleTag;
  readonly fallbackLocale?: WebserverH5LocaleTag;
  readonly supportedLocales?: readonly WebserverH5LocaleTag[];
}

export interface WebserverH5MessageCatalog {
  readonly defaultLocale: WebserverH5LocaleTag;
  readonly fallbackLocale: WebserverH5LocaleTag;
  readonly supportedLocales: readonly WebserverH5LocaleTag[];
  /** True when the key resolves in the default locale. */
  has(key: string): boolean;
  /** Never throws: an unknown key resolves to the key itself, never to blank. */
  resolve(locale: string, key: string): string;
  resolveAll(locale: string): Readonly<Record<string, string>>;
}

/**
 * Map a BCP 47 candidate onto a shipped locale.
 *
 * Exact tag first (`zh-CN`), then the primary subtag (`zh` → `zh-CN`), so a
 * browser that reports `zh-Hans-CN` still lands on the shipped `zh-CN`
 * fragment instead of silently dropping to the default.
 */
export function normalizeWebserverH5Locale(
  candidate: string,
  supported: readonly WebserverH5LocaleTag[] = WEBSERVER_H5_LOCALE_TAGS,
): WebserverH5LocaleTag | undefined {
  const trimmed = candidate.trim();
  if (!trimmed) return undefined;
  const lowercased = trimmed.toLowerCase();
  const exact = supported.find((locale) => locale.toLowerCase() === lowercased);
  if (exact) return exact;
  const primary = lowercased.split("-")[0] ?? "";
  return supported.find((locale) => locale.toLowerCase().split("-")[0] === primary);
}

export function createWebserverH5MessageCatalog(
  options: CreateWebserverH5MessageCatalogOptions,
): WebserverH5MessageCatalog {
  const supportedLocales = options.supportedLocales ?? WEBSERVER_H5_LOCALE_TAGS;
  const defaultLocale = options.defaultLocale ?? "en-US";
  const fallbackLocale = options.fallbackLocale ?? defaultLocale;

  const merged = new Map<WebserverH5LocaleTag, Record<string, string>>();
  for (const locale of supportedLocales) {
    merged.set(locale, {});
  }
  for (const source of options.sources) {
    for (const locale of supportedLocales) {
      const fragment = source[locale];
      if (!fragment) continue;
      Object.assign(merged.get(locale) as Record<string, string>, fragment);
    }
  }

  const lookup = (locale: WebserverH5LocaleTag | undefined, key: string): string | undefined => {
    if (!locale) return undefined;
    return merged.get(locale)?.[key];
  };

  const resolve = (locale: string, key: string): string => {
    const requested = normalizeWebserverH5Locale(locale, supportedLocales);
    return lookup(requested, key)
      ?? lookup(fallbackLocale, key)
      ?? lookup(defaultLocale, key)
      ?? key;
  };

  return {
    defaultLocale,
    fallbackLocale,
    supportedLocales,
    has: (key) => lookup(defaultLocale, key) !== undefined,
    resolve,
    resolveAll: (locale) => {
      const requested = normalizeWebserverH5Locale(locale, supportedLocales) ?? defaultLocale;
      return {
        ...(merged.get(defaultLocale) ?? {}),
        ...(merged.get(fallbackLocale) ?? {}),
        ...(merged.get(requested) ?? {}),
      };
    },
  };
}

/**
 * This package's contribution to the root catalog. The application root spreads
 * `sources` in order (commons first, root last) when it builds the catalog, so a
 * package never reaches into another package's fragment.
 */
export const webserverCommonsMessageSources: WebserverH5MessageSources = {
  "en-US": webserverCommonsChromeEnUs,
  "zh-CN": webserverCommonsChromeZhCn,
};
