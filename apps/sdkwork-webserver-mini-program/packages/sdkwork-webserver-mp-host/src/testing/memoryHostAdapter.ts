import {
  createWebserverMpHostError,
  type WebserverMpHostAdapter,
  type WebserverMpHostError,
  type WebserverMpHostStorage,
} from "../contracts/hostAdapter";

/**
 * In-memory host adapter — the fake `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8
 * requires so the same contract the device runs can be exercised without a
 * device. It is deliberately deterministic: recorded toasts and an ordered
 * storage map, so a contract test can assert exactly what the page asked the host
 * to do.
 */
export interface MemoryWebserverMpHostAdapter extends WebserverMpHostAdapter {
  readonly toasts: readonly string[];
  readonly pullDownRefreshStops: number;
}

export interface CreateMemoryWebserverMpHostAdapterOptions {
  readonly language?: string;
  readonly storage?: Readonly<Record<string, string>>;
}

export function createMemoryWebserverMpHostAdapter(
  options: CreateMemoryWebserverMpHostAdapterOptions = {},
): MemoryWebserverMpHostAdapter {
  const records = new Map<string, string>(Object.entries(options.storage ?? {}));
  const toasts: string[] = [];
  let pullDownRefreshStops = 0;

  const storage: WebserverMpHostStorage = {
    get: (key) => records.get(key) ?? null,
    set: (key, value) => {
      records.set(key, value);
    },
    remove: (key) => {
      records.delete(key);
    },
  };

  return {
    platform: "unknown",
    isAvailable: () => false,
    getLocale: () => ({ language: options.language ?? "en", platform: "unknown" }),
    createStorage: () => storage,
    showToast: (message): WebserverMpHostError | null => {
      if (typeof message !== "string" || message.length === 0) {
        return createWebserverMpHostError("toast-failed", "a toast needs a message");
      }
      toasts.push(message);
      return null;
    },
    stopPullDownRefresh: () => {
      pullDownRefreshStops += 1;
    },
    get toasts() {
      return [...toasts];
    },
    get pullDownRefreshStops() {
      return pullDownRefreshStops;
    },
  };
}
