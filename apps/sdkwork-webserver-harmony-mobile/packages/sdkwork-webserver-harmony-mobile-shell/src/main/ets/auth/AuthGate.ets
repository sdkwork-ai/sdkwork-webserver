import type { WebserverHarmonyRouteContribution } from '../navigation/RoutePlacement';

/**
 * HarmonyOS auth gate.
 *
 * Authority: `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7 — the root owns one
 * session; capability pages ask the shell whether a route may render. The gate
 * takes an injected session/permission predicate, so it never constructs a
 * transport and never reads platform storage itself.
 */
export interface WebserverHarmonyAuthContext {
  readonly authenticated: boolean;
  readonly hasPermission: (permission: string) => boolean;
}

export type WebserverHarmonyAuthDecision =
  | { readonly allowed: true }
  | { readonly allowed: false; readonly reason: 'unauthenticated' | 'forbidden' };

export function resolveWebserverHarmonyRouteAccess(
  route: Pick<WebserverHarmonyRouteContribution, 'auth' | 'permissionHint'>,
  context: WebserverHarmonyAuthContext,
): WebserverHarmonyAuthDecision {
  if (route.auth === 'required' && !context.authenticated) {
    return { allowed: false, reason: 'unauthenticated' };
  }
  const hint: string | undefined = route.permissionHint;
  if (hint !== undefined && hint.length > 0 && !context.hasPermission(hint)) {
    return { allowed: false, reason: 'forbidden' };
  }
  return { allowed: true };
}
