import {
  createWebserverHarmonyHostAdapters,
  type WebserverHarmonyHostAdapters,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-host';

/**
 * Host adapter registration.
 *
 * Authority: `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7. Feature packages
 * depend on these typed interfaces and never call HarmonyOS system APIs
 * directly.
 */

let hostAdapters: WebserverHarmonyHostAdapters | null = null;

export function registerWebserverHarmonyHostAdapters(): WebserverHarmonyHostAdapters {
  hostAdapters = createWebserverHarmonyHostAdapters();
  return hostAdapters;
}

export function getWebserverHarmonyHostAdapters(): WebserverHarmonyHostAdapters {
  if (hostAdapters === null) {
    return registerWebserverHarmonyHostAdapters();
  }
  return hostAdapters;
}
