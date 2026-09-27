import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type {
  ApplicationDomainResponse,
  CertificateResponse,
  DnsAccountResponse,
  RootDomainResponse,
} from "@sdkwork/webserver-pc-admin-core";
import { pollWebserverOperation, type WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { BadgeCheck, Ban, CalendarClock, FileKey2, Globe2, Plus, RefreshCw, RotateCw, Search, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";

import {
  ConfirmDialog,
  DialogBackdrop,
  DialogCloseButton,
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

/// Inventory reads walk the paged SDK surface instead of trusting one big
/// page: the pager caps a page at 200, so a read that stops at the first page
/// silently hides everything past it (the 201st hostname could not be covered
/// by any certificate this form issues). The walk is bounded — ten pages is
/// 2,000 rows, far past anything an edge legitimately configures — so a
/// misbehaving server cannot balloon one picker read into unbounded work.
const INVENTORY_PAGE_SIZE = 200;
const INVENTORY_MAX_PAGES = 10;

async function readAllPages<Row>(
  read: (page: { page: number; pageSize: number }) => Promise<{
    items: Row[];
    pageInfo: { hasMore?: boolean };
  }>,
): Promise<Row[]> {
  const rows: Row[] = [];
  for (let page = 1; page <= INVENTORY_MAX_PAGES; page += 1) {
    const result = await read({ page, pageSize: INVENTORY_PAGE_SIZE });
    rows.push(...result.items);
    if (result.pageInfo.hasMore !== true) return rows;
  }
  return rows;
}

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
  const [nextCursor, setNextCursor] = useState<string>();
  // Cursor paging over the certificate collection: `cursor` is the keyset
  // token of the page on screen (`undefined` = the offset page one, the only
  // page that carries the exact total), and the stack remembers each visited
  // page's own token so Previous can walk back through them.
  const [cursor, setCursor] = useState<string>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
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
      .list({ pageSize: PAGE_SIZE, cursor })
      .then((result) => {
        if (!active) return;
        setCertificates(result.items);
        // `PageInfo.hasMore` is optional on the wire; an absent flag means "no
        // continuation", never "unknown", so it folds to `false` rather than
        // leaking `undefined` into the pager.
        setHasMore(result.pageInfo.hasMore === true);
        setNextCursor(result.pageInfo.nextCursor ?? undefined);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause, t));
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [build, client, cursor, page, t]);

  const reload = () => {
    setCertificates(null);
    // A mutation changes the collection a stale cursor points into, so every
    // reload restarts from the offset page one.
    setCursor(undefined);
    setCursorStack([]);
    setPage(1);
    setBuild((value) => value + 1);
  };

  const gotoNextPage = () => {
    if (!nextCursor || !hasMore) return;
    setCursorStack((stack) => [...stack, cursor ?? ""]);
    setCursor(nextCursor);
    setPage((value) => value + 1);
  };

  const gotoPreviousPage = () => {
    const previous = cursorStack[cursorStack.length - 1];
    setCursorStack((stack) => stack.slice(0, -1));
    setCursor(previous ? previous : undefined);
    setPage((value) => Math.max(1, value - 1));
  };

  // Issue and renew answer 202 with a durable operation, not a finished row.
  // The ledger reloads immediately so the operator sees the operation in
  // flight, and the poller reloads again on the terminal state so success or
  // failure shows up without a manual refresh.
  const trackOperation = (accepted: { operationId?: string } | undefined) => {
    const operationId = accepted?.operationId;
    if (!operationId) return;
    void pollWebserverOperation(operationId, (id, options) => client.certificate.operations.retrieve(id, options))
      .catch(() => undefined)
      .then(() => reload());
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
        setError(errorText(cause, t));
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
            onNext={gotoNextPage}
            onPrevious={gotoPreviousPage}
            page={page}
            t={t}
          />
        </>
      )}

      {issueOpen ? (
        <IssueCertificateDialog
          trackOperation={trackOperation}
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
            run(
              () =>
                client.certificate
                  .renew(renewTarget.id, { idempotencyKey: newIdempotencyKey() })
                  .then((accepted) => trackOperation(accepted)),
              () =>
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

type CertificateScope = "SINGLE_DOMAIN" | "WILDCARD";
type CertificateValidationMethod = "AUTO" | "HTTP_01" | "DNS_01";
type CertificateCaProfile = "LETS_ENCRYPT_PRODUCTION" | "LETS_ENCRYPT_STAGING";

/**
 * The renewal lead time the authored request schema defaults to and bounds.
 *
 * Kept as values here rather than read from the generated SDK, which carries
 * the bounds in the field's prose rather than as numbers. They are the same 7
 * and 90 the server validates and `webserver_certificate` is checked against,
 * and the input states them, so a value outside them is visible before the
 * submit rather than as a refused request.
 */
const MINIMUM_RENEW_BEFORE_DAYS = 7;
const MAXIMUM_RENEW_BEFORE_DAYS = 90;
const DEFAULT_RENEW_BEFORE_DAYS = 30;

/**
 * Whether a hostname is a wildcard, from the shape of the name.
 *
 * The served-domain payload does not carry the stored `hostname_type`, so the
 * form derives it here. That is the same predicate the ACME engine applies when
 * it resolves a challenge (`contains_wildcard_identifier`), and the stored
 * column is written from the same shape, so the two cannot disagree about a
 * name the form offers.
 */
function isWildcardHostname(hostname: string): boolean {
  return hostname.trim().toLowerCase().startsWith("*.");
}

/**
 * Which configured DNS account presents this certificate's challenges.
 *
 * The console's field, shape for shape: a summary row that *is* the answer — a
 * mark, the account's name, and the fact that makes it recognisable — plus the
 * controls that change it, with the choosing done in a dialog rather than in the
 * row. A `<select>` was the first attempt here and was wrong twice over: the
 * scoped stylesheet boxes a `select` only inside `.form-grid`, so this one
 * rendered as an 18px borderless line, and the console's own reading of the
 * field — "what will answer, and how do I change it" — is a summary, not a
 * dropdown. `.cloud-account-selection` and its neighbours were already in
 * `deploy-surface.css` with nothing using them; this is the field they were
 * copied over for.
 *
 * The label carries the zone and the provider as well as the id, because the ids
 * an operator may name are exactly the ones this edge holds credentials for and
 * choosing between two accounts for one provider is when the id alone stops being
 * enough.
 *
 * Rendered even when the edge has no accounts, with the reason in its caption and
 * the picker disabled: a hidden field would leave "why can I not issue a wildcard
 * here" unanswerable from the form.
 */
function CloudAccountField({
  accounts,
  busy,
  onChange,
  onPick,
  t,
  value,
}: {
  /**
   * `null` is "the read has not answered, or failed"; `[]` is "this edge has
   * none". The two are kept apart because the field says different things about
   * them, and a failed read reported as an empty configuration tells an operator
   * to go configure accounts that are already configured.
   */
  accounts: readonly DnsAccountResponse[] | null;
  busy: boolean;
  onChange(next: string | undefined): void;
  onPick(): void;
  t: Translator;
  value: string | undefined;
}) {
  const chosen = accounts?.find((account) => account.accountId === value) ?? undefined;
  return (
    <fieldset className="form-fieldset form-field-wide">
      <legend title={value === undefined ? t("resource.certificates.cloudAccountAutoHint") : undefined}>
        {t("resource.certificates.cloudAccount")}
      </legend>
      <div className="cloud-account-selection" data-state={value === undefined ? "auto" : "pinned"}>
        <span aria-hidden="true" className="cloud-account-mark">
          {value === undefined ? <RotateCw size={15} /> : <BadgeCheck size={15} />}
        </span>
        <div className="cloud-account-summary">
          <strong>{chosen === undefined ? t("resource.certificates.cloudAccountAuto") : chosen.accountId}</strong>
          <small className="form-hint">
            {chosen !== undefined
              ? `${chosen.zoneApex} · ${chosen.provider}`
              : accounts === null
                ? t("resource.certificates.cloudAccountReadFailed")
                : accounts.length === 0
                  ? t("resource.certificates.cloudAccountUnavailable")
                  : t("resource.certificates.cloudAccountHint")}
          </small>
        </div>
        {/* Offered only when something is pinned: "use automatic" on an
            already-automatic field is a control that does nothing. */}
        {value === undefined ? null : (
          <button className="cloud-account-clear" onClick={() => onChange(undefined)} type="button">
            {t("resource.certificates.cloudAccountClear")}
          </button>
        )}
      </div>
      <div className="cloud-account-actions">
        <button
          className="secondary-button"
          disabled={busy || accounts === null || accounts.length === 0}
          onClick={onPick}
          type="button"
        >
          {t("resource.certificates.cloudAccountPick")}
        </button>
      </div>
    </fieldset>
  );
}

/**
 * The account chooser, as the console has it: one column read down, the automatic
 * answer first because it is what most certificates will use.
 *
 * The panel frame is `.delivery-dialog-accounts`, which is the wide, fixed-height
 * reading surface `deploy-surface.css` already carries for this dialog — its rule
 * set was copied over with nothing using it either. The rows are
 * `.hostname-selector-list`, one column read down, which is what a list of
 * accounts is — and the same primitive the console's own account chooser uses,
 * so the two consoles answer "which account?" the same way.
 *
 * The hostname picker deliberately does *not* use that primitive: a hostname's
 * row carries five facts beside its name, so it is a table there and a list
 * here.
 *
 * No family filter and no register button: the accounts this edge can present
 * through are its own configuration file, so there is nothing to filter by and
 * nothing to create here. Both are console features whose data this plane does
 * not have, and the picker says what it holds instead of showing controls that
 * could only ever be empty.
 */
function CloudAccountPickerDialog({
  accounts,
  apply,
  close,
  t,
  value,
}: {
  accounts: readonly DnsAccountResponse[];
  apply(next: string | undefined): void;
  close(): void;
  t: Translator;
  value: string | undefined;
}) {
  const [draft, setDraft] = useState<string | undefined>(value);
  return (
    <DialogBackdrop close={close}>
      <div
        aria-labelledby="admin-cloud-account-picker-title"
        aria-modal="true"
        className="dialog delivery-dialog delivery-dialog-accounts"
        role="dialog"
      >
        <header>
          <h2 id="admin-cloud-account-picker-title">{t("resource.certificates.cloudAccountPickerTitle")}</h2>
          <DialogCloseButton close={close} label={t("resource.certificates.cancel")} />
        </header>
        <fieldset className="form-fieldset">
          <legend>{t("resource.certificates.cloudAccountPickerLegend")}</legend>
          <div className="hostname-selector-list">
            {/* The automatic answer is a row like any other, not a separate
                button: it is one of the values this field can hold, and a
                "clear" control beside a list of choices would make it read as
                the absence of a choice. */}
            <label>
              <input
                checked={draft === undefined}
                name="adminCloudAccount"
                onChange={() => setDraft(undefined)}
                type="radio"
              />
              <span>
                <strong>{t("resource.certificates.cloudAccountAuto")}</strong>
                <small>{t("resource.certificates.cloudAccountAutoHint")}</small>
              </span>
            </label>
            {accounts.map((account) => (
              <label key={account.accountId}>
                <input
                  checked={draft === account.accountId}
                  name="adminCloudAccount"
                  onChange={() => setDraft(account.accountId)}
                  type="radio"
                />
                <span>
                  <strong>{account.accountId}</strong>
                  <small>
                    {account.zoneApex} · {account.provider}
                  </small>
                </span>
              </label>
            ))}
          </div>
        </fieldset>
        <footer className="dialog-footer">
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.certificates.cancel")}
          </button>
          <button className="command-button" onClick={() => apply(draft)} type="button">
            {t("resource.certificates.confirm")}
          </button>
        </footer>
      </div>
    </DialogBackdrop>
  );
}

/**
 * Coverage picker: one root domain on the left, its declared hostnames on the
 * right.
 *
 * One name, not a set. A request is filed under one hostname — the identifier
 * the certificate ledger lists it by — so the pane is a radio group rather than
 * a set of ticks: choosing a second name replaces the first, and no state here
 * holds two. The multi-tick version this replaces let "cover everything I
 * happened to tick" reach the server as a request nobody had decided to make.
 *
 * Switching, not mixing. A certificate covers one root domain's worth of names,
 * and a name folded against one apex is the wrong name against another, so
 * choosing a different root domain re-points the draft at that domain's rows
 * instead of accumulating both. This is the console picker's rule, kept for the
 * same reason.
 *
 * The right pane is a row list rather than a grid of cards, and it is a plain
 * `<table>` rather than the framework `DataTable`: the framework's selectable
 * mode is a set of checkboxes under a select-all header, and neither belongs in
 * a field that holds one value. Column for column it reads the way the console's
 * picker reads down — the record name first, because that is the half of the name
 * that goes into DNS, then the full name, then whether control of it is proven,
 * then the service that answers for it, then how many certificates already claim
 * it. `application` is this plane's own column: the Deployments picker counts
 * bindings instead, because its hostnames hang off a binding record where this
 * plane's hang off a service. The markup is the one `deploy-surface.css` already
 * dresses (`hostname-picker-frame` / `hostname-picker-table` / `selection-column`
 * / `hostname-choice`, down to the `[data-state="selected"]` marker).
 *
 * Rows the chosen scope refuses are shown but not selectable rather than hidden:
 * the operator asked which hostnames this root domain has, and an answer that
 * omitted the ones the current scope cannot take would read as a root domain
 * missing them.
 */
function HostnamePickerDialog({
  apply,
  close,
  initialRootDomainId,
  scope,
  selection,
  t,
}: {
  apply(next: { rootDomainId: string; selection: ApplicationDomainResponse | undefined }): void;
  close(): void;
  initialRootDomainId: string;
  scope: CertificateScope;
  selection: ApplicationDomainResponse | undefined;
  t: Translator;
}) {
  const client = useWebserverAdminSdk();
  const [rootDomains, setRootDomains] = useState<RootDomainResponse[] | null>(null);
  const [activeRootDomainId, setActiveRootDomainId] = useState(initialRootDomainId);
  const [rows, setRows] = useState<ApplicationDomainResponse[] | null>(null);
  const [draft, setDraft] = useState<ApplicationDomainResponse | undefined>(selection);
  const [filter, setFilter] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  // Active root domains only, the same invariant the form's own list holds: a
  // disabled root domain's hostnames are not served, so they are not offered.
  useEffect(() => {
    let active = true;
    void readAllPages((page) => client.domain.rootDomains.list({ ...page, status: 1 }))
      .then((items) => {
        if (!active) return;
        setRootDomains(items);
        // Opens on the caller's root domain when it named one, and on the first
        // one otherwise — never on nothing, because an empty picker with a full
        // list beside it reads as a failed read.
        setActiveRootDomainId((current) => (current === "" ? (items[0]?.id ?? "") : current));
      })
      .catch((cause) => {
        if (active) setError(errorText(cause, t));
      });
    return () => {
      active = false;
    };
  }, [client]);

  useEffect(() => {
    if (activeRootDomainId === "") return;
    let active = true;
    setBusy(true);
    setError(undefined);
    setRows(null);
    void readAllPages((page) => client.domain.rootDomains.subdomains.list(activeRootDomainId, page))
      .then((items) => {
        if (active) setRows(items);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause, t));
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [activeRootDomainId, client]);

  const switchRootDomain = (rootDomainId: string) => {
    setActiveRootDomainId(rootDomainId);
    // Kept only where it still belongs: a row of another root domain would be
    // submitted under the wrong apex.
    setDraft((current) => (current !== undefined && current.rootDomainId === rootDomainId ? current : undefined));
  };

  const rowBlocked = (row: ApplicationDomainResponse) =>
    scope === "SINGLE_DOMAIN" && isWildcardHostname(row.hostname);

  const needle = filter.trim().toLowerCase();
  const candidateRows = rows ?? [];
  const listed =
    needle === "" ? candidateRows : candidateRows.filter((row) => row.hostname.toLowerCase().includes(needle));

  return (
    <DialogBackdrop close={close}>
      <div
        aria-labelledby="admin-hostname-picker-title"
        aria-modal="true"
        className="dialog delivery-dialog delivery-dialog-picker hostname-picker-dialog"
        role="dialog"
      >
        <header>
          <h2 id="admin-hostname-picker-title">{t("resource.certificates.hostnamePickerTitle")}</h2>
          <DialogCloseButton close={close} label={t("resource.certificates.cancel")} />
        </header>
        {error ? (
          <div className="error-banner" role="alert">
            {error}
          </div>
        ) : null}
        <div className="hostname-picker">
          <div className="hostname-picker-zones">
            <span className="hostname-picker-caption">{t("resource.certificates.rootDomainSelect")}</span>
            {(rootDomains ?? []).map((rootDomain) => (
              <button
                aria-pressed={rootDomain.id === activeRootDomainId}
                className="hostname-picker-zone"
                key={rootDomain.id}
                onClick={() => switchRootDomain(rootDomain.id)}
                type="button"
              >
                <strong>{rootDomain.hostname}</strong>
                {/* Only the root domain being browsed can carry the marker: the
                    draft is one root domain's worth by construction, so a mark on
                    the others would be a claim about a choice that is not there.
                    It says *that* a name is chosen rather than which one, because
                    the rail is 140px wide and the name already has a row of its
                    own on the other side. */}
                {rootDomain.id === activeRootDomainId && draft !== undefined ? (
                  <small>{t("resource.certificates.pickerChosen")}</small>
                ) : null}
              </button>
            ))}
            {rootDomains !== null && rootDomains.length === 0 ? (
              <div className="selector-empty">{t("resource.certificates.noRootDomains")}</div>
            ) : null}
          </div>
          <div className="hostname-picker-hostnames">
            <div className="hostname-picker-head">
              <span className="hostname-picker-caption">{t("resource.certificates.hostnameCandidates")}</span>
            </div>
            {/* Filtering earns its box only once the list is long enough to need
                it: under a screenful the search box pushes the rows it filters
                out of view, which is the opposite of what it is for. */}
            {candidateRows.length > 8 ? (
              <div className="search-box selector-search">
                <Search size={16} />
                <input
                  aria-label={t("resource.certificates.searchHostnames")}
                  onChange={(event) => setFilter(event.target.value)}
                  placeholder={t("resource.certificates.searchHostnames")}
                  value={filter}
                />
              </div>
            ) : null}
            {/* A table, not a grid of cards. The same facts sit under every name —
                which record it is, what the name is, whether control of it is
                proven, what serves it, and how many certificates already claim it —
                and columns are what let an operator read down the pane and compare,
                where a card per row only lets them re-read one row at a time. Two
                cards per row also halved every name's width, so the long ones
                wrapped while the pane around them sat half empty.

                The frame is the scrollport `deploy-surface.css` names for this
                dialog (`.hostname-picker-frame` under `.hostname-picker-hostnames`),
                so the caption and the search box stay put and only the rows scroll.
                The table's sticky header needs `overflow:auto` on an ancestor, and
                that frame is the one that carries it. */}
            <div aria-busy={busy} className="table-frame hostname-picker-frame">
              <table className="domain-table hostname-picker-table">
                <thead>
                  <tr>
                    {/* The choice column has no name of its own: each control is
                        named by the hostname in its own row, so a header could
                        only repeat it. */}
                    <th className="selection-column" />
                    <th>{t("resource.domains.recordName")}</th>
                    <th>{t("resource.domains.subdomainHostname")}</th>
                    <th>{t("resource.domains.verification")}</th>
                    <th>{t("resource.domains.application")}</th>
                    <th>{t("resource.domains.certificateCount")}</th>
                  </tr>
                </thead>
                <tbody>
                  {listed.length === 0 ? (
                    // The empty answer is a row of the table rather than a block
                    // beside it, so it lands under the headers it is answering
                    // about and scrolls with them.
                    <tr>
                      <td className="hostname-picker-empty" colSpan={6}>
                        {busy
                          ? t("resource.certificates.hostnamesLoading")
                          : needle === ""
                            ? t("resource.certificates.noHostnamesInRootDomain")
                            : t("resource.certificates.noHostnameMatch")}
                      </td>
                    </tr>
                  ) : (
                    listed.map((row) => {
                      // Rows the current certificate type refuses are disabled
                      // rather than hidden: the operator asked which hostnames this
                      // root domain has, and an answer that silently omitted the
                      // ones the scope cannot take would read as a root domain that
                      // is missing them.
                      const blocked = rowBlocked(row);
                      const chosen = draft !== undefined && draft.id === row.id;
                      return (
                        // The row is the click target as well as its control: a
                        // hostname is one line of a table read down, and aiming at
                        // a 16px circle is how the wrong name ends up chosen. Both
                        // set the draft to this row, so a click that lands on both
                        // is still one answer.
                        <tr
                          aria-disabled={blocked ? true : undefined}
                          data-state={chosen ? "selected" : undefined}
                          key={row.id}
                          onClick={() => {
                            if (!blocked) setDraft(row);
                          }}
                        >
                          <td className="selection-column">
                            <span className="hostname-choice">
                              {/* Named by the hostname rather than by the word
                                  "select": the row is already the context, and a
                                  screen reader reading down the pane would
                                  otherwise hear the same three words per row. */}
                              <input
                                aria-label={t("resource.certificates.pickerChooseHostname", {
                                  hostname: row.hostname,
                                })}
                                checked={chosen}
                                disabled={blocked}
                                name="adminCertificateHostname"
                                onChange={() => setDraft(row)}
                                type="radio"
                              />
                            </span>
                          </td>
                          <td>{row.recordName || "-"}</td>
                          <td>
                            <span className="hostname-cell">
                              <Globe2 size={16} />
                              <span>
                                <strong>{row.hostname}</strong>
                                <small className="cell-subtitle">
                                  {isWildcardHostname(row.hostname)
                                    ? t("resource.certificates.wildcard")
                                    : t("resource.certificates.exact")}
                                </small>
                              </span>
                            </span>
                          </td>
                          <td>
                            <StatusBadge t={t} value={row.isVerified ? "VERIFIED" : "PENDING"} />
                          </td>
                          <td>{row.applicationName || "-"}</td>
                          <td>{row.certificateCount}</td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </div>
        <footer className="dialog-footer">
          {/* What Confirm will carry back, on the side the reading starts — see
              `.hostname-picker-count`'s `margin-right:auto`. */}
          <span className="hostname-picker-count">
            {draft === undefined
              ? t("resource.certificates.pickerNoChoice")
              : t("resource.certificates.pickerChosenName", { hostname: draft.hostname })}
          </span>
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.certificates.cancel")}
          </button>
          {/* Confirming an empty set is allowed and means what it says: the
              picker is how the set is changed, and clearing it here is a change
              the operator may make rather than removing the chips one by one. */}
          <button
            className="command-button"
            disabled={activeRootDomainId === ""}
            onClick={() => apply({ rootDomainId: activeRootDomainId, selection: draft })}
            type="button"
          >
            {t("resource.certificates.confirm")}
          </button>
        </footer>
      </div>
    </DialogBackdrop>
  );
}

/**
 * Issue form, in the drawer chrome.
 *
 * Field for field the console's `CertificateFormDialog`: the certificate type
 * decides what may be covered, the hostname picker decides what is, and the
 * validation method, cloud account, CA profile, name and renewal lead time sit
 * where the console puts them. The two forms are read side by side, so a field
 * present on one plane and missing on the other reads as a missing feature
 * rather than as a different opinion.
 *
 * Three of them are answered differently here, and the form says so instead of
 * pretending otherwise:
 *
 * * **Cloud accounts** come from the edge's own DNS accounts file, which is
 *   deployment configuration, not from a control-plane account center. The
 *   control is the same select-with-automatic; the source is the registry the
 *   ACME engine will actually present through, served by `/dns_accounts`.
 * * **CA profile** is a single-directory deployment. Naming the directory this
 *   edge is not configured for is refused by the server with the configured one
 *   named — a refusal, not a silent order from somewhere else.
 * * **Certificate type** (Let's Encrypt / self-signed) exists on this plane and
 *   not on the console's. It is kept rather than dropped to match: self-signed
 *   issuance is a capability this plane has, and the ACME-only controls below
 *   are disabled while it is selected instead of being sent and ignored.
 *
 * The hostname picker is a second, centred dialog rather than a grid inside the
 * drawer, because choosing coverage is a browse-then-commit gesture over one
 * root domain's declared hostnames, while the drawer's scroll band is already
 * carrying the form.
 *
 * `rootDomainId` seeds the picker's root domain and auto-selects `apex`, which
 * is what the Domains ledger's "request certificate" action carries through.
 * The seed is one-shot: an operator who then chooses different coverage must not
 * have it undone by the caller's target.
 */
function IssueCertificateDialog({
  apex,
  close,
  done,
  onError,
  rootDomainId,
  t,
  trackOperation,
}: {
  apex?: string | undefined;
  close(): void;
  done(): void;
  onError(message: string | undefined): void;
  rootDomainId?: string | undefined;
  t: Translator;
  trackOperation(accepted: { operationId?: string } | undefined): void;
}) {
  const client = useWebserverAdminSdk();
  const [scope, setScope] = useState<CertificateScope>(
    apex?.startsWith("*.") === true ? "WILDCARD" : "SINGLE_DOMAIN",
  );
  const [rootDomains, setRootDomains] = useState<RootDomainResponse[]>([]);
  const [chosenRootDomainId, setChosenRootDomainId] = useState(rootDomainId ?? "");
  // One hostname, not a list of them. A request names one identifier, so the
  // field holds one value and `undefined` is its empty state — the state the form
  // refuses to submit on — rather than a list that happens to be empty.
  const [selection, setSelection] = useState<ApplicationDomainResponse>();
  const [pickerOpen, setPickerOpen] = useState(false);
  const [certType, setCertType] = useState<CertificateType>(1);
  const [keyAlgorithm, setKeyAlgorithm] = useState<CertificateKeyAlgorithm>("RSA");
  const [autoRenew, setAutoRenew] = useState(true);
  const [validationMethod, setValidationMethod] = useState<CertificateValidationMethod>("AUTO");
  const [caProfile, setCaProfile] = useState<CertificateCaProfile>("LETS_ENCRYPT_PRODUCTION");
  const [certNameDraft, setCertNameDraft] = useState("");
  const [renewBeforeDays, setRenewBeforeDays] = useState(String(DEFAULT_RENEW_BEFORE_DAYS));
  const [providerAccountId, setProviderAccountId] = useState<string>();
  // `null` until the read answers, and still `null` if it fails — see
  // `CloudAccountField` for why the empty case is a different state.
  const [accounts, setAccounts] = useState<DnsAccountResponse[] | null>(null);
  const [accountPickerOpen, setAccountPickerOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const formId = "admin-certificate-issue-form";

  // Active root domains only, because a certificate covers names this edge
  // answers for and a disabled root domain's hostnames are not served.
  useEffect(() => {
    let active = true;
    void readAllPages((page) => client.domain.rootDomains.list({ ...page, status: 1 }))
      .then((items) => {
        if (active) setRootDomains(items);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause, t));
      });
    return () => {
      active = false;
    };
  }, [client]);

  // An empty list is the truthful answer for an edge with no accounts file, not
  // a failure: such an edge issues single-domain certificates over HTTP-01 and
  // simply has none to offer, which the field says in its hint.
  //
  // A *failed* read is the third answer, and it is deliberately not collapsed
  // into the empty one. A gateway that has not been rebuilt since this endpoint
  // was added answers 404, and reporting that as "no accounts configured" would
  // send an operator to a configuration file that is already correct.
  useEffect(() => {
    let active = true;
    void readAllPages((page) => client.certificate.dnsAccounts.list(page))
      .then((items) => {
        if (active) setAccounts(items);
      })
      .catch((cause) => {
        if (!active) return;
        setAccounts(null);
        setError(errorText(cause, t));
      });
    return () => {
      active = false;
    };
  }, [client]);

  // Resolving the caller's target into the row it names, so a link from a
  // hostname row opens the form on that hostname rather than on an empty field
  // the operator has to rebuild.
  const seeded = useRef(false);
  useEffect(() => {
    if (seeded.current || chosenRootDomainId === "") return;
    const target = apex?.trim().toLowerCase() ?? "";
    if (target === "") {
      seeded.current = true;
      return;
    }
    seeded.current = true;
    let active = true;
    void readAllPages((page) => client.domain.rootDomains.subdomains.list(chosenRootDomainId, page))
      .then((items) => {
        if (!active) return;
        const row = items.find((item) => item.hostname.trim().toLowerCase() === target);
        if (row !== undefined) setSelection(row);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause, t));
      });
    return () => {
      active = false;
    };
  }, [apex, chosenRootDomainId, client]);

  const chosenRootDomain = rootDomains.find((rootDomain) => rootDomain.id === chosenRootDomainId);

  // The method a wildcard ends up with is a consequence of the scope, so
  // switching to WILDCARD while HTTP-01 is selected resolves to AUTO rather
  // than submitting a pair the server refuses. The state is left alone so
  // switching back restores the operator's own choice.
  const effectiveValidationMethod: CertificateValidationMethod =
    scope === "WILDCARD" && validationMethod === "HTTP_01" ? "AUTO" : validationMethod;

  const renewDays = Number.parseInt(renewBeforeDays, 10);
  const renewIssue: MessageKey | undefined =
    Number.isInteger(renewDays) &&
    renewDays >= MINIMUM_RENEW_BEFORE_DAYS &&
    renewDays <= MAXIMUM_RENEW_BEFORE_DAYS
      ? undefined
      : "resource.certificates.renewBeforeDaysInvalid";

  // Two of the three states here cannot be reached from inside the picker: it
  // disables the rows the scope refuses, so a wildcard cannot be chosen under
  // `SINGLE_DOMAIN` and an exact name cannot be chosen under `WILDCARD`. They are
  // still reachable by changing the certificate type *after* choosing — nothing in
  // the picker goes back and unsets a choice the new type cannot take — so the
  // form reports the pair that is on screen now rather than trusting the order the
  // two clicks happened in.
  const selectionIssue: MessageKey | undefined =
    selection === undefined
      ? "resource.certificates.hostnameRequired"
      : scope === "SINGLE_DOMAIN" && isWildcardHostname(selection.hostname)
        ? "resource.certificates.singleDomainRejectsWildcard"
        : scope === "WILDCARD" && !isWildcardHostname(selection.hostname)
          ? "resource.certificates.wildcardRequired"
          : undefined;

  const acme = certType === 1;
  const submitDisabled = busy || selectionIssue !== undefined || renewIssue !== undefined;

  const submit = () => {
    // The second half restates the first — `selectionIssue` already refuses an
    // empty selection — and is what lets the id below be read without a `!`.
    if (submitDisabled || selection === undefined) return;
    setBusy(true);
    setError(undefined);
    void client.certificate
      .issue(
        {
          domainIds: [selection.id],
          certType,
          keyAlgorithm,
          autoRenew,
          certificateScope: scope,
          renewBeforeDays: renewDays,
          // The three ACME-only settings travel only for an ACME request. A
          // self-signed one has no CA directory, no challenge and no DNS
          // account, and sending them would store preferences the certificate
          // will never act on.
          ...(acme
            ? {
                validationMethod: effectiveValidationMethod,
                caProfile,
                ...(providerAccountId === undefined ? {} : { providerAccountId }),
              }
            : {}),
          ...(certNameDraft.trim() === "" ? {} : { certName: certNameDraft.trim() }),
        },
        { idempotencyKey: newIdempotencyKey() },
      )
      .then((accepted) => {
        close();
        done();
        trackOperation(accepted);
      })
      .catch((cause) => {
        const message = errorText(cause, t);
        setError(message);
        onError(message);
      })
      .finally(() => setBusy(false));
  };

  // The refusal and the row are the footer band's two children, spaced by the
  // band's own gap — see `.delivery-drawer-footer` in `deploy-surface.css`. The
  // submit button carries `form` rather than an `onClick`, because the footer
  // sits outside the `<form>`: that keeps the form's own `onSubmit` — and the
  // Enter key — the one path that runs.
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
        <button className="command-button" disabled={submitDisabled} form={formId} type="submit">
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
        id={formId}
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        {/* The scope decides everything after it: which identifier shapes are
            legal, and which validation methods stay available. */}
        <fieldset className="form-fieldset">
          <legend>{t("resource.certificates.certificateType")}</legend>
          <div aria-label={t("resource.certificates.certificateType")} className="segmented-control">
            {(["SINGLE_DOMAIN", "WILDCARD"] as const).map((value) => (
              <button
                aria-pressed={scope === value}
                disabled={busy}
                key={value}
                onClick={() => setScope(value)}
                type="button"
              >
                {value === "SINGLE_DOMAIN"
                  ? t("resource.certificates.scopeSingleDomain")
                  : t("resource.certificates.scopeWildcard")}
              </button>
            ))}
          </div>
          <small className="form-hint">
            {scope === "WILDCARD"
              ? t("resource.certificates.scopeWildcardHint")
              : t("resource.certificates.scopeSingleDomainHint")}
          </small>
        </fieldset>

        <fieldset className="form-fieldset">
          <legend>{t("resource.certificates.selectIdentifiers")}</legend>
          <div className="hostname-summary">
            <button className="secondary-button" disabled={busy} onClick={() => setPickerOpen(true)} type="button">
              <Globe2 size={15} />
              {t("resource.certificates.pickHostnames")}
            </button>
            <small className="form-hint">
              {chosenRootDomain === undefined
                ? t("resource.certificates.rootDomainUnset")
                : t("resource.certificates.rootDomainLabel", { apex: chosenRootDomain.hostname })}
            </small>
          </div>
          {selection !== undefined ? (
            <div className="selected-hostnames">
              <span>{t("resource.certificates.selectedHostname")}</span>
              <div>
                <span>
                  {selection.hostname}
                  <button
                    aria-label={t("resource.certificates.removeHostname", { hostname: selection.hostname })}
                    disabled={busy}
                    onClick={() => setSelection(undefined)}
                    title={t("resource.certificates.removeHostname", { hostname: selection.hostname })}
                    type="button"
                  >
                    <X size={12} />
                  </button>
                </span>
              </div>
            </div>
          ) : null}
          {selectionIssue !== undefined ? (
            <small className="form-error" role="alert">
              {t(selectionIssue)}
            </small>
          ) : null}
        </fieldset>

        {/* Self-signed or CA-issued. Not on the console's form, which issues
            ACME certificates only; kept here because this plane can do both. */}
        <fieldset className="form-fieldset">
          <legend>{t("resource.certificates.issueOptions")}</legend>
          <div className="hostname-summary">
            <label className="checkbox-field">
              <input
                checked={acme}
                disabled={busy}
                name="adminCertificateType"
                onChange={() => setCertType(1)}
                type="radio"
              />
              {t("resource.certificates.letsEncrypt")}
            </label>
            <label className="checkbox-field">
              <input
                checked={!acme}
                disabled={busy}
                name="adminCertificateType"
                onChange={() => setCertType(3)}
                type="radio"
              />
              {t("resource.certificates.selfSigned")}
            </label>
          </div>
        </fieldset>

        {/* DNS-01 is the method that needs an account — and `AUTO` may resolve
            to it — so the method and the account share a row rather than taking
            two. The account stays visible for every method: a wildcard shown as
            HTTP-01-ineligible is exactly when the operator needs to see which
            account will answer. */}
        <div className="dialog-pair dialog-pair-settings">
          <fieldset className="form-fieldset">
            <legend
              title={
                scope === "WILDCARD"
                  ? t("resource.certificates.validationWildcardRequiresDns01")
                  : t("resource.certificates.validationAutoHint")
              }
            >
              {t("resource.certificates.validationMethod")}
            </legend>
            <div aria-label={t("resource.certificates.validationMethod")} className="segmented-control">
              {(["AUTO", "DNS_01", "HTTP_01"] as const).map((value) => (
                <button
                  aria-pressed={effectiveValidationMethod === value}
                  disabled={busy || !acme || (scope === "WILDCARD" && value === "HTTP_01")}
                  key={value}
                  onClick={() => setValidationMethod(value)}
                  type="button"
                >
                  {value === "AUTO"
                    ? t("resource.certificates.validationAuto")
                    : value === "DNS_01"
                      ? t("resource.certificates.validationDns01")
                      : t("resource.certificates.validationHttp01")}
                </button>
              ))}
            </div>
          </fieldset>

          <CloudAccountField
            accounts={accounts}
            busy={busy || !acme}
            onChange={setProviderAccountId}
            onPick={() => setAccountPickerOpen(true)}
            t={t}
            value={providerAccountId}
          />
        </div>

        <div className="form-grid certificate-form-grid">
          <fieldset className="form-fieldset">
            <legend>{t("resource.certificates.keyAlgorithmTitle")}</legend>
            <div
              aria-label={t("resource.certificates.keyAlgorithmTitle")}
              className="segmented-control algorithm-control"
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
            <small className="form-hint">
              {keyAlgorithm === "RSA"
                ? t("resource.certificates.keyAlgorithmRsaHint")
                : t("resource.certificates.keyAlgorithmEcdsaHint")}
            </small>
          </fieldset>
          <label>
            <span title={t("resource.certificates.caProfileHint")}>{t("resource.certificates.caProfile")}</span>
            <select
              disabled={busy || !acme}
              onChange={(event) => setCaProfile(event.target.value as CertificateCaProfile)}
              value={caProfile}
            >
              <option value="LETS_ENCRYPT_PRODUCTION">{t("resource.certificates.caProfileProduction")}</option>
              <option value="LETS_ENCRYPT_STAGING">{t("resource.certificates.caProfileStaging")}</option>
            </select>
          </label>
          <label className="form-field-wide">
            <span title={t("resource.certificates.certNameFieldHint")}>
              {t("resource.certificates.certNameField")}
            </span>
            <input
              autoComplete="off"
              disabled={busy}
              onChange={(event) => setCertNameDraft(event.target.value)}
              title={t("resource.certificates.certNameFieldHint")}
              value={certNameDraft}
            />
          </label>
          <div className="form-field-wide">
            <label className="checkbox-field">
              <input checked={autoRenew} disabled={busy} onChange={() => setAutoRenew((value) => !value)} type="checkbox" />
              {t("resource.certificates.autoRenew")}
            </label>
          </div>
          <label>
            <span title={t("resource.certificates.renewBeforeDaysHint")}>
              {t("resource.certificates.renewBeforeDays")}
            </span>
            <input
              aria-invalid={renewIssue !== undefined}
              disabled={busy}
              inputMode="numeric"
              max={MAXIMUM_RENEW_BEFORE_DAYS}
              min={MINIMUM_RENEW_BEFORE_DAYS}
              onChange={(event) => setRenewBeforeDays(event.target.value)}
              step={1}
              title={t("resource.certificates.renewBeforeDaysHint")}
              type="number"
              value={renewBeforeDays}
            />
            {renewIssue !== undefined ? (
              <small className="form-error" role="alert">
                {t(renewIssue)}
              </small>
            ) : null}
          </label>
        </div>
      </form>
      {/* A sibling of the form, not a descendant: anything that mounts a form of
          its own belongs outside this one, or its submit button submits the
          outer form instead. */}
      {pickerOpen ? (
        <HostnamePickerDialog
          apply={(next) => {
            setChosenRootDomainId(next.rootDomainId);
            setSelection(next.selection);
            setPickerOpen(false);
          }}
          close={() => setPickerOpen(false)}
          initialRootDomainId={chosenRootDomainId}
          scope={scope}
          selection={selection}
          t={t}
        />
      ) : null}
      {accountPickerOpen && accounts !== null ? (
        <CloudAccountPickerDialog
          accounts={accounts}
          apply={(next) => {
            setProviderAccountId(next);
            setAccountPickerOpen(false);
          }}
          close={() => setAccountPickerOpen(false)}
          t={t}
          value={providerAccountId}
        />
      ) : null}
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
                {t(`resource.certificates.reason.${value}`)}
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
          .catch((cause) => onError(errorText(cause, t)))
          .finally(() => setBusy(false));
      }}
      t={t}
      title={t("resource.certificates.revoke")}
    />
  );
}
