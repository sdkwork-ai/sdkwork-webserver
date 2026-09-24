import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type { ApplicationDomainResponse, CertificateResponse } from "@sdkwork/webserver-pc-admin-core";
import type { WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { Ban, CalendarClock, FileKey2, Plus, RefreshCw, RotateCw, Search, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";

import {
  ConfirmDialog,
  Pagination,
  SideDrawer,
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
type CertificateKeyAlgorithm = "RSA" | "ECDSA";

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
  // The entry point from the Domains ledger, read the way the console's
  // Certificates page reads its own: a root domain plus the apex as a label. The
  // form then opens on the root domain alone and the operator picks hostnames
  // inside it — a request for one preselected name is a different gesture and has
  // its own entry point on the hostname ledger.
  const [searchParams, setSearchParams] = useSearchParams();
  const initialRootDomainId = searchParams.get("rootDomainId") ?? undefined;
  const initialApex = searchParams.get("apex") ?? undefined;
  const [certificates, setCertificates] = useState<CertificateResponse[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [build, setBuild] = useState(0);
  const [issueOpen, setIssueOpen] = useState(Boolean(initialRootDomainId));
  const [renewTarget, setRenewTarget] = useState<CertificateResponse>();
  const [revokeTarget, setRevokeTarget] = useState<CertificateResponse>();
  const [deleteTarget, setDeleteTarget] = useState<CertificateResponse>();

  // A second navigation from the Domains ledger updates the query string without
  // remounting this page, so the form has to reopen on the new parameters rather
  // than only on the first render.
  useEffect(() => {
    if (initialRootDomainId) setIssueOpen(true);
  }, [initialRootDomainId]);

  const closeIssue = () => {
    setIssueOpen(false);
    // Dropping the parameters keeps the URL honest: the form is closed, so
    // nothing is preselected any more, and a reload does not reopen it.
    setSearchParams({}, { replace: true });
  };

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
          apex={initialApex}
          close={closeIssue}
          done={reload}
          onError={setError}
          rootDomainId={initialRootDomainId}
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
 * Issue form, in the drawer chrome.
 *
 * The identifier picker reads the served-domain inventory and renders it as a
 * selectable grid rather than a multi-select: the coverage of a certificate is
 * a set an operator builds and re-reads, and a native multi-select shows
 * neither the whole set nor what is not in it. The rows come from the same
 * plane the Domains page reads, so a certificate cannot name a hostname this
 * edge does not answer for.
 *
 * The drawer, not a centred dialog, is the frame for that building work: the
 * picker is tall (a filter, the grid, the running selection), the ledger it is
 * answered against stays legible beside the panel instead of disappearing
 * under it, and the submit row is pinned to the panel's bottom band instead of
 * scrolling away with the form. It is the console's request form restated on
 * this plane with the same class vocabulary, so the two read as one product.
 *
 * `rootDomainId` scopes the picker to one root domain, which is what the Domains
 * ledger's "request certificate" action opens. The console's request form takes
 * the same parameter from the same gesture (`zoneId` there); `apex` is carried
 * only as a label, because the scoped read is the thing that actually decides
 * what is offered.
 */
function IssueCertificateDialog({
  apex,
  close,
  done,
  onError,
  rootDomainId,
  t,
}: {
  apex?: string | undefined;
  close(): void;
  done(): void;
  onError(message: string | undefined): void;
  rootDomainId?: string | undefined;
  t: Translator;
}) {
  const client = useWebserverAdminSdk();
  const [identifiers, setIdentifiers] = useState<ApplicationDomainResponse[] | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [certType, setCertType] = useState<CertificateType>(1);
  // RSA is the platform default (the shared vocabulary in sdkwork-deploy-core):
  // a managed certificate is renewed unattended, and RSA-2048 is the leaf key
  // every TLS client accepts. The control is rendered anyway, because a default
  // the operator cannot see is a decision they cannot overturn — and ECDSA is
  // one click away for a name whose clients are all known to support it.
  const [keyAlgorithm, setKeyAlgorithm] = useState<CertificateKeyAlgorithm>("RSA");
  const [autoRenew, setAutoRenew] = useState(true);
  const [filter, setFilter] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    // One scoped read and one whole-inventory read are the same call shape, so
    // the only thing the scope changes is which plane answers.
    const read = rootDomainId
      ? client.domain.rootDomains.subdomains.list(rootDomainId, { page: 1, pageSize: 200 })
      : client.domain.list({ page: 1, pageSize: 200 });
    void read
      .then((result) => {
        if (active) setIdentifiers(result.items);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause));
      });
    return () => {
      active = false;
    };
  }, [client, rootDomainId]);

  const toggle = (id: string) =>
    setSelected((current) => (current.includes(id) ? current.filter((value) => value !== id) : [...current, id]));

  // Alphabetical, because the picker is read as a list to scan: the inventory's
  // own order is the reconcile's write order, which says nothing to an operator
  // choosing coverage. Sorting a copy keeps the read's result untouched.
  const inventory = [...(identifiers ?? [])].sort((a, b) => a.hostname.localeCompare(b.hostname));
  const needle = filter.trim().toLowerCase();
  const visible = needle
    ? inventory.filter((domain) => domain.hostname.toLowerCase().includes(needle))
    : inventory;
  const selectedHostnames = inventory.filter((domain) => selected.includes(domain.id));

  const submit = () => {
    if (selected.length === 0) return;
    setBusy(true);
    setError(undefined);
    void client.certificate
      .issue({ domainIds: selected, certType, keyAlgorithm, autoRenew }, { idempotencyKey: newIdempotencyKey() })
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

  // The refusal and the row are the footer band's two children, spaced by the
  // band's own gap — see `.delivery-drawer-footer` in `deploy-surface.css`.
  const footer = (
    <>
      {error ? (
        <div className="error-banner" role="alert">
          {error}
        </div>
      ) : null}
      <div className="dialog-footer">
        <button className="secondary-button" onClick={close} type="button">
          {t("resource.certificates.cancel")}
        </button>
        <button className="command-button" disabled={busy || selected.length === 0} onClick={submit} type="button">
          {t("resource.certificates.issueSubmit")}
        </button>
      </div>
    </>
  );

  return (
    <SideDrawer
      close={close}
      closeLabel={t("resource.certificates.close")}
      footer={footer}
      title={t("resource.certificates.issue")}
    >
      {/* A form, so the drawer's density pass (the group rhythm keyed on
          `form>.form-fieldset`) applies and Enter in a control submits. */}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <fieldset className="form-fieldset">
          <legend>{t("resource.certificates.selectIdentifiers")}</legend>
          {/* The root domain the picker is scoped to, and only when the caller
              named one: the toolbar opens this form with no scope at all, and
              an empty label line there would read as a missing value. */}
          {apex ? <p className="form-hint">{apex}</p> : null}
          {identifiers === null ? (
            <p className="form-hint">{t("resource.certificates.loading")}</p>
          ) : inventory.length === 0 ? (
            <p className="form-hint">{t("resource.domains.noSubdomains")}</p>
          ) : (
            <>
              {/* Filtering earns its box only once the list is long enough to
                  need it: under a screenful the search box pushes the rows it
                  filters out of view, which is the opposite of what it is for.
                  Enter inside it must not submit the form around it. */}
              {inventory.length > 8 ? (
                <div className="search-box selector-search">
                  <Search size={16} />
                  <input
                    aria-label={t("resource.certificates.searchHostnames")}
                    disabled={busy}
                    onChange={(event) => setFilter(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") event.preventDefault();
                    }}
                    placeholder={t("resource.certificates.searchHostnames")}
                    value={filter}
                  />
                </div>
              ) : null}
              <div className="hostname-selector-list">
                {visible.map((domain) => (
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
                {visible.length === 0 ? (
                  <p className="selector-empty">{t("resource.certificates.noHostnameMatch")}</p>
                ) : null}
              </div>
              {/* The running selection, as chips: the grid shows what is
                  offered, this shows what was actually built — including the
                  ones a filter is currently hiding. */}
              {selectedHostnames.length > 0 ? (
                <div className="selected-hostnames">
                  <span>{t("resource.certificates.selectedCount", { count: selectedHostnames.length })}</span>
                  <div>
                    {selectedHostnames.map((domain) => (
                      <span key={domain.id}>
                        {domain.hostname}
                        <button
                          aria-label={t("resource.certificates.removeHostname", { hostname: domain.hostname })}
                          disabled={busy}
                          onClick={() => toggle(domain.id)}
                          title={t("resource.certificates.removeHostname", { hostname: domain.hostname })}
                          type="button"
                        >
                          <X size={12} />
                        </button>
                      </span>
                    ))}
                  </div>
                </div>
              ) : null}
            </>
          )}
        </fieldset>
        {/* This group used to borrow the certificate-name column heading as its
            legend, which read as "type the name here"; it is the issuance
            options group and is labelled as one now. */}
        <fieldset className="form-fieldset">
          <legend>{t("resource.certificates.issueOptions")}</legend>
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
          <fieldset className="form-fieldset">
            <legend>{t("resource.certificates.keyAlgorithmTitle")}</legend>
            <div
              aria-label={t("resource.certificates.keyAlgorithmTitle")}
              className="segmented-control algorithm-control"
              role="group"
            >
              {(["RSA", "ECDSA"] as const).map((value) => (
                <button
                  aria-pressed={keyAlgorithm === value}
                  disabled={busy}
                  key={value}
                  onClick={() => setKeyAlgorithm(value)}
                  type="button"
                >
                  {value}
                </button>
              ))}
            </div>
            <p className="form-hint">
              {keyAlgorithm === "RSA"
                ? t("resource.certificates.keyAlgorithmRsaHint")
                : t("resource.certificates.keyAlgorithmEcdsaHint")}
            </p>
          </fieldset>
        </fieldset>
      </form>
    </SideDrawer>
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
