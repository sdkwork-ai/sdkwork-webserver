import type { ReactNode } from "react";

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
        className={`sdkwork-surface-drawer-panel sdkwork-surface-drawer-panel--${size}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="sdkwork-surface-drawer-title"
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
