import {
  loadWebserverH5RuntimeConfig,
  type WebserverH5RuntimeConfig,
} from "@sdkwork/webserver-h5-core/sdk";
import type { WebserverH5LocaleTag } from "@sdkwork/webserver-h5-commons";

import {
  commitWebserverH5LocalePreference,
  currentBrowserLanguages,
  readWebserverH5LocalePreference,
  resolveInitialWebserverH5Locale,
} from "./locale.ts";
import { createWebserverH5SdkClients, type WebserverH5SdkClients } from "./sdkClients.ts";

export interface WebserverH5Bootstrap {
  readonly clients: WebserverH5SdkClients;
  readonly config: WebserverH5RuntimeConfig;
  readonly locale: WebserverH5LocaleTag;
}

export interface BootstrapWebserverH5Options {
  readonly fetcher?: typeof fetch;
  readonly preferredLocales?: readonly string[];
  readonly storedLocale?: string | undefined;
  /** Skips persisting the negotiated locale; used by tests. */
  readonly persistLocale?: boolean;
}

/**
 * Boot the H5 root.
 *
 * Order matters: the runtime configuration is the authority for which locales
 * may be negotiated, so configuration is loaded (and validated) **before** a
 * locale is chosen, and the SDK clients are built from the resolved base URLs so
 * no client can ever be created against an unresolved `"/"`.
 */
export async function bootstrapWebserverH5(
  options: BootstrapWebserverH5Options = {},
): Promise<WebserverH5Bootstrap> {
  const config = await loadWebserverH5RuntimeConfig(options.fetcher ?? fetch);
  const locale = resolveInitialWebserverH5Locale({
    defaultLocale: config.defaultLocale,
    preferredLocales: options.preferredLocales ?? currentBrowserLanguages(),
    storedPreference: options.storedLocale ?? readWebserverH5LocalePreference(),
    supportedLocales: config.supportedLocales,
  });
  if (options.persistLocale !== false) {
    commitWebserverH5LocalePreference(locale);
  }
  return {
    clients: createWebserverH5SdkClients(config),
    config,
    locale,
  };
}
