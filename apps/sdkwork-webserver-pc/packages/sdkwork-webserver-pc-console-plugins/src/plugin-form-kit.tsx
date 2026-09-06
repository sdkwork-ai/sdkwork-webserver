import type { ReactNode } from "react";

/**
 * Shared field/section primitives for the plugin console create/edit drawers.
 * Fields are div-based (not label-wrapped) so file inputs can live inside
 * sections without nested-label validity issues; pair `htmlFor` with an
 * explicit input `id`.
 */
export function ConsoleField({
  hint,
  htmlFor,
  label,
  optionalLabel,
  required = false,
  children,
}: {
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
