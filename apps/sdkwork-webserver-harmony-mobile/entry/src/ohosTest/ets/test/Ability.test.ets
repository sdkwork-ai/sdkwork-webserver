import { describe, expect, it } from '@ohos/hypium';

import { createWebserverHarmonyEnvironment } from '../../../main/ets/bootstrap/Environment';

/**
 * HarmonyOS instrumented-test entrypoint.
 *
 * `ohosTest` runs on a device/emulator and therefore only covers what needs the
 * platform runtime. The architecture contract itself is enforced without the
 * toolchain by `tests/harmony-surface-contract.test.mjs`.
 */
export default function abilityTest() {
  describe('WebserverHarmonyEnvironment', () => {
    it('rejects a runtime config whose target is not harmony-native', 0, () => {
      expect(() => createWebserverHarmonyEnvironment({
        environment: 'development',
        deploymentProfile: 'standalone',
        profileId: 'standalone.development',
        runtimeTarget: 'flutter-android',
        webserver: {
          apiBaseUrl: 'http://127.0.0.1:3800/app/v3/api',
          backendApiBaseUrl: 'http://127.0.0.1:3800/backend/v3/api',
        },
        appbase: {
          appApiBaseUrl: 'http://127.0.0.1:3800',
          loginUrl: 'http://127.0.0.1:3800',
        },
      })).assertThrow(Error);
    });
  });
}
