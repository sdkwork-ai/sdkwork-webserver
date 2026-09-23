import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type { ApplicationDomainResponse, CertificateResponse } from "@sdkwork/webserver-pc-admin-core";
import type { WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { Ban, CalendarClock, FileKey2, Plus, RefreshCw, RotateCw, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import {
  ConfirmDialog,
  Pagination,
  StatusBadge,
  errorText,
  formatInstant,
  newIdempotencyKey,
  translator,
  type MessageKey,
  type Translator,
} from "./AdminSurfaceAtoms.tsx";

/**
 * TLS certificate lifecycle for the hostnames this edge serves.
 *
 * The certificate plane is the Web Server's own (`webserver_certificate` and its
 * version / operation / binding tables), so an operator can see here which of the
 * served hostnames are covered and what state each certificate is in, without
 * leaving the edge that terminates the TLS in the first place.
 *
 * Issuance needs an identifier set, so the issue form is driven by the served
 * domain inventory (`/domains`) rather than by free text: the operator ticks the
 * hostnames, and the ids come from the same rows the Domains page shows. That
 * keeps a certificate from naming a hostname this edge does not answer for.
 *
 * ## Look
 *
 * Same Deployments surface vocabulary as `ServedDomainAdminSurface` — see the
 * long note there for why the tenant-level pair is dressed in the console's
 * stylesheet rather than the host's registry chrome. The command bar, the
 * certificate ledger, the identifier chips, the status chips, the dialogs and the
 * pager are the console's rules; the console's Certificates page carries no
 * search box either, because this plane offers no free-text parameter.
 *
 * The page renders inside `WebserverAdminSdkProvider`; no transport is built here.
 */

const PAGE_SIZE = 50;

const REVOKE_REASONS = [
  "keyCompromise",
  "affiliationChanged",
  "superseded",
  "cessationOfOperation",
  "privilegeWithdrawn",
] as const;

type RevokeReason = (typeof REVOKE_REASONS)[number];
type CertificateType = 1 | 3;

export interface ServedCertificateAdminSurfaceProps {
  locale: WebserverLocale;
  resource: "certificates";
}

export function ServedCertificateAdminSurface({ locale, resource }: ServedCertificateAdminSurfaceProps) {
  return (
    <div className="deploy-surface" data-resource={resource}>
      <CertificateLedger locale={locale} />
    </div>
  );
}

function CertificateLedger({ locale }: { locale: WebserverLocale }) {
  const client = useWebserverAdminSdk();
  const t = translator(locale);
  const [certificates, setCertificates] = useState<CertificateResponse[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [build, setBuild] = useState(0);
  const [issueOpen, setIssueOpen] = useState(false);
  const [renewTarget, setRenewTarget] = useState<CertificateResponse>();
  const [revokeTarget, setRevokeTarget] = useState<CertificateResponse>();
  const [deleteTarget, setDeleteTarget] = useState<CertificateResponse>();

  useEffect(() => {
    let active = true;
    setBusy(true);
    setError(undefined);
    void client.certificate
      .list({ page, pageSize: PAGE_SIZE })
      .then((result) => {
        if (!active) return;
        setCertificates(result.items);
        // `PageInfo.hasMore` is optional on the wire; an absent flag means "no
        // continuation", never "unknown", so it folds to `false` rather than
        // leaking `undefined` into the pager.
        setHasMore(result.pageInfo.hasMore === true);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause));
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [build, client, page]);

  const reload = () => {
    setCertificates(null);
    setBuild((value) => value + 1);
  };

  const run = (operation: () => Promise<unknown>, done?: () => void) => {
    setBusy(true);
    void operation()
      .then(() => {
        done?.();
        reload();
      })
      .catch((cause) => {
        done?.();
        setError(errorText(cause));
      })
      .finally(() => setBusy(false));
  };

  return (
    <section className="resource-page domain-page">
      <div className="resource-commandbar">
        <div className="resource-identity">
          <h1>{t("resource.certificates.admin.label")}</h1>
        </div>
        <div className="actions">
          <button className="icon-button" disabled={busy} onClick={reload} title={t("resource.domains.refresh")} type="button">
            <RefreshCw size={17} />
          </button>
          <button className="command-button" onClick={() => setIssueOpen(true)} type="button">
            <Plus size={16} />
            {t("resource.certificates.issue")}
          </button>
        </div>
      </div>

      {error ? (
        <div className="error-banner" role="alert">
          {error}
        </div>
      ) : null}

      {certificates === null && !error ? (
        <div className="resource-loading" role="status">
          <p>{t("resource.certificates.loading")}</p>
        </div>
      ) : (
        <>
          <div aria-busy={busy} className="table-frame domain-table-frame certificate-table-frame">
            <table className="domain-table">
              <thead>
                <tr>
                  <th>{t("resource.certificates.certName")}</th>
                  <th>{t("resource.certificates.identifiers")}</th>
                  <th>{t("resource.certificates.status")}</th>
                  <th>{t("resource.certificates.issuer")}</th>
                  <th>{t("resource.certificates.keyAlgorithm")}</th>
                  <th>{t("resource.certificates.notAfter")}</th>
                  <th>{t("resource.certificates.autoRenew")}</th>
                  <th className="operations-column">{t("resource.domains.operations")}</th>
                </tr>
              </thead>
              <tbody>
                {(certificates ?? []).map((certificate) => (
                  <tr key={certificate.id}>
                    <td>
                      <span className="certificate-name">
                        <FileKey2 size={17} />
                        <strong>{certificate.certName}</strong>
                      </span>
                    </td>
                    <td>
                      {/* The identifiers are a set, not a sentence: an operator
                          scans them for the one hostname they came about. */}
                      <div className="identifier-list">
                        {certificate.identifiers.length > 0 ? (
                          certificate.identifiers.map((identifier) => (
                            <span key={`${identifier.domainId}-${identifier.position}`}>{identifier.hostname}</span>
                          ))
                        ) : (
                          <span>-</span>
                        )}
                      </div>
                    </td>
                    <td>
                      <StatusBadge t={t} value={certificate.status} />
                    </td>
                    <td>{certificate.issuer || "-"}</td>
                    <td>{certificate.keyAlgorithm}</td>
                    <td>{formatInstant(certificate.notAfter, locale)}</td>
                    <td>{certificate.autoRenew === true ? t("resource.domains.yes") : t("resource.domains.no")}</td>
                    <td>
                      <div className="row-actions">
                        <button
                          aria-label={`${t("resource.certificates.renew")} ${certificate.certName}`}
                          className="table-action"
                          disabled={busy || certificate.status === "REVOKED"}
                          onClick={() => setRenewTarget(certificate)}
                          title={t("resource.certificates.renew")}
                          type="button"
                        >
                          <RotateCw size={16} />
                        </button>
                        <button
                          aria-label={`${
                            certificate.autoRenew === true
                              ? t("resource.certificates.autoRenewOff")
                              : t("resource.certificates.autoRenewOn")
                          } ${certificate.certName}`}
                          className="table-action"
                          disabled={busy || certificate.status === "REVOKED"}
                          onClick={() =>
                            run(() =>
                              client.certificate.update(
                                certificate.id,
                                { autoRenew: certificate.autoRenew !== true },
                                { idempotencyKey: newIdempotencyKey() },
                              ),
                            )
                          }
                          title={
                            certificate.autoRenew === true
                              ? t("resource.certificates.autoRenewOff")
                              : t("resource.certificates.autoRenewOn")
                          }
                          type="button"
                        >
                          <CalendarClock size={16} />
                        </button>
                        <button
                          aria-label={`${t("resource.certificates.revoke")} ${certificate.certName}`}
                          className="table-action danger-action"
                          disabled={busy || certificate.status === "REVOKED"}
                          onClick={() => setRevokeTarget(certificate)}
                          title={t("resource.certificates.revoke")}
                          type="button"
                        >
                          <Ban size={16} />
                        </button>
                        {/* Row delete stays separate from revocation: revoking
                            withdraws the certificate while keeping the record
                            that it existed, deleting forgets the record. */}
                        <button
                          aria-label={`${t("resource.domains.delete")} ${certificate.certName}`}
                          className="table-action danger-action"
                          disabled={busy}
                          onClick={() => setDeleteTarget(certificate)}
                          title={t("resource.domains.delete")}
                          type="button"
                        >
                          <Trash2 size={16} />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!busy && (certificates ?? []).length === 0 ? (
              <div className="empty-state">
                <FileKey2 size={24} />
                {t("resource.certificates.noCertificates")}
              </div>
            ) : null}
          </div>
          <Pagination
            busy={busy}
            hasMore={hasMore}
            onNext={() => setPage((current) => current + 1)}
            onPrevious={() => setPage((current) => Math.max(1, current - 1))}
            page={page}
            t={t}
          />
        </>
      )}

      {issueOpen ? (
        <IssueCertificateDialog
          close={() => setIssueOpen(false)}
          done={reload}
          onError={setError}
          t={t}
        />
      ) : null}

      {renewTarget ? (
        <ConfirmDialog
          close={() => setRenewTarget(undefined)}
          confirmLabel={t("resource.certificates.renew")}
          message={t("resource.certificates.renewConfirm", { name: renewTarget.certName })}
          onConfirm={() =>
            run(() => client.certificate.renew(renewTarget.id, { idempotencyKey: newIdempotencyKey() }), () =>
              setRenewTarget(undefined),
            )
          }
          t={t}
          title={t("resource.certificates.renew")}
        />
      ) : null}

      {revokeTarget ? (
        <RevokeDialog
          certificate={revokeTarget}
          close={() => setRevokeTarget(undefined)}
          done={reload}
          onError={setError}
          t={t}
        />
      ) : null}

      {deleteTarget ? (
        <ConfirmDialog
          close={() => setDeleteTarget(undefined)}
          confirmLabel={t("resource.domains.delete")}
          dangerous
          message={t("resource.certificates.deleteConfirm", { name: deleteTarget.certName })}
          onConfirm={() =>
            run(
              () => client.certificate.delete(deleteTarget.id, { idempotencyKey: newIdempotencyKey() }),
              () => setDeleteTarget(undefined),
            )
          }
          t={t}
          title={t("resource.domains.delete")}
        />
      ) : null}
    </section>
  );
}

/**
 * Issue form.
 *
 * The identifier picker reads the served-domain inventory and renders it as a
 * checkbox grid rather than a multi-select: the coverage of a certificate is a
 * set an operator builds and re-reads, and a native multi-select shows neither
 * the whole set nor what is not in it. The rows come from the same plane the
 * Domains page reads, so a certificate cannot name a hostname this edge does not
 * answer for.
 */
function IssueCertificateDialog({
  close,
  done,
  onError,
  t,
}: {
  close(): void;
  done(): void;
  onError(message: string | undefined): void;
  t: Translator;
}) {
  const client = useWebserverAdminSdk();
  const [identifiers, setIdentifiers] = useState<ApplicationDomainResponse[] | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [certType, setCertType] = useState<CertificateType>(1);
  const [autoRenew, setAutoRenew] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    void client.domain
      .list({ page: 1, pageSize: 200 })
      .then((result) => {
        if (active) setIdentifiers(result.items);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause));
      });
    return () => {
      active = false;
    };
  }, [client]);

  const toggle = (id: string) =>
    setSelected((current) => (current.includes(id) ? current.filter((value) => value !== id) : [...current, id]));

  const submit = () => {
    if (selected.length === 0) return;
    setBusy(true);
    setError(undefined);
    void client.certificate
      .issue({ domainIds: selected, certType, autoRenew }, { idempotencyKey: newIdempotencyKey() })
      .then(() => {
        close();
        done();
      })
      .catch((cause) => {
        const message = errorText(cause);
        setError(message);
        onError(message);
      })
      .finally(() => setBusy(false));
  };

  return (
    <div className="dialog-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) close(); }} role="presentation">
      <div
        aria-labelledby="served-certificate-issue-title"
        aria-modal="true"
        className="dialog delivery-dialog delivery-dialog-wide"
        role="dialog"
      >
        <header>
          <h2 id="served-certificate-issue-title">{t("resource.certificates.issue")}</h2>
        </header>
        <div className="form-grid single-column">
          <fieldset className="form-fieldset">
            <legend>{t("resource.certificates.selectIdentifiers")}</legend>
            {identifiers === null ? (
              <p className="form-hint">{t("resource.certificates.loading")}</p>
            ) : identifiers.length === 0 ? (
              <p className="form-hint">{t("resource.domains.noSubdomains")}</p>
            ) : (
              <div className="hostname-selector-list">
                {identifiers.map((domain) => (
                  <label key={domain.id}>
                    <input
                      checked={selected.includes(domain.id)}
                      disabled={busy}
                      onChange={() => toggle(domain.id)}
                      type="checkbox"
                    />
                    <span>
                      <strong>{domain.hostname}</strong>
                      {domain.applicationName ? <small>{domain.applicationName}</small> : null}
                    </span>
                  </label>
                ))}
              </div>
            )}
          </fieldset>
          <fieldset className="form-fieldset">
            <legend>{t("resource.certificates.certName")}</legend>
            <div className="hostname-summary">
              <label className="checkbox-field">
                <input
                  checked={certType === 1}
                  disabled={busy}
                  name="certType"
                  onChange={() => setCertType(1)}
                  type="radio"
                />
                {t("resource.certificates.letsEncrypt")}
              </label>
              <label className="checkbox-field">
                <input
                  checked={certType === 3}
                  disabled={busy}
                  name="certType"
                  onChange={() => setCertType(3)}
                  type="radio"
                />
                {t("resource.certificates.selfSigned")}
              </label>
              <label className="checkbox-field">
                <input checked={autoRenew} disabled={busy} onChange={() => setAutoRenew((value) => !value)} type="checkbox" />
                {t("resource.certificates.autoRenew")}
              </label>
            </div>
          </fieldset>
        </div>
        {error ? (
          <div className="error-banner" role="alert">
            {error}
          </div>
        ) : null}
        <footer className="dialog-footer">
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.certificates.cancel")}
          </button>
          <button className="command-button" disabled={busy || selected.length === 0} onClick={submit} type="button">
            {t("resource.certificates.issueSubmit")}
          </button>
        </footer>
      </div>
    </div>
  );
}

/**
 * Revocation.
 *
 * The reason is part of the request, not a page-level setting: RFC 5280 pins it
 * to the act of revoking, and a toolbar control would let the choice that
 * applies to *this* revocation be changed by the next operator without touching
 * the dialog it belongs to.
 */
function RevokeDialog({
  certificate,
  close,
  done,
  onError,
  t,
}: {
  certificate: CertificateResponse;
  close(): void;
  done(): void;
  onError(message: string | undefined): void;
  t: Translator;
}) {
  const client = useWebserverAdminSdk();
  const [reason, setReason] = useState<RevokeReason>("superseded");
  const [busy, setBusy] = useState(false);

  return (
    <ConfirmDialog
      close={close}
      confirmLabel={t("resource.certificates.revoke")}
      dangerous
      extra={
        <label>
          {t("resource.certificates.revokeReason")}
          <select
            aria-label={t("resource.certificates.revokeReason")}
            disabled={busy}
            onChange={(event) => setReason(event.target.value as RevokeReason)}
            value={reason}
          >
            {REVOKE_REASONS.map((value) => (
              <option key={value} value={value}>
                {t(`resource.certificates.reason.${value}` as Parameters<typeof translator>[0] extends never ? never : never)}
              </option>
            ))}
          </select>
        </label>
      }
      message={t("resource.certificates.revokeConfirm", { name: certificate.certName })}
      onConfirm={() => {
        setBusy(true);
        void client.certificate
          .revoke(certificate.id, { reason }, { idempotencyKey: newIdempotencyKey() })
          .then(() => {
            close();
            done();
          })
          .catch((cause) => onError(errorText(cause)))
          .finally(() => setBusy(false));
      }}
      t={t}
      title={t("resource.certificates.revoke")}
    />
  );
}
