/**
 * Core-owned SDK inventory. Mirrors
 * `specs/component.spec.json#contracts.sdkClients` and is the single place the
 * H5 application root declares which generated app SDK families it composes.
 *
 * The `deploy_app` entity is owned by `sdkwork-deployments`; this root consumes
 * that authority through `@sdkwork/deployments-app-sdk`. The legacy
 * `@sdkwork/webserver-app-sdk` family is not composed here — it belongs to the
 * webserver app-api surface that the deployments-owned one supersedes
 * (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7: one owner per normalized route).
 */
export function listWebserverH5CoreSdkInventory() {
  return [
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
