import type { WebserverMpRouteContribution } from "../navigation/routePlacement";

/**
 * Mini program auth gate.
 *
 * Authority: `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §7 — the root owns one
 * session; capability pages ask the shell whether a route may render. The gate
 * takes an injected session/permission predicate so it never constructs a
 * transport or reads platform storage itself.
 */
export interface WebserverMpAuthContext {
  readonly authenticated: boolean;
  readonly hasPermission: (permission: string) => boolean;
}

export type WebserverMpAuthDecision =
  | { readonly allowed: true }
  | { readonly allowed: false; readonly reason: "unauthenticated" | "forbidden" };

export function resolveWebserverMpRouteAccess(
  route: Pick<WebserverMpRouteContribution, "auth" | "permissionHint">,
  context: WebserverMpAuthContext,
): WebserverMpAuthDecision {
  if (route.auth === "required" && !context.authenticated) {
    return { allowed: false, reason: "unauthenticated" };
  }
  if (route.permissionHint && !context.hasPermission(route.permissionHint)) {
    return { allowed: false, reason: "forbidden" };
  }
  return { allowed: true };
}
