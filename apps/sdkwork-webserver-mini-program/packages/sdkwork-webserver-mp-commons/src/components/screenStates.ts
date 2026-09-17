/**
 * Domain-neutral screen/list state primitives.
 *
 * Capability packages map these to their own data; the primitives stay
 * payload-free so they remain reusable across capabilities
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §3 — commons owns the shared
 * primitives, capability packages own the pages).
 */
export type WebserverMpScreenStatus = "loading" | "ready" | "empty" | "error";

export interface WebserverMpScreenState {
  readonly status: WebserverMpScreenStatus;
  readonly errorMessage?: string;
}

export const initialWebserverMpScreenState: WebserverMpScreenState = { status: "loading" };

export function resolveWebserverMpScreenStatus(
  itemCount: number,
  loading: boolean,
  errorMessage?: string,
): WebserverMpScreenStatus {
  if (loading) {
    return "loading";
  }
  if (typeof errorMessage === "string" && errorMessage.length > 0) {
    return "error";
  }
  return itemCount === 0 ? "empty" : "ready";
}

export function resolveWebserverMpScreenState(
  itemCount: number,
  loading: boolean,
  errorMessage?: string,
): WebserverMpScreenState {
  return {
    status: resolveWebserverMpScreenStatus(itemCount, loading, errorMessage),
    ...(errorMessage ? { errorMessage } : {}),
  };
}
