/**
 * Core-owned SDK inventory. Mirrors
 * `specs/component.spec.json#contracts.sdkClients` and is the single place the
 * H5 application root declares which generated app SDK families it composes.
 */
export function listWebserverH5CoreSdkInventory() {
  return [
    {
      packageName: "@sdkwork/webserver-app-sdk",
      authority: "sdkwork-webserver-app-api",
      surface: "app-api",
    },
    {
      packageName: "@sdkwork/drive-app-sdk",
      authority: "sdkwork-drive-app-api",
      surface: "app-api",
    },
    {
      packageName: "@sdkwork/deployments-app-sdk",
      authority: "sdkwork-deployments-app-api",
      surface: "app-api",
    },
  ] as const;
}

export type WebserverH5CoreSdkInventoryEntry =
  ReturnType<typeof listWebserverH5CoreSdkInventory>[number];
