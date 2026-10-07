import { AbilityContext } from '@kit.AbilityKit';
import { util } from '@kit.ArkTS';
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
const RUNTIME_ENV_RAWFILE: string = 'runtime-env.json';

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
      createWebserverHarmonyRuntimeConfig(context),
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
 * `config/app/runtime-env.<deploymentProfile>.<environment>.json` into the HAP
 * rawfile `runtime-env.json` — `scripts/project-runtime.mjs --profile <id>`
 * copies the committed profile before the build, mirroring the mini-program
 * root's build step. This module only reads and hands the parsed document on;
 * a missing rawfile fails the launch with the fix in the message instead of
 * falling back to a hand-written host, because a silent default host is
 * exactly the class of bug `SOURCE_CONFIG_SPEC.md` exists to prevent.
 */
function createWebserverHarmonyRuntimeConfig(context: AbilityContext): Record<string, Object> {
  let rawFile: Uint8Array;
  try {
    rawFile = context.resourceManager.getRawFileContentSync(RUNTIME_ENV_RAWFILE);
  } catch (error) {
    throw new Error(
      `Harmony rawfile ${RUNTIME_ENV_RAWFILE} is missing; run ` +
        `scripts/project-runtime.mjs --profile <profile-id> before building (cause: ${JSON.stringify(error)})`,
    );
  }
  const text: string = util.TextDecoder.create('utf-8').decodeToString(rawFile);
  return JSON.parse(text) as Record<string, Object>;
}
