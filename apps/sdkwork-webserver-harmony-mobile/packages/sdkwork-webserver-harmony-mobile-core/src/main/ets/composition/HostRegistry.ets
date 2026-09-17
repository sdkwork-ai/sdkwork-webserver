/**
 * Host adapter capability registry.
 *
 * Renderer/feature code routes through the declared capability set; a direct
 * platform branch on HarmonyOS globals is forbidden
 * (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7). The concrete adapters live in
 * `@sdkwork/sdkwork-webserver-harmony-mobile-host`; this registry only declares
 * which of them the bootstrapped runtime composed.
 */

export const webserverHarmonyHostCapabilities: string[] = [
  'secureStorage',
  'networkStatus',
  'appLifecycle',
  'deviceInfo',
];

export function hasWebserverHarmonyHostCapability(capability: string): boolean {
  return webserverHarmonyHostCapabilities.includes(capability);
}
