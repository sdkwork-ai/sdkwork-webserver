import type { WebserverMessageKey } from "./i18n/index.ts";
import { isRecord } from "./normalize.ts";
import { WebserverOperationError } from "./operation-polling.ts";

export type WebserverErrorTranslate = (
  key: WebserverMessageKey,
  values?: Record<string, string | number>,
) => string;

export interface WebserverErrorMessageOptions {
  fallbackKey?: WebserverMessageKey;
}

const RESULT_CODE_KEYS: Readonly<Record<number, WebserverMessageKey>> = {
  40001: "errors.result.40001",
  40002: "errors.result.40002",
  40003: "errors.result.40003",
  40004: "errors.result.40004",
  40101: "errors.result.40101",
  40102: "errors.result.40102",
  40103: "errors.result.40103",
  40104: "errors.result.40104",
  40301: "errors.result.40301",
  40302: "errors.result.40302",
  40303: "errors.result.40303",
  40304: "errors.result.40304",
  40401: "errors.result.40401",
  40501: "errors.result.40501",
  40801: "errors.result.40801",
  40901: "errors.result.40901",
  41001: "errors.result.41001",
  41201: "errors.result.41201",
  41301: "errors.result.41301",
  41501: "errors.result.41501",
  42201: "errors.result.42201",
  42301: "errors.result.42301",
  42801: "errors.result.42801",
  42901: "errors.result.42901",
  50001: "errors.result.50001",
  50201: "errors.result.50201",
  50301: "errors.result.50301",
  50401: "errors.result.50401",
  60001: "errors.result.60001",
  60002: "errors.result.60002",
  60003: "errors.result.60003",
  60004: "errors.result.60004",
  60005: "errors.result.60005",
  70001: "errors.result.70001",
  70002: "errors.result.70002",
};

const SDK_ERROR_KEYS: Readonly<Record<string, WebserverMessageKey>> = {
  BAD_GATEWAY: "errors.result.50201",
  CANCELLED: "error.cancelled",
  CONFLICT: "errors.result.40901",
  FORBIDDEN: "errors.result.40301",
  GATEWAY_TIMEOUT: "errors.result.50401",
  NETWORK_ERROR: "error.network",
  NOT_FOUND: "errors.result.40401",
  RATE_LIMIT: "errors.result.42901",
  SERVER_ERROR: "errors.result.50001",
  SERVICE_UNAVAILABLE: "errors.result.50301",
  TIMEOUT: "error.timeout",
  TOKEN_EXPIRED: "errors.result.40102",
  TOKEN_INVALID: "errors.result.40103",
  UNAUTHORIZED: "errors.result.40101",
  VALIDATION_ERROR: "errors.result.40001",
};

const STATUS_ERROR_KEYS: Readonly<Record<number, WebserverMessageKey>> = {
  400: "errors.result.40001",
  401: "errors.result.40101",
  403: "errors.result.40301",
  404: "errors.result.40401",
  405: "errors.result.40501",
  408: "errors.result.40801",
  409: "errors.result.40901",
  410: "errors.result.41001",
  412: "errors.result.41201",
  413: "errors.result.41301",
  415: "errors.result.41501",
  422: "errors.result.42201",
  423: "errors.result.42301",
  428: "errors.result.42801",
  429: "errors.result.42901",
  500: "errors.result.50001",
  502: "errors.result.50201",
  503: "errors.result.50301",
  504: "errors.result.50401",
};

interface StructuredError {
  code?: number;
  details: readonly unknown[];
  problemDetail?: string;
  sdkCode?: string;
  status?: number;
  traceId?: string;
}

