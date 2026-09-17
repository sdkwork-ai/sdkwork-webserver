/**
 * Platform storage bridge for the mini program session store.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8 keeps platform globals behind typed
 * adapters. The core is the only package allowed to bind the real WeChat storage
 * API, and it does so through this interface so tests and non-WeChat hosts can
 * inject an in-memory implementation instead of monkey-patching `wx`.
 */
export interface WebserverMpSyncStorage {
  get(key: string): string | null;
  set(key: string, value: string): void;
  remove(key: string): void;
}

/** The subset of `wx.*StorageSync` this bridge binds. */
export interface WebserverMpPlatformStorageApi {
  getStorageSync?: (key: string) => unknown;
  setStorageSync?: (key: string, value: unknown) => void;
  removeStorageSync?: (key: string) => void;
}

function platformStorageApi(): WebserverMpPlatformStorageApi | undefined {
  return (globalThis as { wx?: WebserverMpPlatformStorageApi }).wx;
}

/**
 * Bind the WeChat synchronous storage API.
 *
 * Every call is guarded: a mini program may run this bundle before the platform
 * bridge exists (for example in the Node-side contract tests), and an
 * unavailable storage must degrade to "no session", never to a thrown error that
 * takes the whole launch down.
 */
export function createWebserverMpPlatformStorage(
  api: WebserverMpPlatformStorageApi | undefined = platformStorageApi(),
): WebserverMpSyncStorage {
  return {
    get: (key) => {
      try {
        const value = api?.getStorageSync?.(key);
        return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
      } catch {
        return null;
      }
    },
    set: (key, value) => {
      try {
        api?.setStorageSync?.(key, value);
      } catch {
        // A failed write degrades to an in-memory session for this launch.
      }
    },
    remove: (key) => {
      try {
        api?.removeStorageSync?.(key);
      } catch {
        // ignore
      }
    },
  };
}

/** Deterministic storage for tests and non-WeChat hosts. */
export function createWebserverMpMemoryStorage(
  seed: Readonly<Record<string, string>> = {},
): WebserverMpSyncStorage {
  const records = new Map<string, string>(Object.entries(seed));
  return {
    get: (key) => records.get(key) ?? null,
    set: (key, value) => {
      records.set(key, value);
    },
    remove: (key) => {
      records.delete(key);
    },
  };
}
