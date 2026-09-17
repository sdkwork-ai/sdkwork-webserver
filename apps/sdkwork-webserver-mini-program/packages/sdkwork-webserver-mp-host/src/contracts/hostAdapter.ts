/**
 * Mini program host adapter contract.
 *
 * Authority: `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8. Capability packages,
 * and the core session store, only ever see this interface — never a platform
 * global. Adapter methods normalize platform-specific failures into the stable
 * `WebserverMpHostError` vocabulary below so callers branch on a small closed set
 * instead of on vendor error strings.
 *
 * Only the categories the Web Server console currently needs are declared here.
 * The spec's remaining categories are added when a capability consumes them.
 */

export type WebserverMpHostPlatform = "mp-weixin" | "unknown";

/** Stable host error codes; platform error text never leaks to callers. */
export type WebserverMpHostErrorCode =
  | "host-unavailable"
  | "capability-unsupported"
  | "toast-failed"
  | "storage-failed";

export interface WebserverMpHostError {
  readonly code: WebserverMpHostErrorCode;
  readonly message: string;
  /** Vendor error message, kept for diagnostics only. */
  readonly cause?: string;
}

/** Synchronous, platform-backed key/value store (session persistence lives here). */
export interface WebserverMpHostStorage {
  get(key: string): string | null;
  set(key: string, value: string): void;
  remove(key: string): void;
}

export interface WebserverMpHostLocaleInfo {
  /** BCP 47 tag the platform reports, e.g. `zh_CN` or `en`. */
  readonly language: string;
  readonly platform: WebserverMpHostPlatform;
}

export interface WebserverMpHostAdapter {
  readonly platform: WebserverMpHostPlatform;
  /** Whether a real platform bridge is present in this runtime. */
  isAvailable(): boolean;
  /** Platform locale, used to negotiate the message catalog at launch. */
  getLocale(): WebserverMpHostLocaleInfo;
  createStorage(): WebserverMpHostStorage;
  /** Non-blocking user feedback; a failed toast is reported, never thrown. */
  showToast(message: string): WebserverMpHostError | null;
  /** End a pull-to-refresh gesture started by the platform. */
  stopPullDownRefresh(): void;
}

export function createWebserverMpHostError(
  code: WebserverMpHostErrorCode,
  message: string,
  cause?: unknown,
): WebserverMpHostError {
  const normalizedCause = typeof cause === "string"
    ? cause
    : cause instanceof Error
      ? cause.message
      : undefined;
  return {
    code,
    message,
    ...(normalizedCause ? { cause: normalizedCause } : {}),
  };
}

/**
 * Normalize the platform's `zh_CN`-style locale tag onto a BCP 47 candidate
 * (`zh-CN`). Only the tag shape is normalized here; mapping a candidate onto a
 * shipped locale is the commons catalog's job.
 */
export function normalizeWebserverMpPlatformLocale(language: string | undefined): string {
  const trimmed = (language ?? "").trim();
  if (!trimmed) return "";
  const [primary, region] = trimmed.replace(/_/gu, "-").split("-");
  const normalizedPrimary = (primary ?? "").toLowerCase();
  if (!normalizedPrimary) return "";
  const normalizedRegion = (region ?? "").toUpperCase();
  return normalizedRegion ? `${normalizedPrimary}-${normalizedRegion}` : normalizedPrimary;
}
