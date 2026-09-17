import { webserverPortalLandingEnUs } from "./en-US/infrastructure/portal/landing.ts";
import { webserverPortalLandingZhCn } from "./zh-CN/infrastructure/portal/landing.ts";
import type { PortalLocale } from "../types.ts";

export type PortalMessageKey = keyof typeof webserverPortalLandingEnUs;

/** Locale names are keyed, not literal, so the switch control authors no copy of its own. */
export type PortalLocaleNameKey = `locale.name.${PortalLocale}`;

export const portalLocaleNameKeys = {
  "en-US": "locale.name.en-US",
  "zh-CN": "locale.name.zh-CN",
} as const satisfies Record<PortalLocale, PortalLocaleNameKey>;

export const webserverPortalI18nMessages = {
  "en-US": webserverPortalLandingEnUs,
  "zh-CN": webserverPortalLandingZhCn,
} satisfies Record<PortalLocale, Record<PortalMessageKey, string>>;

