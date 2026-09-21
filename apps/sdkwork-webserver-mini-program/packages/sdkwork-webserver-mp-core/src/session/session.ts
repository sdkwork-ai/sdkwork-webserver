import { readBootstrapAccessTokenFromProcessEnv } from "@sdkwork/iam-credential-entry";
import type { AuthTokenManager, AuthTokens } from "@sdkwork/sdk-common";

import {
  createWebserverMpPlatformStorage,
  type WebserverMpSyncStorage,
} from "./storage";

/**
 * Mini program session authority.
 *
 * This module is the single owner of the persisted dual-token session for the
 * WeChat console: it reads/writes the durable platform-storage record, exposes
 * the token projections the generated app SDK clients need, implements the
 * `AuthTokenManager` contract those clients accept, and owns the
 * sensitive-state clearing registry required by
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §7.
 *
 * Capability packages MUST NOT read or write tokens directly, and MUST NOT call
 * `wx.*`; they receive an already-constructed SDK client from core
 * (`APP_SDK_INTEGRATION_SPEC.md` §2).
 */

export interface WebserverMpSession {
  accessToken?: string;
  authToken?: string;
  refreshToken?: string;
  expiresAt?: number;
  tenantId?: string;
  organizationId?: string;
  userId?: string;
}

const SESSION_STORAGE_KEY = "sdkwork-webserver-mp:session:v1";

function normalizeToken(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function normalizeSession(value: unknown): WebserverMpSession | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const candidate = value as WebserverMpSession;
  const session: WebserverMpSession = {
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

// ---------------------------------------------------------------------------
// Sensitive-state clearing registry
// ---------------------------------------------------------------------------

export type WebserverMpSensitiveStateClearer = () => void;

const sensitiveStateClearers = new Set<WebserverMpSensitiveStateClearer>();

/**
 * Register a package-level cache that holds tenant/account-scoped data.
 * `clearWebserverMpSession` runs every registered clearer, so a logout drops the
 * platform storage record, the token manager, and every capability cache in one
 * call instead of relying on each screen to notice.
 */
export function registerWebserverMpSensitiveStateClearer(
  clearer: WebserverMpSensitiveStateClearer,
): () => void {
  sensitiveStateClearers.add(clearer);
  return () => {
    sensitiveStateClearers.delete(clearer);
  };
}

export function clearRegisteredWebserverMpSensitiveState(): void {
  for (const clearer of sensitiveStateClearers) {
    try {
      clearer();
    } catch {
      // A broken clearer must not block the rest of the teardown.
    }
  }
}

// ---------------------------------------------------------------------------
// Session record
// ---------------------------------------------------------------------------

export function readWebserverMpSession(
  storage: WebserverMpSyncStorage = createWebserverMpPlatformStorage(),
): WebserverMpSession | null {
  const raw = storage.get(SESSION_STORAGE_KEY);
  if (!raw) {
    return null;
  }
  try {
    return normalizeSession(JSON.parse(raw));
  } catch {
    storage.remove(SESSION_STORAGE_KEY);
    return null;
  }
}

export function persistWebserverMpSession(
  session: WebserverMpSession,
  storage: WebserverMpSyncStorage = createWebserverMpPlatformStorage(),
): WebserverMpSession | null {
  const normalized = normalizeSession(session);
  if (!normalized) {
    clearWebserverMpSession(storage);
    return null;
  }
  storage.set(SESSION_STORAGE_KEY, JSON.stringify(normalized));
  return normalized;
}

/**
 * Full sensitive-state teardown: platform storage record, then every registered
 * capability cache (§7 token/session clearing).
 */
export function clearWebserverMpSession(
  storage: WebserverMpSyncStorage = createWebserverMpPlatformStorage(),
): void {
  storage.remove(SESSION_STORAGE_KEY);
  clearRegisteredWebserverMpSensitiveState();
}

export function isWebserverMpSessionExpired(session: WebserverMpSession | null): boolean {
  const expiresAt = session?.expiresAt;
  return typeof expiresAt === "number" && Number.isFinite(expiresAt) && Date.now() >= expiresAt;
}

export function isWebserverMpSessionAuthenticated(session: WebserverMpSession | null): boolean {
  return Boolean(session?.accessToken && session?.authToken) && !isWebserverMpSessionExpired(session);
}

// ---------------------------------------------------------------------------
// Token manager
// ---------------------------------------------------------------------------

/**
 * `AuthTokenManager` over the persisted mini program session. The generated app
 * SDK clients call this on every request, so it always reads the live record
 * instead of a construction-time snapshot.
 */
export function createWebserverMpTokenManager(
  storage: WebserverMpSyncStorage = createWebserverMpPlatformStorage(),
): AuthTokenManager {
  const readSession = (): WebserverMpSession | null => readWebserverMpSession(storage);

  const patch = (tokens: Partial<WebserverMpSession>): void => {
    const existing = readSession() ?? {};
    const next = normalizeSession({ ...existing, ...tokens });
    if (!next) {
      clearWebserverMpSession(storage);
      return;
    }
    persistWebserverMpSession(next, storage);
  };

  return {
    getAccessToken: () => readSession()?.accessToken ?? readBootstrapAccessTokenFromProcessEnv(),
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
    clearTokens: () => clearWebserverMpSession(storage),
    clearAuthToken: () => patch({ authToken: undefined }),
    clearAccessToken: () => patch({ accessToken: undefined }),
    isExpired: () => isWebserverMpSessionExpired(readSession()),
    isValid: () => isWebserverMpSessionAuthenticated(readSession()),
    hasToken: () => {
      const session = readSession();
      return Boolean(session?.accessToken && session?.authToken);
    },
    hasAuthToken: () => Boolean(readSession()?.authToken),
    hasAccessToken: () => Boolean(readSession()?.accessToken)
      || readBootstrapAccessTokenFromProcessEnv() !== undefined,
    willExpireIn: (seconds: number) => {
      const expiresAt = readSession()?.expiresAt;
      return typeof expiresAt === "number"
        && Number.isFinite(expiresAt)
        && Date.now() + seconds * 1000 >= expiresAt;
    },
  };
}

/**
 * Process-wide token manager. The runtime constructs exactly one so every
 * generated client instance shares the same live token source
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §7).
 */
let globalTokenManager: AuthTokenManager | null = null;

export function getWebserverMpGlobalTokenManager(): AuthTokenManager {
  globalTokenManager = globalTokenManager ?? createWebserverMpTokenManager();
  return globalTokenManager;
}

export function resetWebserverMpGlobalTokenManager(): void {
  globalTokenManager = null;
}
