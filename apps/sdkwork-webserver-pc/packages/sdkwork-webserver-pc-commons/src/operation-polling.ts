export type WebserverOperationStatus =
  | "PENDING"
  | "RUNNING"
  | "SUCCEEDED"
  | "FAILED"
  | "CANCELLED";

export interface WebserverOperation {
  failureCode?: string;
  /**
   * What the failure said, as the provider worded it.
   *
   * `failureCode` classifies a failure so the UI can localize and the scheduler
   * can branch on it, which is precisely why it cannot also explain one. Without
   * this, a refused DNS credential, a revoked API token and an unowned zone all
   * reached the operator as the same string, and the only place the provider's
   * own sentence existed was the server log.
   */
  failureDetail?: string;
  status: WebserverOperationStatus | Lowercase<WebserverOperationStatus>;
}

export interface WebserverOperationRequestOptions {
  signal: AbortSignal;
  timeout: number;
}

export type WebserverOperationErrorKind = "cancelled" | "failed" | "invalid-status" | "timeout";

export class WebserverOperationError extends Error {
  constructor(
    readonly kind: WebserverOperationErrorKind,
    readonly operationId: string,
    readonly failureCode: string,
    readonly failureDetail?: string,
    options?: ErrorOptions,
  ) {
    super(failureCode, options);
    this.name = "WebserverOperationError";
  }
}

export interface PollWebserverOperationOptions {
  intervalMs?: number;
  signal?: AbortSignal;
  timeoutMs?: number;
}

export type RetrieveWebserverOperation<T extends WebserverOperation> = (
  operationId: string,
  options: WebserverOperationRequestOptions,
) => Promise<T>;

const DEFAULT_INTERVAL_MS = 2_000;
const DEFAULT_TIMEOUT_MS = 10 * 60 * 1_000;
export const WEBSERVER_OPERATION_REQUEST_TIMEOUT_MS = 30_000;

export function webserverOperationRequestOptions(signal?: AbortSignal): WebserverOperationRequestOptions {
  return {
    signal: signal ?? new AbortController().signal,
    timeout: WEBSERVER_OPERATION_REQUEST_TIMEOUT_MS,
  };
}

export async function pollWebserverOperation<T extends WebserverOperation>(
  operationId: string,
  retrieve: RetrieveWebserverOperation<T>,
  options: PollWebserverOperationOptions = {},
): Promise<T> {
  const normalizedOperationId = operationId.trim();
  if (!normalizedOperationId) throw new Error("Certificate operation ID is required");

  const intervalMs = positiveDuration(options.intervalMs, DEFAULT_INTERVAL_MS, "Polling interval");
  const timeoutMs = positiveDuration(options.timeoutMs, DEFAULT_TIMEOUT_MS, "Polling timeout");
  const controller = new AbortController();
  const timeoutError = new WebserverOperationError(
    "timeout",
    normalizedOperationId,
    "CERTIFICATE_OPERATION_POLL_TIMEOUT",
  );
  const forwardAbort = () => controller.abort(options.signal?.reason);
  options.signal?.throwIfAborted();
  options.signal?.addEventListener("abort", forwardAbort, { once: true });
  const timeout = globalThis.setTimeout(() => controller.abort(timeoutError), timeoutMs);

  try {
    while (true) {
      const operation = await retrieve(normalizedOperationId, {
        signal: controller.signal,
        timeout: WEBSERVER_OPERATION_REQUEST_TIMEOUT_MS,
      });
      const status = normalizedStatus(operation.status);
      if (status === "SUCCEEDED") return operation;
      if (status === "FAILED") {
        throw new WebserverOperationError(
          "failed",
          normalizedOperationId,
          normalizedFailureCode(operation.failureCode, "CERTIFICATE_OPERATION_FAILED"),
          operation.failureDetail,
        );
      }
      if (status === "CANCELLED") {
        throw new WebserverOperationError(
          "cancelled",
          normalizedOperationId,
          normalizedFailureCode(operation.failureCode, "CERTIFICATE_OPERATION_CANCELLED"),
        );
      }
      if (status !== "PENDING" && status !== "RUNNING") {
        throw new WebserverOperationError(
          "invalid-status",
          normalizedOperationId,
          "CERTIFICATE_OPERATION_STATUS_INVALID",
        );
      }
      await abortableDelay(intervalMs, controller.signal);
    }
  } catch (error) {
    if (controller.signal.aborted && controller.signal.reason === timeoutError) throw timeoutError;
    throw error;
  } finally {
    globalThis.clearTimeout(timeout);
    options.signal?.removeEventListener("abort", forwardAbort);
  }
}

function abortableDelay(durationMs: number, signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const timeout = globalThis.setTimeout(() => {
      signal.removeEventListener("abort", abort);
      resolve();
    }, durationMs);
    const abort = () => {
      globalThis.clearTimeout(timeout);
      reject(signal.reason);
    };
    signal.addEventListener("abort", abort, { once: true });
  });
}

function normalizedStatus(value: string): string {
  return value.trim().toUpperCase();
}

function normalizedFailureCode(value: string | undefined, fallback: string): string {
  const normalized = value?.trim().toUpperCase();
  return normalized && /^[A-Z][A-Z0-9_]{2,127}$/.test(normalized) ? normalized : fallback;
}

function positiveDuration(value: number | undefined, fallback: number, label: string): number {
  const normalized = value ?? fallback;
  if (!Number.isSafeInteger(normalized) || normalized <= 0) throw new Error(`${label} must be a positive integer`);
  return normalized;
}
