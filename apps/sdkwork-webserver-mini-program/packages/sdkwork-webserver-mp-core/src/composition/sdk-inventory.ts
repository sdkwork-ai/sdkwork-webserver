/**
 * Core-owned SDK inventory. Mirrors
 * `specs/component.spec.json#contracts.sdkClients` and is the single place the
 * mini program application root declares which generated app SDK families it
 * composes.
 *
 * A native mini program has no browser origin to align to, so every surface
 * below is reached through the absolute application origin of the selected
 * runtime profile (`config/mini-program/runtime-env.<profile>.json`).
 */
export function listWebserverMpCoreSdkInventory() {
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

export type WebserverMpCoreSdkInventoryEntry =
  ReturnType<typeof listWebserverMpCoreSdkInventory>[number];
