import {
  createSdkworkMessageCatalog,
  defineSdkworkI18nRuntimeConfig,
  type SdkworkI18nRuntimeConfig,
} from "@sdkwork/i18n-pc-react";
import type { WebserverPcRuntimeConfig } from "@sdkwork/webserver-pc-core";
import { webserverShellApplicationEnUs } from "./en-US/webserver/shell/application.ts";
import { webserverShellApplicationZhCn } from "./zh-CN/webserver/shell/application.ts";

/**
 * Namespace for the Web Server PC application-shell catalog.
 *
 * Feature packages keep their own fragments and namespaces; this catalog only
 * carries copy the browser host itself renders, such as bootstrap status and
 * runtime-configuration failure.
 */
export const WEBSERVER_APPLICATION_I18N_NAMESPACE = "webserver-shell";

/**
 * The structural default is `en-US` because every deployment profile declares
 * `fallbackLocale: en-US`, so an unmatched locale falls back the same way here
 * as it does through the runtime locale chain.
 */
export const webserverApplicationCatalog = createSdkworkMessageCatalog({
  defaultLocale: "en-US",
  locales: {
    "en-US": webserverShellApplicationEnUs,
    "zh-CN": webserverShellApplicationZhCn,
  },
  namespace: WEBSERVER_APPLICATION_I18N_NAMESPACE,
});

export type WebserverApplicationMessage = ReturnType<
  typeof webserverApplicationCatalog.resolveMessages
>;

/**
 * Projects the deployment runtime configuration onto the SDKWork i18n runtime
 * config so locale strategy stays a deployment concern and the supported,
 * active, default, and fallback locale set is validated once at bootstrap
 * (`I18N_SPEC.md` section 12).
 */
export function createWebserverI18nRuntimeConfig(
  config: WebserverPcRuntimeConfig,
): SdkworkI18nRuntimeConfig {
  return defineSdkworkI18nRuntimeConfig({
    activeLocales: config.activeLocales,
    defaultLocale: config.defaultLocale,
    fallbackLocale: config.fallbackLocale,
    loadingStrategy: "eager-core-lazy-feature",
    supportedLocales: config.supportedLocales,
  });
}