export function formatWebserverErrorMessage(
  error: unknown,
  translate: WebserverErrorTranslate,
  options: WebserverErrorMessageOptions = {},
): string {
  if (error instanceof WebserverOperationError) {
    // The provider's own sentence is the only part of a failed operation an
    // operator can act on: the code says a provider refused something, not which
    // credential or which zone, and it is stable precisely because retry and
    // cooldown logic branch on it. The detail comes from a third-party HTTP
    // response body, so it goes through the same guard as any other server-
    // authored text rather than being trusted because it arrived over our API.
    const failureDetail = safeDisplayText(error.failureDetail, 320);
    if (error.kind === "failed" && failureDetail) {
      return translate("error.asyncOperationFailedDetail", {
        failureCode: error.failureCode,
        failureDetail,
      });
    }
    const key = error.kind === "timeout"
      ? "error.asyncOperationTimeout"
      : error.kind === "cancelled"
        ? "error.asyncOperationCancelled"
        : "error.asyncOperationFailed";
    return translate(key, { failureCode: error.failureCode });
  }

  return structuredErrorMessage(error, translate)
    ?? translate(options.fallbackKey ?? "error.operation");
}

function structuredErrorMessage(error: unknown, translate: WebserverErrorTranslate): string | undefined {
  const structured = readStructuredError(error);
  if (!structured) return undefined;

  const key = errorMessageKey(structured);
  const parts = [translate(key)];
  const fieldDetails = canDisplayFieldDetails(structured.status, structured.code)
    ? formatFieldDetails(structured.details)
    : undefined;
  if (fieldDetails) {
    parts.push(translate("error.fieldDetails", { details: fieldDetails }));
  } else if (structured.problemDetail && canDisplayProblemDetail(structured.status, structured.code)) {
    const detail = safeDisplayText(structured.problemDetail, 320);
    if (detail && !parts[0].toLocaleLowerCase().includes(detail.toLocaleLowerCase())) {
      parts.push(detail);
    }
  }
  if (structured.traceId) {
    parts.push(translate("error.traceReference", { traceId: structured.traceId }));
  }
  return parts.join(" ");
}

function readStructuredError(
  error: unknown,
  seen: Set<unknown> = new Set(),
  depth = 0,
): StructuredError | undefined {
  if (!isRecord(error) || seen.has(error) || depth >= 5) return undefined;
  seen.add(error);
  const problem = isRecord(error.problem) ? error.problem : undefined;
  const sdkCode = typeof error.code === "string" ? error.code.toUpperCase() : undefined;
  const code = numericCode(problem?.code) ?? numericCode(error.code) ?? numericCode(error.businessCode);
  const status = httpStatus(error.httpStatus) ?? httpStatus(problem?.status) ?? statusFromResultCode(code);
  const networkLike = sdkCode ? sdkCode in SDK_ERROR_KEYS : isKnownBrowserTransportError(error);
  if (!problem && !code && !status && !networkLike) {
    return readStructuredError(error.cause, seen, depth + 1);
  }
  return {
    code,
    details: [
      ...arrayValue(error.details),
      ...arrayValue(problem?.errors),
    ],
    problemDetail: typeof problem?.detail === "string" ? problem.detail : undefined,
    sdkCode: sdkCode ?? browserTransportCode(error),
    status,
    traceId: safeTraceId(error.traceId) ?? safeTraceId(problem?.traceId),
  };
}

function errorMessageKey(error: StructuredError): WebserverMessageKey {
  if (error.code && RESULT_CODE_KEYS[error.code]) return RESULT_CODE_KEYS[error.code];
  if (error.sdkCode && SDK_ERROR_KEYS[error.sdkCode]) return SDK_ERROR_KEYS[error.sdkCode];
  if (error.status && STATUS_ERROR_KEYS[error.status]) return STATUS_ERROR_KEYS[error.status];
  if (error.status && error.status >= 500) return "error.serviceUnavailable";
  if (error.status && error.status >= 400 && error.status < 500) return "error.validation";
  return "error.operation";
}

