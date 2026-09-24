import { useEffect, useRef, type ReactNode } from "react";

/**
 * Drawer and confirm dialog for this surface.
 *
 * A local copy rather than an import: the shared drawer/modal styling lives in
 * the host's stylesheet under the `sdkwork-surface-*` classes, and the console
 * capability packages each carry their own markup for it (plugins, mcp) so a
 * capability package never has to depend on a sibling capability package to
 * draw a dialog. Importing the stylesheet classes is deliberate; importing
 * another package's component would be the coupling this avoids.
 */

export interface SurfaceDrawerProps {
  open: boolean;
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  size?: "md" | "lg";
}

export function SurfaceDrawer({
  open,
  title,
  description,
  onClose,
  children,
  footer,
  size = "lg",
}: SurfaceDrawerProps) {
  const panelRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const previouslyFocused = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    panelRef.current?.focus();
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKeyDown);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = previousOverflow;
      previouslyFocused?.focus();
    };
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div className="sdkwork-surface-drawer-root" role="presentation">
      <button
        type="button"
        className="sdkwork-surface-drawer-backdrop"
        aria-label="Close"
        onClick={onClose}
      />
      <aside
        ref={panelRef}
        className={`sdkwork-surface-drawer-panel sdkwork-surface-drawer-panel--${size}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="sdkwork-surface-drawer-title"
        tabIndex={-1}
      >
        <header className="sdkwork-surface-drawer-header">
          <div>
            <h3 id="sdkwork-surface-drawer-title">{title}</h3>
            {description ? <p>{description}</p> : null}
          </div>
          <button
            type="button"
            className="sdkwork-surface-drawer-close"
            onClick={onClose}
            aria-label="Close"
          >
            ×
          </button>
        </header>
        <div className="sdkwork-surface-drawer-body">{children}</div>
        {footer ? <footer className="sdkwork-surface-drawer-footer">{footer}</footer> : null}
      </aside>
    </div>
  );
}

export interface ConfirmModalProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  cancelLabel: string;
  busy?: boolean;
  tone?: "danger" | "default";
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmModal({
  open,
  title,
  description,
  confirmLabel,
  cancelLabel,
  busy = false,
  tone = "danger",
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  if (!open) return null;
  return (
    <div className="sdkwork-surface-modal-root" role="presentation">
      <button
        type="button"
        className="sdkwork-surface-drawer-backdrop"
        aria-label="Close"
        onClick={onCancel}
        disabled={busy}
      />
      <div
        className="sdkwork-surface-modal-panel"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="sdkwork-surface-modal-title"
        aria-describedby="sdkwork-surface-modal-description"
      >
        <h3 id="sdkwork-surface-modal-title">{title}</h3>
        <p id="sdkwork-surface-modal-description">{description}</p>
        <div className="sdkwork-surface-modal-actions">
          <button type="button" className="sdkwork-surface-modal-cancel" onClick={onCancel} disabled={busy}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className={
              tone === "danger"
                ? "sdkwork-surface-modal-confirm sdkwork-surface-modal-confirm--danger"
                : "sdkwork-surface-modal-confirm"
            }
            onClick={onConfirm}
            disabled={busy}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Field/section primitives shared by the provisioning form. */
export function ConsoleField({
  error,
  hint,
  htmlFor,
  label,
  optionalLabel,
  required = false,
  children,
}: {
  error?: string;
  hint?: ReactNode;
  htmlFor?: string;
  label: string;
  optionalLabel?: string;
  required?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="skills-console-field">
      <label className="skills-console-field-label" htmlFor={htmlFor}>
        {label}
        {required ? <span className="skills-console-field-required" aria-hidden="true">*</span> : null}
        {!required && optionalLabel ? (
          <span className="skills-console-field-optional">{optionalLabel}</span>
        ) : null}
      </label>
      {children}
      {error ? (
        <small className="form-error" role="alert">{error}</small>
      ) : null}
      {hint ? <small className="skills-console-field-hint">{hint}</small> : null}
    </div>
  );
}

export function ConsoleFormSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="plugin-form-section">
      <header className="plugin-form-section-header">
        <h4>{title}</h4>
      </header>
      <div className="plugin-form-section-body">{children}</div>
    </section>
  );
}
