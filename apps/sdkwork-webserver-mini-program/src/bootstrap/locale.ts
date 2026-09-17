/**
 * Locale negotiation for the mini program root.
 *
 * The device locale is a platform fact, so it arrives through the host adapter
 * and never from a `wx.*` call in this file. Mapping that fact onto a shipped
 * locale is the commons catalog's job; this module only decides the order to try
 * and which one wins (`I18N_SPEC.md`).
 */
import {
  normalizeWebserverMpLocale,
  type WebserverMpLocaleTag,
} from "@sdkwork/webserver-mp-commons";
import type { WebserverMpHostAdapter } from "@sdkwork/webserver-mp-host";

export interface ResolveWebserverMiniProgramLocaleOptions {
  readonly host: WebserverMpHostAdapter;
  readonly supportedLocales: readonly WebserverMpLocaleTag[];
  readonly defaultLocale: WebserverMpLocaleTag;
  readonly fallbackLocale: WebserverMpLocaleTag;
}

export interface WebserverMiniProgramLocaleResolution {
  readonly locale: WebserverMpLocaleTag;
  /** Host-reported tag, normalized to BCP 47 shape; empty when the host is silent. */
  readonly hostLanguage: string;
  readonly source: "host" | "default";
}

/**
 * Device locale first, declared default second. A host tag that maps onto no
 * shipped locale (`en_GB` when only `en-US` ships) still resolves through the
 * primary-subtag rule before giving up, so a near-miss locale does not fall back
 * to English unnecessarily.
 */
export function resolveWebserverMiniProgramLocale(
  options: ResolveWebserverMiniProgramLocaleOptions,
): WebserverMiniProgramLocaleResolution {
  const hostLanguage = options.host.getLocale().language;
  const negotiated = hostLanguage
    ? normalizeWebserverMpLocale(hostLanguage, options.supportedLocales)
    : undefined;
  if (negotiated) {
    return { hostLanguage, locale: negotiated, source: "host" };
  }
  const declared = normalizeWebserverMpLocale(options.defaultLocale, options.supportedLocales)
    ?? normalizeWebserverMpLocale(options.fallbackLocale, options.supportedLocales)
    ?? options.supportedLocales[0];
  return {
    hostLanguage,
    // The catalog always lists at least one locale; an empty set is a build error.
    locale: (declared ?? "en-US") as WebserverMpLocaleTag,
    source: "default",
  };
}
