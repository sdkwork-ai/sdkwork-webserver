/**
 * Package-local i18n helpers.
 *
 * Authority: `I18N_SPEC.md` §6.1 — authored copy is split by locale, domain,
 * capability, and screen fragment, and platform monolithic resources are
 * assembled from those fragments rather than hand-authored as a whole-root
 * catalog. This module owns only the tag normalization and the fragment shapes.
 */

export type WebserverHarmonyLocale = 'en-US' | 'zh-CN';

export const WEBSERVER_HARMONY_LOCALE_TAGS: WebserverHarmonyLocale[] = ['en-US', 'zh-CN'];

/** One package's authored copy for one locale. */
export type WebserverHarmonyMessageFragment = Map<string, string>;

/** One package's authored copy across the locales it ships. */
export interface WebserverHarmonyMessageSources {
  readonly enUs?: WebserverHarmonyMessageFragment;
  readonly zhCn?: WebserverHarmonyMessageFragment;
}

/**
 * Map a BCP 47 candidate onto a shipped locale.
 *
 * Exact tag first (`zh-CN`), then the primary subtag (`zh` → `zh-CN`), so a device
 * reporting `zh-Hans-CN` still lands on the shipped `zh-CN` fragment instead of
 * silently dropping to the default.
 */
export function normalizeWebserverHarmonyLocale(
  candidate: string,
  supported: WebserverHarmonyLocale[] = WEBSERVER_HARMONY_LOCALE_TAGS,
): WebserverHarmonyLocale | undefined {
  const trimmed: string = candidate.trim();
  if (trimmed.length === 0) {
    return undefined;
  }
  const lowercased: string = trimmed.toLowerCase();
  for (const locale of supported) {
    if (locale.toLowerCase() === lowercased) {
      return locale;
    }
  }
  const primary: string = lowercased.split('-')[0];
  for (const locale of supported) {
    if (locale.toLowerCase().split('-')[0] === primary) {
      return locale;
    }
  }
  return undefined;
}

/** Normalize the platform's `zh_CN`-style tag onto a BCP 47 candidate. */
export function normalizeWebserverHarmonyPlatformLocale(language: string | undefined): string {
  const trimmed: string = (language ?? '').trim();
  if (trimmed.length === 0) {
    return '';
  }
  const parts: string[] = trimmed.replace(/_/gu, '-').split('-');
  const primary: string = (parts[0] ?? '').toLowerCase();
  if (primary.length === 0) {
    return '';
  }
  const region: string = parts.length > 1 ? (parts[1] ?? '').toUpperCase() : '';
  return region.length > 0 ? `${primary}-${region}` : primary;
}
