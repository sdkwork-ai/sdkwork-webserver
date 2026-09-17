import type { SdkworkAuthController } from "@sdkwork/auth-pc-react";

/**
 * Collapse concurrent `bootstrap()` callers onto a single in-flight request.
 *
 * The session bootstrap is reachable from more than one mount point — the public
 * portal/documentation shell and the authenticated auth gate each drive it — and
 * React re-runs mount effects an extra time in development. An unguarded
 * controller therefore issues one `auth/sessions/current` request per caller.
 *
 * Only *concurrent* callers are collapsed. The settled result is deliberately not
 * cached so that an explicit retry after a failure, and the re-bootstrap that
 * follows `signOut()`, still reach the network.
 *
 * This mirrors the `currentRequest` guard already used by
 * `createWebserverAuthRuntimeConfigLoader` in `../auth/authRuntimeConfig.ts`.
 */
export function dedupeAuthControllerBootstrap<TController extends SdkworkAuthController>(
  controller: TController,
): TController {
  let inFlight: Promise<unknown> | undefined;

  return {
    ...controller,
    bootstrap() {
      if (!inFlight) {
        const request = controller.bootstrap().finally(() => {
          if (inFlight === request) {
            inFlight = undefined;
          }
        });
        inFlight = request;
      }
      return inFlight as ReturnType<SdkworkAuthController["bootstrap"]>;
    },
  };
}
