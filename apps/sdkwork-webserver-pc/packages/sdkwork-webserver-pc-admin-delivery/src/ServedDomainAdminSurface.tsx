import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type {
  ApplicationDomainResponse,
  DomainVerifyResponse,
  RootDomainResponse,
} from "@sdkwork/webserver-pc-admin-core";
import type { WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { ArrowLeft, CirclePause, CirclePlay, FileKey2, Globe2, Pencil, Plus, RefreshCw, Search, ShieldCheck, Trash2 } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { Link, Route, Routes, useParams } from "react-router-dom";

import {
  ConfirmDialog,
  DialogBackdrop,
  DialogCloseButton,
  FormDialog,
  Metric,
  Pagination,
  StatusBadge,
  errorText,
  formatInstant,
  newIdempotencyKey,
  translator,
  type Translator,
} from "./AdminSurfaceAtoms.tsx";

/**
 * The served-domain inventory: which root domains this edge answers for, and
 * which hostname under each one is registered.
 *
 * The rows are not hand-maintained. The gateway reconciles them from the
 * configuration it actually serves (the effective nginx sidecar plus every
 * configured module import) once the database is up, so opening this page after
 * a config change shows what the edge is really serving, not what somebody
 * remembered to type. Manual entries are still possible — a hostname the edge
 * does not serve yet can be registered here — but they are a supplement to the
 * reconciled inventory, never its replacement.
 *
 * Both levels are tenant-level: rows are written with `user_id IS NULL`, which
 * is why this page lives on the operations surface and not in the tenant
 * console.
 *
 * ## Why this page is dressed in the Deployments surface vocabulary
 *
 * The tenant console's Domains page and this one show the same entity at two
 * ownership levels, and an operator reads them side by side. The console pair is
 * the canonical `sdkwork-deployments` page, bridged in through
 * `DeployDomainManagementSurface`, so its look comes from the mirror stylesheet
 * in `src/deploy-surface.css` — every rule scoped under `.deploy-surface`.
 * Authoring this page against the host's *registry* vocabulary instead
 * (`resource-toolbar` / `toolbar-meta` / the framework `DataTable`) is what made
 * it read as a different product: neither of those two class names has a single
 * rule in this app, so the toolbar rendered as unstyled stacked children, and
 * `.data-surface` is a two-row grid that six children fell out of.
 *
 * Wrapping the page in `.deploy-surface` and speaking that vocabulary is the
 * whole fix: the geometry, the command bar, the ledger, the empty state, the
 * status chips, the pager and the operation column all become the console's,
 * because they are literally the same rules. It also matches what
 * `DeployAppsAdminSurface` already does — the admin Applications page *is* the
 * console page, not a re-drawing of it.
 *
 * The page renders inside `WebserverAdminSdkProvider`, so the client is
 * injected through the admin-core hook; no transport is constructed here.
 */

const PAGE_SIZE = 50;

/**
 * The operations surface's certificate ledger.
 *
 * The tenant console's zone row links to `/console/certificates?zoneId=…&apex=…`
 * and that page opens its request form pre-scoped to the zone; the operations
 * surface mirrors the gesture at its own base path. The two paths are written
 * out rather than derived because the base path belongs to the host that mounts
 * the surface (`WebserverAuthorizedWorkspace` fixes `/admin` and `/console`), and
 * a capability package has no way to ask for it — the console hardcodes its half
 * for the same reason.
 */
const CERTIFICATES_PATH = "/admin/certificates";

/** Root-domain lifecycle on the wire: 0=pending, 1=active, 2=disabled. */
const ALL_STATUSES = "ALL";
const STATUS_FILTERS = [ALL_STATUSES, 1, 0, 2] as const;
type StatusFilter = (typeof STATUS_FILTERS)[number];

export interface ServedDomainAdminSurfaceProps {
  locale: WebserverLocale;
  resource: "domains";
}

export function ServedDomainAdminSurface({ locale, resource }: ServedDomainAdminSurfaceProps) {
  return (
    <div className="deploy-surface" data-resource={resource}>
      {/* Two levels, two routes — the shape the console uses, so opening a root
          domain is a navigation an operator can link to and come back from. An
          unparsable tail renders the root ledger rather than a blank pane. */}
      <Routes>
        <Route element={<RootDomainLedger locale={locale} />} index />
        <Route element={<RootDomainHostnames locale={locale} />} path=":rootDomainId" />
        <Route element={<RootDomainLedger locale={locale} />} path="*" />
      </Routes>
    </div>
  );
}

function RootDomainLedger({ locale }: { locale: WebserverLocale }) {
  const client = useWebserverAdminSdk();
  const t = translator(locale);
  const [roots, setRoots] = useState<RootDomainResponse[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [status, setStatus] = useState<StatusFilter>(ALL_STATUSES);
  const [searchDraft, setSearchDraft] = useState("");
  const [keyword, setKeyword] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<RootDomainResponse>();
  const [editTarget, setEditTarget] = useState<RootDomainResponse>();
  const [statusTarget, setStatusTarget] = useState<RootDomainResponse>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    setBusy(true);
    setError(undefined);
    void client.domain.rootDomains
      .list({
        page,
        pageSize: PAGE_SIZE,
        // Absent means "every status", which is what the ALL segment means too —
        // so ALL omits the parameter rather than naming a wildcard the wire has
        // no value for.
        status: status === ALL_STATUSES ? undefined : status,
        q: keyword || undefined,
      })
      .then((result) => {
        if (!active) return;
        setRoots(result.items);
        // `PageInfo.hasMore` is optional on the wire; an absent flag means "no
        // continuation", never "unknown".
        setHasMore(result.pageInfo.hasMore === true);
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
  }, [client, keyword, page, status]);

  const removeRoot = (root: RootDomainResponse) => {
    setBusy(true);
    void client.domain.rootDomains
      .delete(root.id, { idempotencyKey: newIdempotencyKey() })
      .then(() => {
        setDeleteTarget(undefined);
        // Re-read rather than splice in place: the delete only succeeds on an
        // empty root, and a refused delete has to leave the row and its
        // counters exactly as the server still reports them.
        setRoots(null);
        setPage(1);
      })
      .catch((cause) => {
        setDeleteTarget(undefined);
        setError(errorText(cause, t));
      })
      .finally(() => setBusy(false));
  };

  /**
   * Edit and lifecycle share one call shape.
   *
   * The contract sends only the members the caller names, so the edit form and
   * the pause/resume confirmation are two bodies of the same request rather than
   * two endpoints — which is also why the console reaches both through one
   * `updateDomainZone`.
   */
  const patchRoot = (
    root: RootDomainResponse,
    body: { displayName?: string; dnsProvider?: string; providerZoneRef?: string; status?: number },
    done: () => void,
  ) => {
    setBusy(true);
    void client.domain.rootDomains
      .update(root.id, body, { idempotencyKey: newIdempotencyKey() })
      .then(() => {
        done();
        setRoots(null);
      })
      .catch((cause) => {
        done();
        setError(errorText(cause, t));
      })
      .finally(() => setBusy(false));
  };

  return (
    <section className="resource-page domain-page">
      <div className="resource-commandbar">
        <div className="resource-identity">
          <h1>{t("resource.domains.admin.label")}</h1>
        </div>
        <div className="resource-query">
          <form
            className="search-box"
            onSubmit={(event) => {
              event.preventDefault();
              setPage(1);
              setKeyword(searchDraft.trim());
            }}
          >
            <Search size={16} />
            <input
              aria-label={t("resource.domains.search")}
              onChange={(event) => setSearchDraft(event.target.value)}
              placeholder={t("resource.domains.search")}
              value={searchDraft}
            />
          </form>
          {/* The state filter keeps the console's exact control; the ownership
              tabs the page is also meant to carry slot in beside it without
              competing, because they answer a different question. */}
          <div aria-label={t("resource.domains.status")} className="segmented-control" role="group">
            {STATUS_FILTERS.map((value) => (
              <button
                aria-pressed={status === value}
                key={String(value)}
                onClick={() => {
                  setPage(1);
                  setStatus(value);
                }}
                type="button"
              >
                {statusLabel(value, t)}
              </button>
            ))}
          </div>
        </div>
        <div className="actions">
          <button
            className="icon-button"
            disabled={busy}
            onClick={() => {
              setRoots(null);
              setPage(1);
            }}
            aria-label={t("resource.domains.refresh")}
            title={t("resource.domains.refresh")}
            type="button"
          >
            <RefreshCw size={17} />
          </button>
          <button className="command-button" onClick={() => setCreateOpen(true)} type="button">
            <Plus size={16} />
            {t("resource.domains.defineRoot")}
          </button>
        </div>
      </div>

      {error ? (
        <div className="error-banner" role="alert">
          {error}
        </div>
      ) : null}

      {roots === null && !error ? (
        <div className="resource-loading" role="status">
          <p>{t("resource.domains.loading")}</p>
        </div>
      ) : (
        <>
          <div aria-busy={busy} className="table-frame domain-table-frame">
            <table className="domain-table">
              <thead>
                <tr>
                  <th>{t("resource.domains.rootHostname")}</th>
                  <th>{t("resource.domains.status")}</th>
                  <th>{t("resource.domains.subdomainCount")}</th>
                  <th>{t("resource.domains.httpsCount")}</th>
                  <th>{t("resource.domains.boundCount")}</th>
                  <th>{t("resource.domains.deploymentCount")}</th>
                  <th>{t("resource.domains.updatedAt")}</th>
                  <th className="operations-column">{t("resource.domains.operations")}</th>
                </tr>
              </thead>
              <tbody>
                {(roots ?? []).map((root) => {
                  // The console blocks the delete while the zone still owns
                  // anything at all — `hostnameCount > 1 || certificateCount > 0
                  // || bindingCount > 0`. Its `> 1` is "more than the apex row
                  // itself", because the Deployments plane registers the apex as
                  // a hostname of its own zone. On this plane the apex lives in
                  // `webserver_root_domain` and is never duplicated into
                  // `webserver_domain`, so the same intent reads as "any child at
                  // all". It is also verbatim the predicate the delete endpoint
                  // enforces (`subdomain_count > 0` → 409), so the button's
                  // availability follows the call it makes rather than promising
                  // one that cannot succeed.
                  const deleteBlocked = Number(root.subdomainCount) > 0;
                  return (
                  <tr key={root.id}>
                    <td>
                      <Link className="primary-cell-link" to={root.id}>
                        <Globe2 size={17} />
                        <span>
                          <strong>{root.hostname}</strong>
                        </span>
                      </Link>
                    </td>
                    <td>
                      <StatusBadge t={t} value={rootStatusLabel(root.status)} />
                    </td>
                    <td>
                      <strong>{root.subdomainCount}</strong>
                      <small className="cell-subtitle">
                        {t("resource.domains.verifiedSummary", {
                          total: root.subdomainCount,
                          verified: root.verifiedSubdomainCount,
                        })}
                      </small>
                    </td>
                    <td>{root.httpsSubdomainCount}</td>
                    <td>{root.boundSubdomainCount}</td>
                    <td>{root.activeDeploymentCount}</td>
                    <td>{formatInstant(root.updatedAt, locale)}</td>
                    <td>
                      <div className="row-actions">
                        {/* The first two actions carry words rather than bare
                            glyphs, exactly as the console's zone ledger does it:
                            entering the hostname list and requesting a
                            certificate are the two things an operator opens this
                            table for, and neither is guessable from an icon
                            alone. The literal word stays inside the accessible
                            name so the label still matches what is read out. */}
                        <Link
                          aria-label={`${t("resource.domains.hostnames")} · ${root.hostname}`}
                          className="table-action table-action-text"
                          title={t("resource.domains.openHostnames")}
                          to={root.id}
                        >
                          <Globe2 size={15} />
                          <span>{t("resource.domains.hostnames")}</span>
                        </Link>
                        <Link
                          aria-label={`${t("resource.domains.certificates")} · ${root.hostname}`}
                          className="table-action table-action-text"
                          title={t("resource.domains.requestCertificate")}
                          to={`${CERTIFICATES_PATH}?rootDomainId=${encodeURIComponent(root.id)}&apex=${encodeURIComponent(root.hostname)}`}
                        >
                          <FileKey2 size={15} />
                          <span>{t("resource.domains.certificates")}</span>
                        </Link>
                        <button
                          aria-label={`${t("resource.domains.edit")} ${root.hostname}`}
                          className="table-action"
                          onClick={() => setEditTarget(root)}
                          title={t("resource.domains.edit")}
                          type="button"
                        >
                          <Pencil size={16} />
                        </button>
                        <button
                          aria-label={`${
                            root.status === 1 ? t("resource.domains.pause") : t("resource.domains.resume")
                          } ${root.hostname}`}
                          className="table-action"
                          onClick={() => setStatusTarget(root)}
                          title={root.status === 1 ? t("resource.domains.pause") : t("resource.domains.resume")}
                          type="button"
                        >
                          {root.status === 1 ? <CirclePause size={16} /> : <CirclePlay size={16} />}
                        </button>
                        <button
                          aria-label={`${t("resource.domains.delete")} ${root.hostname}`}
                          className="table-action danger-action"
                          disabled={busy || deleteBlocked}
                          onClick={() => setDeleteTarget(root)}
                          title={deleteBlocked ? t("resource.domains.deleteBlocked") : t("resource.domains.delete")}
                          type="button"
                        >
                          <Trash2 size={16} />
                        </button>
                      </div>
                    </td>
                  </tr>
                  );
                })}
              </tbody>
            </table>
            {!busy && (roots ?? []).length === 0 ? (
              <div className="empty-state">
                <Globe2 size={24} />
                {t("resource.domains.noRoots")}
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

      {createOpen ? (
        <FormDialog
          close={() => setCreateOpen(false)}
          submit={async (hostname) => {
            await client.domain.rootDomains.create({ hostname }, { idempotencyKey: newIdempotencyKey() });
            setCreateOpen(false);
            setRoots(null);
            setPage(1);
          }}
          submitLabel={t("resource.domains.create")}
          t={t}
          title={t("resource.domains.defineRoot")}
        >
          {(disabled) => (
            <label>
              {t("resource.domains.rootHostname")}
              <input
                disabled={disabled}
                name="hostname"
                placeholder={t("resource.domains.addRootPlaceholder")}
                required
                type="text"
              />
            </label>
          )}
        </FormDialog>
      ) : null}

      {deleteTarget ? (
        <ConfirmDialog
          close={() => setDeleteTarget(undefined)}
          confirmLabel={t("resource.domains.delete")}
          dangerous
          message={t("resource.domains.deleteRootConfirm", { hostname: deleteTarget.hostname })}
          onConfirm={() => removeRoot(deleteTarget)}
          t={t}
          title={t("resource.domains.delete")}
        />
      ) : null}

      {editTarget ? (
        <EditRootDomainDialog
          close={() => setEditTarget(undefined)}
          onError={setError}
          root={editTarget}
          submit={(body) => patchRoot(editTarget, body, () => setEditTarget(undefined))}
          t={t}
        />
      ) : null}

      {/* Pausing and resuming are one dialog because they are one decision seen
          from two sides, and the wording carries which side this is. The console
          asks the same question on its zone ledger. */}
      {statusTarget ? (
        <ConfirmDialog
          close={() => setStatusTarget(undefined)}
          confirmLabel={statusTarget.status === 1 ? t("resource.domains.pause") : t("resource.domains.resume")}
          dangerous={statusTarget.status === 1}
          message={
            statusTarget.status === 1
              ? t("resource.domains.pauseRootConfirm")
              : t("resource.domains.resumeRootConfirm")
          }
          onConfirm={() =>
            patchRoot(
              statusTarget,
              // 1 = active, 2 = disabled, which is the lifecycle the column
              // itself declares (`chk_webserver_root_domain_status`).
              { status: statusTarget.status === 1 ? 2 : 1 },
              () => setStatusTarget(undefined),
            )
          }
          t={t}
          title={
            statusTarget.status === 1
              ? t("resource.domains.pauseRootTitle")
              : t("resource.domains.resumeRootTitle")
          }
        />
      ) : null}
    </section>
  );
}

/**
 * Root-domain edit form.
 *
 * The three fields the plane stores, in the console's dialog chrome. The apex is
 * shown read-only rather than as a disabled input: it is the row's identity — a
 * wildcard or a subdomain here would break the uniqueness index every hostname
 * under it resolves against — so it is a value being shown, not a field being
 * edited.
 *
 * A field left blank is omitted from the request, which the contract reads as
 * "leave it as it is". That is the same reading the console's zone form has, and
 * it is why saving with nothing changed is refused here instead of being sent as
 * a request that would come back 422: there is no member to send.
 */
function EditRootDomainDialog({
  close,
  onError,
  root,
  submit,
  t,
}: {
  close(): void;
  onError(message: string | undefined): void;
  root: RootDomainResponse;
  submit(body: { displayName?: string; dnsProvider?: string; providerZoneRef?: string }): void;
  t: Translator;
}) {
  const [displayName, setDisplayName] = useState(root.displayName ?? "");
  const [dnsProvider, setDnsProvider] = useState(root.dnsProvider ?? "");
  const [providerZoneRef, setProviderZoneRef] = useState(root.providerZoneRef ?? "");

  const fields = { displayName, dnsProvider, providerZoneRef };
  const current = {
    displayName: root.displayName ?? "",
    dnsProvider: root.dnsProvider ?? "",
    providerZoneRef: root.providerZoneRef ?? "",
  };
  const changed = (Object.keys(fields) as (keyof typeof fields)[]).filter(
    (key) => fields[key].trim() !== current[key],
  );

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (changed.length === 0) return;
    onError(undefined);
    const body: { displayName?: string; dnsProvider?: string; providerZoneRef?: string } = {};
    for (const key of changed) {
      const value = fields[key].trim();
      // Blank means "left alone", so it is dropped rather than sent: the wire
      // has no "clear this" for these three, and sending an empty string would
      // be rejected by `minLength: 1` even though it reads as the same intent.
      if (value !== "") body[key] = value;
    }
    if (Object.keys(body).length === 0) {
      onError(t("resource.domains.editNeedsAValue"));
      return;
    }
    submit(body);
  };

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
      role="presentation"
    >
      <form
        aria-labelledby="served-domain-edit-title"
        aria-modal="true"
        className="dialog delivery-dialog"
        onSubmit={onSubmit}
        role="dialog"
      >
        <header>
          <h2 id="served-domain-edit-title">{t("resource.domains.editRoot")}</h2>
        </header>
        <div className="form-grid">
          <label className="form-field-wide">
            <span>{t("resource.domains.rootHostname")}</span>
            <input readOnly value={root.hostname} />
          </label>
          <label>
            <span>{t("resource.domains.displayName")}</span>
            <input
              name="displayName"
              onChange={(event) => setDisplayName(event.target.value)}
              placeholder={root.hostname}
              value={displayName}
            />
          </label>
          <label>
            <span>{t("resource.domains.dnsProvider")}</span>
            <input
              name="dnsProvider"
              onChange={(event) => setDnsProvider(event.target.value)}
              value={dnsProvider}
            />
          </label>
          <label className="form-field-wide">
            <span>{t("resource.domains.providerZoneRef")}</span>
            <input
              name="providerZoneRef"
              onChange={(event) => setProviderZoneRef(event.target.value)}
              value={providerZoneRef}
            />
          </label>
        </div>
        <footer className="dialog-footer">
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.domains.cancel")}
          </button>
          <button className="command-button" disabled={changed.length === 0} type="submit">
            {t("resource.domains.save")}
          </button>
        </footer>
      </form>
    </div>
  );
}

function RootDomainHostnames({ locale }: { locale: WebserverLocale }) {
  const client = useWebserverAdminSdk();
  const t = translator(locale);
  const { rootDomainId = "" } = useParams();
  const [root, setRoot] = useState<RootDomainResponse>();
  const [hostnames, setHostnames] = useState<ApplicationDomainResponse[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [createOpen, setCreateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<ApplicationDomainResponse>();
  // Ownership verification for one declared hostname. The challenge and the
  // control that re-runs it live together, because the second call is the same
  // call: re-reading the record is how a published TXT value is noticed.
  const [verifyTarget, setVerifyTarget] = useState<ApplicationDomainResponse>();
  const [challenge, setChallenge] = useState<DomainVerifyResponse>();
  // The record name and its wildcard form are one answer, so they are one piece
  // of state: ticking the box *is* setting the name to `*`, and clearing it gives
  // the operator back whatever they had typed. Keeping the two apart would let
  // the field and the box disagree about what the request is about to carry.
  const [recordNameDraft, setRecordNameDraft] = useState("");
  const [wildcardRecord, setWildcardRecord] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    setBusy(true);
    setError(undefined);
    // Both reads in one pass: the heading is the root's own hostname, and a
    // hostname ledger titled "-" because its zone read had not landed yet is the
    // one state an operator would read as a failure.
    void Promise.all([
      client.domain.rootDomains.retrieve(rootDomainId),
      client.domain.rootDomains.subdomains.list(rootDomainId, { page, pageSize: PAGE_SIZE }),
    ])
      .then(([rootResult, hostnameResult]) => {
        if (!active) return;
        setRoot(rootResult);
        setHostnames(hostnameResult.items);
        setHasMore(hostnameResult.pageInfo.hasMore === true);
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
  }, [client, page, rootDomainId]);

  const removeHostname = (hostname: ApplicationDomainResponse) => {
    setBusy(true);
    void client.domain
      .delete(hostname.id, { idempotencyKey: newIdempotencyKey() })
      .then(() => {
        setDeleteTarget(undefined);
        setHostnames(null);
        setPage(1);
      })
      .catch((cause) => {
        setDeleteTarget(undefined);
        setError(errorText(cause, t));
      })
      .finally(() => setBusy(false));
  };

  /**
   * Create or re-check one hostname's ownership challenge.
   *
   * The endpoint is one operation with two readings, which is why a single button
   * serves both: the first call mints the TXT record, and every later call
   * observes DNS again and answers with the current verdict. A verified answer
   * reloads the ledger, because the row the certificate picker reads is the
   * stored one — the response alone would leave the table still saying PENDING.
   *
   * This is the step the wildcard flow cannot skip. `*.example.com` is not
   * something the edge reconciles from its own configuration unless the
   * configuration already names it, so a wildcard hostname declared here has no
   * other route to `VERIFIED` — and a certificate identifier may only name a
   * verified hostname.
   */
  const runVerification = (hostname: ApplicationDomainResponse) => {
    setBusy(true);
    setError(undefined);
    void client.domain
      .verify(hostname.id, { idempotencyKey: newIdempotencyKey() })
      .then((result) => {
        setChallenge(result);
        if (result.verified) {
          setHostnames(null);
          setPage(1);
        }
      })
      .catch((cause) => setError(errorText(cause, t)))
      .finally(() => setBusy(false));
  };

  const startVerification = (hostname: ApplicationDomainResponse) => {
    setVerifyTarget(hostname);
    setChallenge(undefined);
    runVerification(hostname);
  };

  return (
    <section className="resource-page domain-page">
      <Link className="back-link" to="..">
        <ArrowLeft size={16} />
        {t("resource.domains.backToDomains")}
      </Link>
      <div className="resource-commandbar">
        <div className="resource-identity">
          <h1>{root?.hostname ?? "-"}</h1>
        </div>
        <div className="actions">
          <button
            className="icon-button"
            disabled={busy}
            onClick={() => {
              setHostnames(null);
              setPage(1);
            }}
            aria-label={t("resource.domains.refresh")}
            title={t("resource.domains.refresh")}
            type="button"
          >
            <RefreshCw size={17} />
          </button>
          <button className="command-button" onClick={() => setCreateOpen(true)} type="button">
            <Plus size={16} />
            {t("resource.domains.addSubdomain")}
          </button>
        </div>
      </div>

      {root ? (
        <div className="metric-strip">
          <Metric
            label={t("resource.domains.verifiedCount")}
            value={t("resource.domains.verifiedSummary", {
              total: root.subdomainCount,
              verified: root.verifiedSubdomainCount,
            })}
          />
          <Metric label={t("resource.domains.httpsCount")} value={root.httpsSubdomainCount} />
          <Metric label={t("resource.domains.boundCount")} value={root.boundSubdomainCount} />
          <Metric label={t("resource.domains.deploymentCount")} value={root.activeDeploymentCount} />
        </div>
      ) : null}

      {error ? (
        <div className="error-banner" role="alert">
          {error}
        </div>
      ) : null}

      {hostnames === null && !error ? (
        <div className="resource-loading" role="status">
          <p>{t("resource.domains.loading")}</p>
        </div>
      ) : (
        <div aria-busy={busy} className="table-frame domain-table-frame">
          <table className="domain-table">
            <thead>
              <tr>
                <th>{t("resource.domains.subdomainHostname")}</th>
                <th>{t("resource.domains.recordName")}</th>
                <th>{t("resource.domains.verification")}</th>
                <th>{t("resource.domains.ssl")}</th>
                <th>{t("resource.domains.application")}</th>
                <th>{t("resource.domains.certificateCount")}</th>
                <th className="operations-column">{t("resource.domains.operations")}</th>
              </tr>
            </thead>
            <tbody>
              {(hostnames ?? []).map((hostname) => (
                <tr key={hostname.id}>
                  <td>
                    <span className="hostname-cell">
                      <Globe2 size={16} />
                      <span>
                        <strong>{hostname.hostname}</strong>
                        {hostname.isPrimary ? <small>{t("resource.domains.primary")}</small> : null}
                      </span>
                    </span>
                  </td>
                  <td>{hostname.recordName || "-"}</td>
                  <td>
                    <StatusBadge t={t} value={hostname.isVerified ? "VERIFIED" : "PENDING"} />
                  </td>
                  <td>
                    {hostname.sslEnabled ? hostname.sslProvider || t("resource.domains.yes") : t("resource.domains.sslOff")}
                  </td>
                  <td>{hostname.applicationName || "-"}</td>
                  <td>{hostname.certificateCount}</td>
                  <td>
                    <div className="row-actions">
                      {/* Offered only while the row is unverified: ownership
                          evidence is the issuance path's precondition, so this is
                          the step that turns a declared hostname into one a
                          certificate can cover — and on an already-verified row it
                          is a control that does nothing. */}
                      {hostname.isVerified ? null : (
                        <button
                          aria-label={`${t("resource.domains.verify")} ${hostname.hostname}`}
                          className="table-action"
                          disabled={busy}
                          onClick={() => startVerification(hostname)}
                          title={t("resource.domains.verifyActionHint")}
                          type="button"
                        >
                          <ShieldCheck size={16} />
                        </button>
                      )}
                      <button
                        aria-label={`${t("resource.domains.delete")} ${hostname.hostname}`}
                        className="table-action danger-action"
                        disabled={busy || hostname.isPrimary}
                        onClick={() => setDeleteTarget(hostname)}
                        // The apex row cannot go on its own: removing it is what
                        // removing the whole root domain does, and that is a row
                        // action on the ledger above.
                        title={hostname.isPrimary ? t("resource.domains.deleteApexBlocked") : t("resource.domains.delete")}
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
          {!busy && (hostnames ?? []).length === 0 ? (
            <div className="empty-state">
              <Globe2 size={24} />
              {t("resource.domains.noSubdomains")}
            </div>
          ) : null}
        </div>
      )}

      <Pagination
        busy={busy}
        hasMore={hasMore}
        onNext={() => setPage((current) => current + 1)}
        onPrevious={() => setPage((current) => Math.max(1, current - 1))}
        page={page}
        t={t}
      />

      {createOpen ? (
        <FormDialog
          close={() => setCreateOpen(false)}
          submit={async (recordName) => {
            await client.domain.rootDomains.subdomains.create(
              rootDomainId,
              { recordName, sslEnabled: true },
              { idempotencyKey: newIdempotencyKey() },
            );
            setCreateOpen(false);
            setWildcardRecord(false);
            setRecordNameDraft("");
            setHostnames(null);
            setPage(1);
          }}
          submitLabel={t("resource.domains.create")}
          t={t}
          title={t("resource.domains.addSubdomain")}
        >
          {(disabled) => (
            <>
              {/* The record-name field is first in the tree, and the checkbox
                  below it carries no `name`: `FormDialog` submits the first named
                  control's value, so the box toggles what the field holds rather
                  than competing with it for the one value the dialog has. */}
              <label>
                {t("resource.domains.recordName")}
                <input
                  disabled={disabled}
                  name="recordName"
                  onChange={(event) => setRecordNameDraft(event.target.value)}
                  placeholder={
                    wildcardRecord ? "*" : t("resource.domains.addSubdomainPlaceholder")
                  }
                  readOnly={wildcardRecord}
                  required
                  type="text"
                  value={wildcardRecord ? "*" : recordNameDraft}
                />
              </label>
              {/* `readOnly`, never `disabled`: a disabled control is dropped from
                  the submitted form data, so the wildcard value would vanish and
                  the dialog would submit the checkbox instead. */}
              <label className="checkbox-field">
                <input
                  checked={wildcardRecord}
                  disabled={disabled}
                  onChange={() => setWildcardRecord((value) => !value)}
                  type="checkbox"
                />
                {t("resource.domains.addSubdomainWildcard")}
              </label>
              <small className="form-hint">
                {t("resource.domains.addSubdomainWildcardHint", {
                  hostname: `*.${root?.hostname ?? ""}`,
                })}
              </small>
            </>
          )}
        </FormDialog>
      ) : null}

      {verifyTarget ? (
        <VerifyHostnameDialog
          busy={busy}
          challenge={challenge}
          close={() => setVerifyTarget(undefined)}
          domain={verifyTarget}
          locale={locale}
          recheck={() => runVerification(verifyTarget)}
          t={t}
        />
      ) : null}

      {deleteTarget ? (
        <ConfirmDialog
          close={() => setDeleteTarget(undefined)}
          confirmLabel={t("resource.domains.delete")}
          dangerous
          message={t("resource.domains.deleteSubdomainConfirm", { hostname: deleteTarget.hostname })}
          onConfirm={() => removeHostname(deleteTarget)}
          t={t}
          title={t("resource.domains.delete")}
        />
      ) : null}
    </section>
  );
}

/**
 * Ownership verification for one declared hostname.
 *
 * A hostname registered here is stored `PENDING`, and the issuance path resolves
 * certificate identifiers against `VERIFIED` rows only — so a name that is never
 * verified can be declared and then covered by nothing. The challenge is a TXT
 * record; the same control re-runs the check, which is what makes this a loop
 * rather than a one-shot.
 *
 * The wildcard form is the reason this dialog exists at all: `*.example.com` is
 * not reconciled from the edge's own configuration unless that configuration
 * already names it, so a wildcard hostname declared here has no other route to
 * `VERIFIED`. Its challenge is the same TXT name as the apex's, because the
 * ownership question a wildcard asks is about the zone, not the star.
 */
function VerifyHostnameDialog({
  busy,
  challenge,
  close,
  domain,
  locale,
  recheck,
  t,
}: {
  busy: boolean;
  challenge: DomainVerifyResponse | undefined;
  close(): void;
  domain: ApplicationDomainResponse;
  locale: WebserverLocale;
  recheck(): void;
  t: Translator;
}) {
  return (
    <DialogBackdrop close={close}>
      <div
        aria-labelledby="served-domain-verify-title"
        aria-modal="true"
        className="dialog delivery-dialog"
        role="dialog"
      >
        <header>
          <h2 id="served-domain-verify-title">
            {t("resource.domains.verifyTitle", { hostname: domain.hostname })}
          </h2>
          <DialogCloseButton close={close} label={t("resource.domains.cancel")} />
        </header>
        {challenge === undefined ? (
          // No challenge yet: the first call either answers or fails, and the
          // failure is reported by the ledger's banner behind this dialog.
          <p className="form-hint">{busy ? t("resource.domains.verifyRunning") : t("resource.domains.verifyIdle")}</p>
        ) : (
          <div className="form-grid single-column">
            <p className="form-hint">
              {challenge.verified
                ? t("resource.domains.verifyVerifiedHint")
                : t("resource.domains.verifyHint")}
            </p>
            {/* The two values are the whole instruction an operator needs: which
                record name to create and what to put in it. Read-only rather than
                disabled, so both can still be selected and copied. */}
            <label>
              {t("resource.domains.verifyRecordName")}
              <input readOnly value={challenge.recordName} />
            </label>
            <label>
              {t("resource.domains.verifyRecordValue")}
              <input readOnly value={challenge.recordValue} />
            </label>
            <div className="hostname-summary">
              <StatusBadge t={t} value={challenge.status} />
              <small className="form-hint">
                {t("resource.domains.verifyAttempts", {
                  count: challenge.attemptCount,
                  expiresAt: formatInstant(challenge.expiresAt, locale),
                })}
              </small>
            </div>
            {challenge.failureCode === undefined ? null : (
              <small className="form-error">{challenge.failureCode}</small>
            )}
          </div>
        )}
        <footer className="dialog-footer">
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.domains.cancel")}
          </button>
          <button className="command-button" disabled={busy} onClick={recheck} type="button">
            {t("resource.domains.verifyRecheck")}
          </button>
        </footer>
      </div>
    </DialogBackdrop>
  );
}

function statusLabel(value: StatusFilter, t: ReturnType<typeof translator>): string {
  if (value === ALL_STATUSES) return t("resource.domains.all");
  if (value === 1) return t("resource.domains.active");
  if (value === 0) return t("resource.domains.pending");
  return t("resource.domains.disabled");
}

function rootStatusLabel(status: number): string {
  if (status === 1) return "ACTIVE";
  if (status === 2) return "DISABLED";
  return "PENDING";
}
