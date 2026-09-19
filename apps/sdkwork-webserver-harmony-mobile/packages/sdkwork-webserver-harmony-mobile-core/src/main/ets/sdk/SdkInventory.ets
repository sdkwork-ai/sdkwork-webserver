/**
 * Core-owned SDK inventory.
 *
 * Mirrors `specs/component.spec.json#contracts.sdkClients` and is the single
 * place this application root declares which generated app SDK families it
 * composes. The set is identical to the PC, H5, mini program, and Flutter roots
 * so every client reaches the same authorities
 * (`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §8).
 *
 * The legacy `@sdkwork/webserver-app-sdk` family is not composed here: the
 * `deploy_app` entity is owned by `sdkwork-deployments`, and composing a second,
 * webserver-owned application surface would be a duplicate owner
 * (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7).
 */

export interface WebserverHarmonyCoreSdkInventoryEntry {
  readonly packageName: string;
  readonly authority: string;
  readonly surface: string;
  readonly permissionModuleId: string;
}

export const webserverHarmonyCoreSdkInventory: WebserverHarmonyCoreSdkInventoryEntry[] = [
  {
    packageName: '@sdkwork/drive-app-sdk',
    authority: 'sdkwork-drive-app-api',
    surface: 'app-api',
    permissionModuleId: 'drive',
  },
  {
    packageName: '@sdkwork/deployments-app-sdk',
    authority: 'sdkwork-deployments-app-api',
    surface: 'app-api',
    permissionModuleId: 'deployments',
  },
];

export function listWebserverHarmonyCoreSdkInventory(): WebserverHarmonyCoreSdkInventoryEntry[] {
  return webserverHarmonyCoreSdkInventory;
}
