import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type { ApplicationDomainResponse, RootDomainResponse } from "@sdkwork/webserver-pc-admin-core";
import type { WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { ArrowLeft, Globe2, Plus, RefreshCw, Search, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, Route, Routes, useParams } from "react-router-dom";

import {
  ConfirmDialog,
  FormDialog,
  Metric,
  Pagination,
  StatusBadge,
  errorText,
  formatInstant,
  newIdempotencyKey,
  translator,
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
        if (active) setError(errorText(cause));
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
        setError(errorText(cause));
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
                {(roots ?? []).map((root) => (
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
                        {/* Text label rather than a bare glyph, exactly as the
                            console's zone ledger does it: entering the hostname
                            list is the thing an operator opens this table for,
                            and it is not guessable from a globe icon alone. The
                            literal word also stays inside the accessible name. */}
                        <Link
                          aria-label={`${t("resource.domains.openHostnames")} · ${root.hostname}`}
                          className="table-action table-action-text"
                          title={t("resource.domains.openHostnames")}
                          to={root.id}
                        >
                          <Globe2 size={15} />
                          <span>{t("resource.domains.openHostnames")}</span>
                        </Link>
                        <button
                          aria-label={`${t("resource.domains.delete")} ${root.hostname}`}
                          className="table-action danger-action"
                          disabled={busy}
                          onClick={() => setDeleteTarget(root)}
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
    </section>
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
        if (active) setError(errorText(cause));
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
        setError(errorText(cause));
      })
      .finally(() => setBusy(false));
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
            setHostnames(null);
            setPage(1);
          }}
          submitLabel={t("resource.domains.create")}
          t={t}
          title={t("resource.domains.addSubdomain")}
        >
          {(disabled) => (
            <label>
              {t("resource.domains.recordName")}
              <input
                disabled={disabled}
                name="recordName"
                placeholder={t("resource.domains.addSubdomainPlaceholder")}
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
          message={t("resource.domains.deleteSubdomainConfirm", { hostname: deleteTarget.hostname })}
          onConfirm={() => removeHostname(deleteTarget)}
          t={t}
          title={t("resource.domains.delete")}
        />
      ) : null}
    </section>
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
