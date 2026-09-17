/**
 * Core-owned SDK inventory.
 *
 * Mirrors `specs/component.spec.json#contracts.sdkClients` and is the single
 * place this application root declares which generated app SDK families it
 * composes. The set is identical to the PC, H5, and mini program roots so all
 * four clients reach the same authorities
 * (`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §7).
 */

export interface WebserverHarmonyCoreSdkInventoryEntry {
  readonly packageName: string;
  readonly authority: string;
  readonly surface: string;
  readonly permissionModuleId: string;
}

export const webserverHarmonyCoreSdkInventory: WebserverHarmonyCoreSdkInventoryEntry[] = [
  {
    packageName: '@sdkwork/webserver-app-sdk',
    authority: 'sdkwork-webserver-app-api',
    surface: 'app-api',
    permissionModuleId: 'web',
  },
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
