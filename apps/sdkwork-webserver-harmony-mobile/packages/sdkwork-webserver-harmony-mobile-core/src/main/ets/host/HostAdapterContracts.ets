/**
 * Host adapter contracts owned by core and implemented by the host package.
 *
 * Authority: `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7. Host adapters expose
 * stable user-safe errors only; platform error text never reaches a caller.
 *
 * Only the categories the Web Server console currently consumes are declared
 * here. The spec's remaining categories are added when a capability needs them.
 */

export type WebserverHarmonyHostAdapterError =
  | 'unsupported'
  | 'permission-denied'
  | 'unavailable'
  | 'cancelled'
  | 'invalid-state';

export interface WebserverHarmonyHostAdapterResult<T> {
  readonly ok: boolean;
  readonly value?: T;
  readonly error?: WebserverHarmonyHostAdapterError;
}

/** Synchronous platform-backed key/value store (session persistence lives here). */
export interface WebserverHarmonySecureStorageAdapter {
  get(key: string): Promise<WebserverHarmonyHostAdapterResult<string>>;
  set(key: string, value: string): Promise<WebserverHarmonyHostAdapterResult<void>>;
  remove(key: string): Promise<WebserverHarmonyHostAdapterResult<void>>;
  clear(): Promise<WebserverHarmonyHostAdapterResult<void>>;
}

export interface WebserverHarmonyNetworkStatusAdapter {
  isOnline(): Promise<WebserverHarmonyHostAdapterResult<boolean>>;
}

export interface WebserverHarmonyAppLifecycleAdapter {
  currentState(): Promise<WebserverHarmonyHostAdapterResult<string>>;
}

export interface WebserverHarmonyDeviceInfoAdapter {
  deviceType(): Promise<WebserverHarmonyHostAdapterResult<string>>;
}

/**
 * Capability set this root declares present.
 *
 * `isAvailable()` is the contract every adapter must answer honestly; a
 * capability the platform cannot serve reports `false` instead of throwing.
 */
export interface WebserverHarmonyHostAdapter {
  readonly capability: string;
  isAvailable(): boolean;
}

export function createWebserverHarmonyHostAdapterResult<T>(
  value: T,
): WebserverHarmonyHostAdapterResult<T> {
  return { ok: true, value };
}

export function createWebserverHarmonyHostAdapterFailure<T>(
  error: WebserverHarmonyHostAdapterError,
): WebserverHarmonyHostAdapterResult<T> {
  return { ok: false, error };
}
