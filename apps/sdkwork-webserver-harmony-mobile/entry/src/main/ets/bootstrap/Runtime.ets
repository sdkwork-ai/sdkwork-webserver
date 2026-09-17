import { AbilityContext } from '@kit.AbilityKit';
import { hilog } from '@kit.PerformanceAnalysisKit';

import {
  createWebserverHarmonyEnvironment,
  type WebserverHarmonyEnvironment,
} from './Environment';
import { registerWebserverHarmonyHostAdapters } from './HostAdapters';
import { createWebserverHarmonyIamRuntime } from './IamRuntime';
import { createWebserverHarmonySdkClients } from './SdkClients';
import { createWebserverHarmonyRoutes } from './Routes';

const DOMAIN: number = 0x0000;
const TAG: string = 'SdkworkWebserverBootstrap';

/**
 * Root composition entry.
 *
 * This module assembles the environment, SDK clients, IAM runtime, host
 * adapters, and route registry only. It must not own product business logic.
 */
export interface WebserverHarmonyRuntime {
  readonly environment: WebserverHarmonyEnvironment;
  readonly routeCount: number;
}

export function bootstrapWebserverHarmonyApp(
  context: AbilityContext,
): WebserverHarmonyRuntime | null {
  try {
    const environment: WebserverHarmonyEnvironment = createWebserverHarmonyEnvironment(
      createWebserverHarmonyRuntimeConfig(),
    );
    createWebserverHarmonySdkClients().initialize(environment.webserverApiBaseUrl);
    createWebserverHarmonyIamRuntime();
    registerWebserverHarmonyHostAdapters();
    const routes = createWebserverHarmonyRoutes();
    hilog.info(DOMAIN, TAG, 'bootstrap completed for profile %{public}s', environment.profileId);
    return { environment, routeCount: routes.length };
  } catch (error) {
    hilog.error(DOMAIN, TAG, 'bootstrap failed: %{public}s', JSON.stringify(error));
    return null;
  }
}

/**
 * Runtime config projection.
 *
 * The build projects exactly one non-secret
 * `config/app/runtime-env.<deploymentProfile>.<environment>.json` resource into
 * the HAP. Until the hvigor resource-projection task is wired, bootstrap fails
 * fast instead of falling back to a hand-written host: a silent default host is
 * exactly the class of bug `SOURCE_CONFIG_SPEC.md` exists to prevent.
 */
function createWebserverHarmonyRuntimeConfig(): Record<string, Object> {
  throw new Error(
    'Harmony runtime config projection is not wired yet: project config/app/runtime-env.<profile-id>.json as an ArkTS resource module before bootstrapping',
  );
}
