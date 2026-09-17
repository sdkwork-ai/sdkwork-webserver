import { webserverCommonsChromeEnUs } from "./en-US/webserver/commons/chrome.ts";
import { webserverCommonsChromeZhCn } from "./zh-CN/webserver/commons/chrome.ts";

/**
 * Cross-package message catalog for the Web Server mini program root.
 *
 * Authority: `I18N_SPEC.md` §6.1. A native mini program has no browser i18n
 * runtime, so commons owns a deliberately small catalog: fragments of
 * `key -> copy` records contributed per locale, merged in contribution order,
 * and resolved with an explicit fallback chain. Authored copy lives only under
 * `<locale>/<domain>/<capability>/<fragment>`; this module is a thin registry
 * boundary and authors none of it.
 */
export type WebserverMpLocaleTag = "en-US" | "zh-CN";

export const WEBSERVER_MP_LOCALE_TAGS: readonly WebserverMpLocaleTag[] = ["en-US", "zh-CN"];

/** One package's authored copy for one locale. */
export type WebserverMpMessageFragment = Readonly<Record<string, string>>;

/** One package's authored copy across the locales it ships. */
export type WebserverMpMessageSources = Readonly<
  Partial<Record<WebserverMpLocaleTag, WebserverMpMessageFragment>>
>;

export interface CreateWebserverMpMessageCatalogOptions {
  /** Later entries win, so the application root contributes last. */
  readonly sources: readonly WebserverMpMessageSources[];
  readonly defaultLocale?: WebserverMpLocaleTag;
  readonly fallbackLocale?: WebserverMpLocaleTag;
  readonly supportedLocales?: readonly WebserverMpLocaleTag[];
}

export interface WebserverMpMessageCatalog {
  readonly defaultLocale: WebserverMpLocaleTag;
  readonly fallbackLocale: WebserverMpLocaleTag;
  readonly supportedLocales: readonly WebserverMpLocaleTag[];
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
 * device that reports `zh-Hans-CN` still lands on the shipped `zh-CN` fragment
 * instead of silently dropping to the default.
 */
export function normalizeWebserverMpLocale(
  candidate: string,
  supported: readonly WebserverMpLocaleTag[] = WEBSERVER_MP_LOCALE_TAGS,
): WebserverMpLocaleTag | undefined {
  const trimmed = candidate.trim();
  if (!trimmed) return undefined;
  const lowercased = trimmed.toLowerCase();
  const exact = supported.find((locale) => locale.toLowerCase() === lowercased);
  if (exact) return exact;
  const primary = lowercased.split("-")[0] ?? "";
  return supported.find((locale) => locale.toLowerCase().split("-")[0] === primary);
}

export function createWebserverMpMessageCatalog(
  options: CreateWebserverMpMessageCatalogOptions,
): WebserverMpMessageCatalog {
  const supportedLocales = options.supportedLocales ?? WEBSERVER_MP_LOCALE_TAGS;
  const defaultLocale = options.defaultLocale ?? "en-US";
  const fallbackLocale = options.fallbackLocale ?? defaultLocale;

  const merged = new Map<WebserverMpLocaleTag, Record<string, string>>();
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

  const lookup = (locale: WebserverMpLocaleTag | undefined, key: string): string | undefined => {
    if (!locale) return undefined;
    return merged.get(locale)?.[key];
  };

  return {
    defaultLocale,
    fallbackLocale,
    supportedLocales,
    has: (key) => lookup(defaultLocale, key) !== undefined,
    resolve: (locale, key) => lookup(normalizeWebserverMpLocale(locale, supportedLocales), key)
      ?? lookup(fallbackLocale, key)
      ?? lookup(defaultLocale, key)
      ?? key,
    resolveAll: (locale) => {
      const requested = normalizeWebserverMpLocale(locale, supportedLocales) ?? defaultLocale;
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
export const webserverCommonsMessageSources: WebserverMpMessageSources = {
  "en-US": webserverCommonsChromeEnUs,
  "zh-CN": webserverCommonsChromeZhCn,
};
