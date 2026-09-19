import {
  loadWebserverH5RuntimeConfig,
  type WebserverH5RuntimeConfig,
} from "@sdkwork/webserver-h5-core/sdk";
import { readBootstrapAccessTokenFromProcessEnv } from "@sdkwork/iam-credential-entry";
import { resetTokenManagerToBootstrapAccessToken } from "@sdkwork/iam-runtime";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
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
  readonly tokenManager: AuthTokenManager;
}

export interface BootstrapWebserverH5Options {
  readonly fetcher?: typeof fetch;
  readonly preferredLocales?: readonly string[];
  readonly storedLocale?: string | undefined;
  /** Skips persisting the negotiated locale; used by tests. */
  readonly persistLocale?: boolean;
}

/**
 * Seed the shared TokenManager with the credential-entry bootstrap credential
 * (`IAM_CREDENTIAL_ENTRY_SPEC.md` section 5).
 *
 * Without this the applications screen dispatches no request at all: the
 * generated transport fails before network dispatch when the TokenManager holds
 * no access token, and the list reports a load failure whose retry can never
 * succeed. The PC root performs the same step, so the two surfaces boot alike.
 *
 * The guard mirrors the PC root deliberately. `resetTokenManagerToBootstrapAccessToken`
 * clears every token before it writes, so calling it unconditionally would discard
 * a restored user session and leave only the development bootstrap credential.
 * A configured session therefore wins, and the bootstrap credential is used only
 * when there is no session to preserve.
 */
export function initializeWebserverH5BootstrapAccessToken(
  tokenManager: AuthTokenManager,
  readBootstrapToken: () => string | undefined = readBootstrapAccessTokenFromProcessEnv,
): void {
  if (tokenManager.hasAuthToken()) {
    return;
  }
  const bootstrapAccessToken = readBootstrapToken();
  if (!bootstrapAccessToken) {
    // No session and no injected credential: stay unbootstrapped rather than
    // clearing anything. The screen reports the missing credential instead of a
    // misleading network failure.
    return;
  }
  resetTokenManagerToBootstrapAccessToken(tokenManager, bootstrapAccessToken);
}

/**
 * Boot the H5 root.
 *
 * Order matters: the runtime configuration is the authority for which locales
 * may be negotiated, so configuration is loaded (and validated) **before** a
 * locale is chosen, and the SDK clients are built from the resolved base URLs so
 * no client can ever be created against an unresolved `"/"`.
 *
 * The token manager is seeded before the clients are built, so the very first
 * request a screen dispatches already carries the bootstrap credential.
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
  const clients = createWebserverH5SdkClients(config);
  initializeWebserverH5BootstrapAccessToken(clients.tokenManager);
  return {
    clients,
    config,
    locale,
    tokenManager: clients.tokenManager,
  };
}
