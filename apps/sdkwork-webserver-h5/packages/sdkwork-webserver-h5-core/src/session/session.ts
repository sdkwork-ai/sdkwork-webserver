import type { AuthTokenManager, AuthTokens } from "@sdkwork/sdk-common";

/**
 * H5 session authority. This module is the single owner of the persisted
 * dual-token session for the mobile browser console: it reads/writes the
 * durable `localStorage` record, exposes the token projections the generated
 * app SDK clients need, and implements the `AuthTokenManager` contract those
 * clients accept.
 *
 * Feature packages MUST NOT read or write tokens directly; they receive an
 * already-constructed SDK client from the core (APP_SDK_INTEGRATION_SPEC §2).
 */

export interface WebserverH5Session {
  accessToken?: string;
  authToken?: string;
  refreshToken?: string;
  expiresAt?: number;
  tenantId?: string;
  organizationId?: string;
  userId?: string;
}

const SESSION_STORAGE_KEY = "sdkwork-webserver-h5:session:v1";

export const SDKWORK_WEBSERVER_H5_SESSION_CHANGED_EVENT =
  "sdkwork-webserver-h5:auth-session-changed";

function getLocalStorage(): Storage | undefined {
  if (typeof window === "undefined") {
    return undefined;
  }
  return window.localStorage;
}

function normalizeToken(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function normalizeSession(value: unknown): WebserverH5Session | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const candidate = value as WebserverH5Session;
  const session: WebserverH5Session = {
    ...(normalizeToken(candidate.accessToken) ? { accessToken: normalizeToken(candidate.accessToken) } : {}),
    ...(normalizeToken(candidate.authToken) ? { authToken: normalizeToken(candidate.authToken) } : {}),
    ...(normalizeToken(candidate.refreshToken) ? { refreshToken: normalizeToken(candidate.refreshToken) } : {}),
    ...(typeof candidate.expiresAt === "number" && Number.isFinite(candidate.expiresAt)
      ? { expiresAt: candidate.expiresAt }
      : {}),
    ...(normalizeToken(candidate.tenantId) ? { tenantId: normalizeToken(candidate.tenantId) } : {}),
    ...(normalizeToken(candidate.organizationId)
      ? { organizationId: normalizeToken(candidate.organizationId) }
      : {}),
    ...(normalizeToken(candidate.userId) ? { userId: normalizeToken(candidate.userId) } : {}),
  };
  return Object.keys(session).length > 0 ? session : null;
}

function dispatchSessionChanged(session: WebserverH5Session | null): void {
  if (typeof window === "undefined") {
    return;
  }
  window.dispatchEvent(
    new CustomEvent(SDKWORK_WEBSERVER_H5_SESSION_CHANGED_EVENT, { detail: { session } }),
  );
}

export function readWebserverH5Session(): WebserverH5Session | null {
  const raw = getLocalStorage()?.getItem(SESSION_STORAGE_KEY);
  if (!raw) {
    return null;
  }
  try {
    return normalizeSession(JSON.parse(raw));
  } catch {
    clearWebserverH5Session();
    return null;
  }
}

export function persistWebserverH5Session(session: WebserverH5Session): WebserverH5Session | null {
  const normalized = normalizeSession(session);
  if (!normalized) {
    clearWebserverH5Session();
    return null;
  }
  getLocalStorage()?.setItem(SESSION_STORAGE_KEY, JSON.stringify(normalized));
  dispatchSessionChanged(normalized);
  return normalized;
}

export function clearWebserverH5Session(): void {
  getLocalStorage()?.removeItem(SESSION_STORAGE_KEY);
  dispatchSessionChanged(null);
}

export function isWebserverH5SessionExpired(session = readWebserverH5Session()): boolean {
  const expiresAt = session?.expiresAt;
  return typeof expiresAt === "number" && Number.isFinite(expiresAt) && Date.now() >= expiresAt;
}

export function isWebserverH5SessionAuthenticated(session = readWebserverH5Session()): boolean {
  return Boolean(session?.accessToken && session?.authToken) && !isWebserverH5SessionExpired(session);
}

/**
 * `AuthTokenManager` over the persisted H5 session. The generated app SDK
 * clients call this on every request, so it always reads the live record
 * instead of a construction-time snapshot.
 */
export function createWebserverH5TokenManager(
  readSession: () => WebserverH5Session | null = readWebserverH5Session,
): AuthTokenManager {
  const write = (session: WebserverH5Session | null): void => {
    if (session) {
      persistWebserverH5Session(session);
      return;
    }
    clearWebserverH5Session();
  };

  const patch = (tokens: Partial<WebserverH5Session>): void => {
    const existing = readSession() ?? {};
    const next = normalizeSession({ ...existing, ...tokens });
    write(next);
  };

  return {
    getAccessToken: () => readSession()?.accessToken,
    getAuthToken: () => readSession()?.authToken,
    getRefreshToken: () => readSession()?.refreshToken,
    getTokens: () => {
      const session = readSession();
      return {
        ...(session?.accessToken ? { accessToken: session.accessToken } : {}),
        ...(session?.authToken ? { authToken: session.authToken } : {}),
        ...(session?.refreshToken ? { refreshToken: session.refreshToken } : {}),
        ...(session?.expiresAt ? { expiresAt: session.expiresAt } : {}),
      } satisfies AuthTokens;
    },
    setTokens: (tokens: AuthTokens) => {
      const expiresAt = typeof tokens.expiresAt === "number" && Number.isFinite(tokens.expiresAt)
        ? tokens.expiresAt
        : typeof tokens.expiresIn === "number" && Number.isFinite(tokens.expiresIn)
          ? Date.now() + tokens.expiresIn * 1000
          : readSession()?.expiresAt;
      patch({
        accessToken: tokens.accessToken,
        authToken: tokens.authToken,
        refreshToken: tokens.refreshToken,
        ...(expiresAt ? { expiresAt } : {}),
      });
    },
    setAccessToken: (token: string) => patch({ accessToken: normalizeToken(token) }),
    setAuthToken: (token: string) => patch({ authToken: normalizeToken(token) }),
    setRefreshToken: (token: string) => patch({ refreshToken: normalizeToken(token) }),
    clearTokens: () => write(null),
    clearAuthToken: () => patch({ authToken: undefined }),
    clearAccessToken: () => patch({ accessToken: undefined }),
    isExpired: () => isWebserverH5SessionExpired(readSession()),
    isValid: () => isWebserverH5SessionAuthenticated(readSession()),
    hasToken: () => {
      const session = readSession();
      return Boolean(session?.accessToken && session?.authToken);
    },
    hasAuthToken: () => Boolean(readSession()?.authToken),
    hasAccessToken: () => Boolean(readSession()?.accessToken),
    willExpireIn: (seconds: number) => {
      const expiresAt = readSession()?.expiresAt;
      return typeof expiresAt === "number" && Number.isFinite(expiresAt)
        && Date.now() + seconds * 1000 >= expiresAt;
    },
  };
}

/**
 * Process-wide token manager. The core constructs exactly one so every
 * generated client instance shares the same live token source.
 */
let globalTokenManager: AuthTokenManager | null = null;

export function getWebserverH5GlobalTokenManager(): AuthTokenManager {
  globalTokenManager = globalTokenManager ?? createWebserverH5TokenManager();
  return globalTokenManager;
}
