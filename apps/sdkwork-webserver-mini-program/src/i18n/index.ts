import {
  createWebserverMpMessageCatalog,
  webserverCommonsMessageSources,
  type WebserverMpLocaleTag,
  type WebserverMpMessageCatalog,
} from "@sdkwork/webserver-mp-commons";
import { webserverApplicationsMessageSources } from "@sdkwork/webserver-mp-applications";

import { webserverShellApplicationEnUs } from "./en-US/webserver/shell/application.ts";
import { webserverShellApplicationZhCn } from "./zh-CN/webserver/shell/application.ts";

/**
 * Root message catalog for the Web Server mini program.
 *
 * This module is a composition boundary: it decides the contribution order and
 * owns the resolver capability packages call, and it authors no copy itself.
 * `I18N_SPEC.md` §6.1 fixes *where* authored fragments live
 * (`<locale>/<domain>/<capability>/<fragment>`) — the fragments below are laid out
 * exactly that way, and the root's own keys are the shell-level bootstrap strings
 * that belong to `webserver/shell/application`.
 */
export const webserverMiniProgramRootMessageSources = {
  "en-US": webserverShellApplicationEnUs,
  "zh-CN": webserverShellApplicationZhCn,
} as const;

/** Capability packages contribute after commons; the root contributes last. */
export function createWebserverMiniProgramMessageCatalog(options: {
  readonly supportedLocales: readonly WebserverMpLocaleTag[];
  readonly defaultLocale: WebserverMpLocaleTag;
  readonly fallbackLocale: WebserverMpLocaleTag;
}): WebserverMpMessageCatalog {
  return createWebserverMpMessageCatalog({
    sources: [
      webserverCommonsMessageSources,
      webserverApplicationsMessageSources,
      webserverMiniProgramRootMessageSources,
    ],
    supportedLocales: options.supportedLocales,
    defaultLocale: options.defaultLocale,
    fallbackLocale: options.fallbackLocale,
  });
}

/** The resolver capability packages receive; it never throws on an unknown key. */
export type WebserverMiniProgramMessageResolver = (key: string) => string;

export function createWebserverMiniProgramMessageResolver(
  catalog: WebserverMpMessageCatalog,
  locale: string,
): WebserverMiniProgramMessageResolver {
  return (key: string) => catalog.resolve(locale, key);
}
