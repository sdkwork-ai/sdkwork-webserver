/**
 * Appbase IAM runtime wiring for the HarmonyOS root.
 *
 * Authority: `APP_SDK_INTEGRATION_SPEC.md` and `IAM_LOGIN_INTEGRATION_SPEC.md`.
 *
 * The global token-manager equivalent is shared by the appbase app SDK, every
 * authenticated app-api client, and explicit backend-admin clients. Logout and
 * refresh failure must clear the token manager, the session/context store, the
 * secure platform storage, and any realtime bridge in one call
 * (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7).
 */

export interface WebserverHarmonyIamRuntime {
  isAuthenticated(): boolean;
  setAuthenticated(value: boolean): void;
  hasPermission(permission: string): boolean;
  clearOnLogout(): void;
}

export class WebserverHarmonyIamRuntimeImpl implements WebserverHarmonyIamRuntime {
  private authenticated: boolean = false;
  private readonly grantedPermissions: string[] = [];

  isAuthenticated(): boolean {
    return this.authenticated;
  }

  setAuthenticated(value: boolean): void {
    this.authenticated = value;
  }

  hasPermission(permission: string): boolean {
    return this.grantedPermissions.includes(permission);
  }

  clearOnLogout(): void {
    this.authenticated = false;
    this.grantedPermissions.length = 0;
  }
}

let iamRuntime: WebserverHarmonyIamRuntime | null = null;

export function createWebserverHarmonyIamRuntime(): WebserverHarmonyIamRuntime {
  iamRuntime = new WebserverHarmonyIamRuntimeImpl();
  return iamRuntime;
}

export function getWebserverHarmonyIamRuntime(): WebserverHarmonyIamRuntime {
  if (iamRuntime === null) {
    return createWebserverHarmonyIamRuntime();
  }
  return iamRuntime;
}
