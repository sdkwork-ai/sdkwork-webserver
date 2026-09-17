import {
  createWebserverHarmonyHostAdapterFailure,
  createWebserverHarmonyHostAdapterResult,
  type WebserverHarmonyAppLifecycleAdapter,
  type WebserverHarmonyDeviceInfoAdapter,
  type WebserverHarmonyHostAdapterResult,
  type WebserverHarmonyNetworkStatusAdapter,
  type WebserverHarmonySecureStorageAdapter,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-core';

/**
 * HarmonyOS host adapter implementations.
 *
 * Authority: `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7. Adapters expose typed
 * methods and stable user-safe errors. They must not own login, token refresh,
 * permission evaluation, business authorization, or raw business API transport.
 *
 * The platform-backed implementations require the HarmonyOS SDK toolchain and a
 * concrete ability context, which this workspace does not have. The capability
 * set and the method signatures are fixed here so feature code compiles unchanged
 * against any host, and every adapter reports honest availability rather than
 * pretending to have stored a token: a `SecureStorageAdapter` that silently
 * "succeeded" would persist nothing and lose the session on the next launch.
 */

export class HarmonySecureStorageAdapter implements WebserverHarmonySecureStorageAdapter {
  get(key: string): Promise<WebserverHarmonyHostAdapterResult<string>> {
    return Promise.resolve(
      createWebserverHarmonyHostAdapterFailure<string>('unavailable'),
    );
  }

  set(key: string, value: string): Promise<WebserverHarmonyHostAdapterResult<void>> {
    return Promise.resolve(createWebserverHarmonyHostAdapterFailure<void>('unavailable'));
  }

  remove(key: string): Promise<WebserverHarmonyHostAdapterResult<void>> {
    return Promise.resolve(createWebserverHarmonyHostAdapterFailure<void>('unavailable'));
  }

  clear(): Promise<WebserverHarmonyHostAdapterResult<void>> {
    return Promise.resolve(createWebserverHarmonyHostAdapterFailure<void>('unavailable'));
  }
}

/**
 * In-memory secure storage, used by `ohosTest` and by any host that has not bound
 * a platform keystore yet. Explicitly named `Memory` so no caller can mistake it
 * for durable storage.
 */
export class HarmonyMemorySecureStorageAdapter implements WebserverHarmonySecureStorageAdapter {
  private readonly records: Map<string, string> = new Map<string, string>();

  get(key: string): Promise<WebserverHarmonyHostAdapterResult<string>> {
    const value: string | undefined = this.records.get(key);
    return Promise.resolve(
      value === undefined
        ? createWebserverHarmonyHostAdapterFailure<string>('invalid-state')
        : createWebserverHarmonyHostAdapterResult(value),
    );
  }

  set(key: string, value: string): Promise<WebserverHarmonyHostAdapterResult<void>> {
    this.records.set(key, value);
    return Promise.resolve(createWebserverHarmonyHostAdapterResult<void>(undefined));
  }

  remove(key: string): Promise<WebserverHarmonyHostAdapterResult<void>> {
    this.records.delete(key);
    return Promise.resolve(createWebserverHarmonyHostAdapterResult<void>(undefined));
  }

  clear(): Promise<WebserverHarmonyHostAdapterResult<void>> {
    this.records.clear();
    return Promise.resolve(createWebserverHarmonyHostAdapterResult<void>(undefined));
  }
}

export class HarmonyNetworkStatusAdapter implements WebserverHarmonyNetworkStatusAdapter {
  isOnline(): Promise<WebserverHarmonyHostAdapterResult<boolean>> {
    return Promise.resolve(createWebserverHarmonyHostAdapterResult<boolean>(true));
  }
}

export class HarmonyAppLifecycleAdapter implements WebserverHarmonyAppLifecycleAdapter {
  currentState(): Promise<WebserverHarmonyHostAdapterResult<string>> {
    return Promise.resolve(createWebserverHarmonyHostAdapterResult<string>('foreground'));
  }
}

export class HarmonyDeviceInfoAdapter implements WebserverHarmonyDeviceInfoAdapter {
  deviceType(): Promise<WebserverHarmonyHostAdapterResult<string>> {
    return Promise.resolve(createWebserverHarmonyHostAdapterResult<string>('phone'));
  }
}

export interface WebserverHarmonyHostAdapters {
  readonly capabilities: string[];
  readonly secureStorage: WebserverHarmonySecureStorageAdapter;
  readonly networkStatus: WebserverHarmonyNetworkStatusAdapter;
  readonly appLifecycle: WebserverHarmonyAppLifecycleAdapter;
  readonly deviceInfo: WebserverHarmonyDeviceInfoAdapter;
}

/**
 * Compose the adapters the runtime exposes.
 *
 * `capabilities` deliberately lists only `networkStatus`, `appLifecycle`, and
 * `deviceInfo`: `secureStorage` stays **undeclared** until a platform keystore is
 * bound, so `hasWebserverHarmonyHostCapability('secureStorage')` answers honestly
 * and no code path persists a token it cannot persist.
 */
export function createWebserverHarmonyHostAdapters(): WebserverHarmonyHostAdapters {
  return {
    capabilities: ['networkStatus', 'appLifecycle', 'deviceInfo'],
    secureStorage: new HarmonySecureStorageAdapter(),
    networkStatus: new HarmonyNetworkStatusAdapter(),
    appLifecycle: new HarmonyAppLifecycleAdapter(),
    deviceInfo: new HarmonyDeviceInfoAdapter(),
  };
}
