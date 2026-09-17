import {
  createWebserverMpHostError,
  normalizeWebserverMpPlatformLocale,
  type WebserverMpHostAdapter,
  type WebserverMpHostError,
  type WebserverMpHostLocaleInfo,
  type WebserverMpHostPlatform,
  type WebserverMpHostStorage,
} from "../contracts/hostAdapter";

/**
 * The subset of the WeChat global this adapter binds. Declared locally rather
 * than pulled from `miniprogram-api-typings` so the adapter contract stays the
 * source of truth and the compile depends on a shape we control.
 */
export interface WeixinHostApi {
  getAppBaseInfo?: () => { language?: string };
  getSystemInfoSync?: () => { language?: string; platform?: string };
  getStorageSync?: (key: string) => unknown;
  setStorageSync?: (key: string, value: unknown) => void;
  removeStorageSync?: (key: string) => void;
  showToast?: (options: { title: string; icon?: string; duration?: number }) => void;
  stopPullDownRefresh?: () => void;
}

function platformApi(): WeixinHostApi | undefined {
  return (globalThis as unknown as { wx?: WeixinHostApi }).wx;
}

/**
 * WeChat host adapter. Every platform call is guarded twice over: the API may be
 * absent (Node-side contract tests, non-WeChat hosts), and the call itself may
 * throw on an unsupported capability. Both degrade to a reported error instead of
 * taking the launch down.
 */
export function createWeixinHostAdapter(
  api: WeixinHostApi | undefined = platformApi(),
): WebserverMpHostAdapter {
  const platform: WebserverMpHostPlatform = api ? "mp-weixin" : "unknown";

  return {
    platform,
    isAvailable: () => Boolean(api),
    getLocale: (): WebserverMpHostLocaleInfo => {
      let language = "";
      try {
        language = api?.getAppBaseInfo?.().language
          ?? api?.getSystemInfoSync?.().language
          ?? "";
      } catch {
        language = "";
      }
      return {
        language: normalizeWebserverMpPlatformLocale(language),
        platform,
      };
    },
    createStorage: (): WebserverMpHostStorage => ({
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
    }),
    showToast: (message): WebserverMpHostError | null => {
      if (!api?.showToast) {
        return createWebserverMpHostError(
          "capability-unsupported",
          "this host cannot show a toast",
        );
      }
      try {
        api.showToast({ title: message, icon: "none" });
        return null;
      } catch (error) {
        return createWebserverMpHostError("toast-failed", "showing a toast failed", error);
      }
    },
    stopPullDownRefresh: () => {
      try {
        api?.stopPullDownRefresh?.();
      } catch {
        // A missing refresh handle is not an error worth surfacing.
      }
    },
  };
}