function formatFieldDetails(details: readonly unknown[]): string | undefined {
  const messages: string[] = [];
  for (const detail of details) {
    if (!isRecord(detail)) continue;
    const field = safeFieldPath(detail.field);
    const message = safeDisplayText(detail.message, 160);
    const rendered = field && message ? `${field}: ${message}` : message ?? field;
    if (rendered && !messages.includes(rendered)) messages.push(rendered);
    if (messages.length === 4) break;
  }
  return messages.length > 0 ? messages.join("; ") : undefined;
}

function safeFieldPath(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.trim().replace(/^\/+/, "").replaceAll("/", ".");
  return /^[A-Za-z0-9_.\[\]-]{1,80}$/.test(normalized) ? normalized : undefined;
}

function safeDisplayText(value: unknown, maxLength: number): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.replace(/\s+/g, " ").trim();
  if (!normalized || containsSensitiveDiagnostic(normalized)) return undefined;
  return normalized.length <= maxLength
    ? normalized
    : `${normalized.slice(0, maxLength - 3).trimEnd()}...`;
}

function containsSensitiveDiagnostic(value: string): boolean {
  return [
    /-----BEGIN [A-Z ]*PRIVATE KEY-----/i,
    /\b(?:authorization|access[-_ ]?token|refresh[-_ ]?token|password|private[-_ ]?key|api[-_ ]?key|secret)\s*[:=]\s*["']?\S+/i,
    /\b(?:bearer\s+[A-Za-z0-9._~-]+|eyJ[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,})/i,
    /\b(?:select\s+.+\s+from|insert\s+into|update\s+.+\s+set|delete\s+from)\b/i,
    /\b(?:sqlx|postgres(?:ql)?|mysql|sqlite|ora-\d+)\b/i,
    /\b(?:stack trace|traceback)\b|\bat\s+\S+\s*\([^)]*:\d+:\d+\)/i,
    /(?:[A-Za-z]:\\|\/(?:home|etc|usr|var)\/)[^\s]+/i,
    /<\/?[A-Za-z][^>]*>/,
    /\b[A-Za-z0-9_-]{96,}\b/,
  ].some((pattern) => pattern.test(value));
}

function canDisplayProblemDetail(status?: number, code?: number): boolean {
  if (code && code >= 60000 && code < 70000) return status === undefined || status < 500;
  return status !== undefined && [400, 405, 408, 409, 410, 412, 413, 415, 422, 423, 428, 429].includes(status);
}

function canDisplayFieldDetails(status?: number, code?: number): boolean {
  return status === 400 || status === 422 || Boolean(code && code >= 40001 && code <= 40099);
}

function safeTraceId(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.trim();
  return normalized !== "unknown" && /^[A-Za-z0-9][A-Za-z0-9._:-]{5,127}$/.test(normalized)
    ? normalized
    : undefined;
}

function numericCode(value: unknown): number | undefined {
  const parsed = typeof value === "number"
    ? value
    : typeof value === "string" && /^\d+$/.test(value)
      ? Number(value)
      : undefined;
  return parsed && Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

function httpStatus(value: unknown): number | undefined {
  return typeof value === "number" && Number.isInteger(value) && value >= 400 && value <= 599
    ? value
    : undefined;
}

function statusFromResultCode(code?: number): number | undefined {
  if (!code || code < 40000 || code > 59999) return undefined;
  return httpStatus(Math.trunc(code / 100));
}

function browserTransportCode(error: Record<string, unknown>): string | undefined {
  if (error.name === "AbortError") return "CANCELLED";
  if (error.name === "TimeoutError") return "TIMEOUT";
  if (
    error.name === "TypeError"
    && typeof error.message === "string"
    && /failed to fetch|load failed|network(?: error| request failed)/i.test(error.message)
  ) return "NETWORK_ERROR";
  return undefined;
}

function isKnownBrowserTransportError(error: Record<string, unknown>): boolean {
  return browserTransportCode(error) !== undefined;
}

function arrayValue(value: unknown): readonly unknown[] {
  return Array.isArray(value) ? value : [];
}
