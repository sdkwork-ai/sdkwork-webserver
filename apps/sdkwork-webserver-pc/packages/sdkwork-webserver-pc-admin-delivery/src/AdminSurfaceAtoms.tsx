import { translateWebserver, type WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { useState, type FormEvent, type ReactNode } from "react";

/**
 * The chrome both tenant-level delivery ledgers are built from.
 *
 * These are the console's own atoms, re-authored here rather than imported:
 * `@sdkwork/deployments-pc-console-delivery` is the module that *owns* the
 * console pages, and a Web Server capability package reaching into it would
 * invert that ownership. What the two pages actually share is the *design*, and
 * the design is the class vocabulary in `src/deploy-surface.css` — so the shared
 * thing is the markup these helpers emit, not a dependency between the packages.
 * The rules are the same rules, which is what makes the two pages read as one
 * product.
 *
 * Kept out of the package's `index.ts` on purpose: `export *` from both surface
 * modules would collide on these names, and no consumer outside this package
 * needs them.
 */

export type MessageKey = Parameters<typeof translateWebserver>[1];

export type Translator = (key: MessageKey, values?: Record<string, string | number>) => string;

export function translator(locale: WebserverLocale): Translator {
  return (key, values) => translateWebserver(locale, key, values);
}

export function errorText(cause: unknown): string | undefined {
  return cause instanceof Error && cause.message ? cause.message : undefined;
}

export function formatInstant(instant: string | undefined, locale: WebserverLocale): string {
  if (!instant) return "-";
  const parsed = new Date(instant);
  if (Number.isNaN(parsed.getTime())) return instant;
  return parsed.toLocaleString(locale === "zh-CN" ? "zh-CN" : "en-US", { hour12: false });
}

/**
 * Idempotency key for one mutation.
 *
 * Same shape `webserver-config-client.ts` uses: `crypto.randomUUID` is absent —
 * or throws — outside a secure context, and a dev edge reached over plain HTTP at
 * a LAN address is exactly that, so the UUID is assembled from
 * `getRandomValues` rather than assumed.
 */
export function newIdempotencyKey(): string {
  const crypto = globalThis.crypto;
  if (typeof crypto?.randomUUID === "function") {
    try {
      return crypto.randomUUID();
    } catch {
      // Non-secure contexts may reject randomUUID; fall through.
    }
  }
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * Status chip.
 *
 * Class-driven, exactly like the console's: the tone comes from the shared
 * `status-*` palette in the Deployments stylesheet rather than a parallel
 * variant prop, so a state with no rule of its own (an `ISSUED` certificate
 * included) reads in the neutral tone instead of silently borrowing a colour
 * that means something else. A value this build does not know at all prints
 * itself.
 */
const STATUS_LABELS: Partial<Record<string, MessageKey>> = {
  ACTIVE: "resource.domains.active",
  ARCHIVED: "resource.certificates.statusArchived",
  DISABLED: "resource.domains.disabled",
  EXPIRED: "resource.certificates.statusExpired",
  FAILED: "resource.certificates.statusFailed",
  ISSUED: "resource.certificates.statusIssued",
  PENDING: "resource.domains.pending",
  REVOKED: "resource.certificates.statusRevoked",
  VERIFIED: "resource.domains.verificationVerified",
};

export function StatusBadge({ t, value }: { t: Translator; value: string }) {
  const key = STATUS_LABELS[value];
  return <span className={`status-badge status-${value.toLowerCase()}`}>{key ? t(key) : value}</span>;
}

/**
 * Offset-plane pager.
 *
 * Both planes report `hasMore` and a page number but no total, so the summary is
 * the page label — the same fallback the console uses when a count is absent.
 */
export function Pagination({
  busy,
  hasMore,
  onNext,
  onPrevious,
  page,
  t,
}: {
  busy: boolean;
  hasMore: boolean;
  onNext(): void;
  onPrevious(): void;
  page: number;
  t: Translator;
}) {
  return (
    <footer className="pagination">
      <span>{t("resource.domains.page", { page })}</span>
      <button
        className="icon-button"
        disabled={page <= 1 || busy}
        onClick={onPrevious}
        title={t("resource.domains.previous")}
        type="button"
      >
        <ChevronLeft size={18} />
      </button>
      <button
        className="icon-button"
        disabled={!hasMore || busy}
        onClick={onNext}
        title={t("resource.domains.next")}
        type="button"
      >
        <ChevronRight size={18} />
      </button>
    </footer>
  );
}

export function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="domain-metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

/**
 * Single-value create form in the console's dialog chrome.
 *
 * The one field is whatever the caller renders; the form extracts the first
 * named control's value, which keeps the two single-field dialogs (a root
 * hostname, a relative record name) on one implementation.
 */
export function FormDialog({
  children,
  close,
  submit,
  submitLabel,
  t,
  title,
}: {
  children(disabled: boolean): ReactNode;
  close(): void;
  submit(value: string): Promise<void>;
  submitLabel: string;
  t: Translator;
  title: string;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const first = new FormData(event.currentTarget).values().next();
    const value = typeof first.value === "string" ? first.value.trim() : "";
    // The control is `required`, so an empty submit is a browser-level refusal;
    // this is the guard for a caller that renders a non-native control.
    if (!value) return;
    setBusy(true);
    setError(undefined);
    void submit(value)
      .catch((cause) => setError(errorText(cause)))
      .finally(() => setBusy(false));
  };

  return (
    <DialogBackdrop close={close}>
      <form
        aria-labelledby="served-domain-dialog-title"
        aria-modal="true"
        className="dialog delivery-dialog"
        onSubmit={onSubmit}
        role="dialog"
      >
        <header>
          <h2 id="served-domain-dialog-title">{title}</h2>
          <DialogCloseButton close={close} label={t("resource.domains.cancel")} />
        </header>
        <div className="form-grid single-column">{children(busy)}</div>
        {error ? (
          <div className="error-banner" role="alert">
            {error}
          </div>
        ) : null}
        <footer className="dialog-footer">
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.domains.cancel")}
          </button>
          <button className="command-button" disabled={busy} type="submit">
            {submitLabel}
          </button>
        </footer>
      </form>
    </DialogBackdrop>
  );
}

/**
 * Destructive-or-not confirmation in the console's dialog chrome.
 *
 * `extra` carries the one confirmation that needs an argument — a revocation
 * reason — so the revoke flow does not need a dialog of its own.
 */
export function ConfirmDialog({
  close,
  confirmLabel,
  dangerous = false,
  extra,
  message,
  onConfirm,
  t,
  title,
}: {
  close(): void;
  confirmLabel: string;
  dangerous?: boolean;
  extra?: ReactNode;
  message: string;
  onConfirm(): void;
  t: Translator;
  title: string;
}) {
  return (
    <DialogBackdrop close={close}>
      <div aria-labelledby="served-domain-confirm-title" aria-modal="true" className="dialog delivery-dialog" role="dialog">
        <header>
          <h2 id="served-domain-confirm-title">{title}</h2>
          <DialogCloseButton close={close} label={t("resource.domains.cancel")} />
        </header>
        <p className={dangerous ? "confirmation-message dangerous-confirmation" : "confirmation-message"}>{message}</p>
        {extra ? <div className="form-grid single-column">{extra}</div> : null}
        <footer className="dialog-footer">
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.domains.cancel")}
          </button>
          <button className={dangerous ? "danger-button" : "command-button"} onClick={onConfirm} type="button">
            {confirmLabel}
          </button>
        </footer>
      </div>
    </DialogBackdrop>
  );
}

function DialogBackdrop({ children, close }: { children: ReactNode; close(): void }) {
  return (
    // A click on the backdrop itself closes; a click anywhere inside does not,
    // which is why the handler compares the event target against the backdrop
    // rather than relying on propagation.
    <div
      className="dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
      role="presentation"
    >
      {children}
    </div>
  );
}

function DialogCloseButton({ close, label }: { close(): void; label: string }) {
  return (
    <button className="icon-button" onClick={close} title={label} type="button">
      <X size={18} />
    </button>
  );
}
