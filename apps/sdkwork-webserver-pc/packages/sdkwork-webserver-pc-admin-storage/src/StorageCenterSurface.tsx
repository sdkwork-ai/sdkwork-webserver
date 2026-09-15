import type { AuthTokenManager } from "@sdkwork/sdk-common";
import type { WebserverLocale, WebserverResourceKey } from "@sdkwork/webserver-pc-commons";
import { useCallback, useMemo } from "react";
import { createDriveAdminStorageHostClient } from "sdkwork-drive-pc-admin-core";
import {
  StorageBindingsAdminPage,
  StorageBucketsAdminPage,
  StorageProviderKindsAdminPage,
  StorageProvidersAdminPage,
} from "sdkwork-drive-pc-admin-storage-providers";
import { LanguageProvider } from "sdkwork-drive-pc-commons";
import type { SessionSnapshot } from "sdkwork-drive-pc-core";

/**
 * Resources the Storage Center owns, in sidebar order.
 *
 * Kept beside the surface so the host renderer map and the module entry
 * registry can both be checked against one list at compile time.
 */
export const STORAGE_CENTER_RESOURCES = [
  "storage-providers",
  "storage-kinds",
  "storage-buckets",
  "storage-bindings",
] as const satisfies readonly WebserverResourceKey[];

export type StorageCenterResource = (typeof STORAGE_CENTER_RESOURCES)[number];

export interface StorageCenterSurfaceProps {
  /**
   * Drive admin-storage API base URL for this host origin. On the standalone
   * Web Server edge every API shares the page origin, so this is `/`; an
   * absolute URL is equally valid and the shared client aligns its protocol
   * with the hosting page.
   */
  adminStorageApiBaseUrl: string;
  locale: WebserverLocale;
  /** Operator (actor) the storage mutations are attributed to. */
  operatorId: string;
  resource: StorageCenterResource;
  /** Tenant whose storage plane is being administered. */
  tenantId: string;
  /**
   * IAM dual-token manager. It is structurally the drive session token
   * manager the shared pages require, so the host never has to build a drive
   * runtime just to authenticate.
   */
  tokenManager: AuthTokenManager;
}

/**
 * Bridges the drive-owned admin storage plane into the Web Server
 * backend-admin console.
 *
 * The menu entry and the routes stay in the host; the pages themselves are the
 * canonical `sdkwork-drive-pc-admin-storage-providers` implementation that
 * cloudrouter consumes too, so both applications reuse one module instead of
 * forking it. This component therefore does exactly three things:
 *
 * 1. composes the shared admin storage SDK client against the host base URL,
 * 2. adapts the host session into the drive session snapshot the pages read,
 * 3. hands the host's language to the drive dictionary provider.
 *
 * Styles are scoped by `.storage-center-surface`; the storage i18n catalog
 * lives in `sdkwork-drive-pc-commons` and is intentionally not re-declared by
 * the host.
 */
export function StorageCenterSurface({
  adminStorageApiBaseUrl,
  locale,
  operatorId,
  resource,
  tenantId,
  tokenManager,
}: StorageCenterSurfaceProps) {
  const adminStorageSdkClient = useMemo(
    () => createDriveAdminStorageHostClient({ baseUrl: adminStorageApiBaseUrl, tokenManager }),
    [adminStorageApiBaseUrl, tokenManager],
  );
  // The shared pages read exactly two session facts — the tenant they act in
  // and the actor a mutation is attributed to — so the adapter projects those
  // onto the drive snapshot rather than making the host fabricate a drive
  // session store.
  const getSession = useCallback<() => SessionSnapshot>(
    () => ({ context: { actorId: operatorId, tenantId, userId: operatorId } }),
    [operatorId, tenantId],
  );

  return (
    <div className="storage-center-surface">
      {/*
        The host language is authoritative: the drive dictionary ships its own
        en-US/zh-CN catalogs and falls back to the key, so the host supplies
        only the language. `key` re-seeds the provider's one-shot host-language
        read when the locale changes.
      */}
      <LanguageProvider defaultLanguage={locale} key={locale} resolveHostLanguage={() => locale}>
        {resource === "storage-providers" ? (
          <StorageProvidersAdminPage
            adminStorageSdkClient={adminStorageSdkClient}
            getSession={getSession}
          />
        ) : resource === "storage-kinds" ? (
          <StorageProviderKindsAdminPage
            adminStorageSdkClient={adminStorageSdkClient}
            getSession={getSession}
          />
        ) : resource === "storage-buckets" ? (
          <StorageBucketsAdminPage
            adminStorageSdkClient={adminStorageSdkClient}
            getSession={getSession}
          />
        ) : (
          <StorageBindingsAdminPage
            adminStorageSdkClient={adminStorageSdkClient}
            getSession={getSession}
          />
        )}
      </LanguageProvider>
    </div>
  );
}
