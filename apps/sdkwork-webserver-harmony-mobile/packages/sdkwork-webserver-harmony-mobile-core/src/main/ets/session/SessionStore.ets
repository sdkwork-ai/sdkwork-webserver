/**
 * Session and token store for the HarmonyOS root.
 *
 * Authority: `APP_SDK_INTEGRATION_SPEC.md`. Logout, refresh failure, tenant
 * switch, and account switch must clear this store together with the token
 * manager, secure platform storage, and the realtime/session bridges
 * (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7).
 *
 * Capability packages never read or write tokens; they receive an
 * already-constructed SDK port from core.
 */

export interface WebserverHarmonySession {
  readonly accessToken?: string;
  readonly authToken?: string;
  readonly tenantId?: string;
  readonly organizationId?: string;
  readonly userId?: string;
}

/** A package-level cache that holds tenant/account-scoped data. */
export type WebserverHarmonySensitiveStateClearer = () => void;

const sensitiveStateClearers: WebserverHarmonySensitiveStateClearer[] = [];

/**
 * Register a cache that must be dropped on logout.
 *
 * `clearWebserverHarmonySession` runs every registered clearer, so a logout drops
 * the session record, the token projections, and every capability cache in one
 * call instead of relying on each screen to notice.
 */
export function registerWebserverHarmonySensitiveStateClearer(
  clearer: WebserverHarmonySensitiveStateClearer,
): void {
  if (!sensitiveStateClearers.includes(clearer)) {
    sensitiveStateClearers.push(clearer);
  }
}

export function clearRegisteredWebserverHarmonySensitiveState(): void {
  for (const clearer of sensitiveStateClearers) {
    clearer();
  }
  sensitiveStateClearers.length = 0;
}

let currentSession: WebserverHarmonySession | null = null;

export function readWebserverHarmonySession(): WebserverHarmonySession | null {
  return currentSession;
}

function normalizeToken(value: string | undefined): string | undefined {
  const trimmed: string = value === undefined ? '' : value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

export function writeWebserverHarmonySession(
  session: WebserverHarmonySession | null,
): WebserverHarmonySession | null {
  if (session === null) {
    currentSession = null;
    return null;
  }
  const accessToken: string | undefined = normalizeToken(session.accessToken);
  const authToken: string | undefined = normalizeToken(session.authToken);
  if (accessToken === undefined && authToken === undefined) {
    currentSession = null;
    return null;
  }
  const next: WebserverHarmonySession = {
    ...(accessToken === undefined ? {} : { accessToken }),
    ...(authToken === undefined ? {} : { authToken }),
    ...(normalizeToken(session.tenantId) === undefined
      ? {}
      : { tenantId: normalizeToken(session.tenantId) }),
    ...(normalizeToken(session.organizationId) === undefined
      ? {}
      : { organizationId: normalizeToken(session.organizationId) }),
    ...(normalizeToken(session.userId) === undefined
      ? {}
      : { userId: normalizeToken(session.userId) }),
  };
  currentSession = next;
  return next;
}

export function isWebserverHarmonySessionAuthenticated(): boolean {
  return currentSession?.accessToken !== undefined && currentSession?.authToken !== undefined;
}

/** Full sensitive-state teardown: session record, then every registered cache. */
export function clearWebserverHarmonySession(): void {
  currentSession = null;
  clearRegisteredWebserverHarmonySensitiveState();
}
