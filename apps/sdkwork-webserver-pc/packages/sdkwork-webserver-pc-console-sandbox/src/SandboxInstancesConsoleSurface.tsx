import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createSandboxAppClient,
  type SandboxAppClient,
  type SandboxInstance,
  type SandboxInstanceState,
} from "@sdkwork/webserver-pc-console-core";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SandboxProvisionForm, type SandboxProvisionFormSubmit } from "./SandboxProvisionForm.tsx";
import {
  SANDBOX_CAPABILITY_LABEL_KEYS,
  SANDBOX_STATE_LABEL_KEYS,
  SANDBOX_STATE_TONE,
  SANDBOX_PROFILE_LABEL_KEYS,
} from "./i18n.ts";
import { useSandboxInstancesT, SandboxInstancesLocaleProvider } from "./locale.tsx";
import { ConfirmModal, SurfaceDrawer } from "./SurfaceOverlay.tsx";
import { sandboxTotalItems, isSandboxDeletable } from "./sandbox-instances-model.ts";

/** Compatible with IAM session-auth boundary attachment (dual-token clients). */
type AttachSdkClientBoundaries = (
  clients: readonly { http?: unknown }[],
) => readonly { http?: unknown }[];

/**
 * VM Instances on the app-console surface.
 *
 * `sandbox_instance` is the per-user half of sdkwork-sandbox: the virtual
 * machine (VM) runtimes one account provisioned. The collection is served by
 * that module's own app-api face on this edge's origin
 * (`/app/v3/api/sandbox/sandbox_instances`), and the tenant and owner are taken
 * from the verified principal — nothing here
 * sends either, so this page cannot ask for another account's rows even by
 * accident. It is mounted on the console only; the tenant-wide inventory needs a
 * different authorization argument and is not this page's job.
 *
 * Styles are scoped by `.sandbox-console-surface`.
 */
export interface SandboxInstancesConsoleSurfaceProps {
  /** Same-origin app-api base; the sandbox routes are served under it. */
  appApiBaseUrl: string;
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  locale?: string | null;
  resource: "sandbox-instances";
  tokenManager: AuthTokenManager;
}

export function SandboxInstancesConsoleSurface({
  appApiBaseUrl,
  attachSdkClientBoundaries,
  locale,
  tokenManager,
}: SandboxInstancesConsoleSurfaceProps) {
  const client = useMemo(() => {
    const next = createSandboxAppClient(appApiBaseUrl, tokenManager);
    // Dual-token only — never project x-sdkwork-tenant-id (API_SPEC §10.2).
    // Registering the client with the session boundary is what makes a 401 from
    // this plane clear the session like a 401 from any generated client.
    attachSdkClientBoundaries?.([next]);
    return next;
  }, [appApiBaseUrl, attachSdkClientBoundaries, tokenManager]);
  const localeKey = locale?.trim() || "en-US";
  return (
    <div className="sandbox-console-surface" lang={localeKey}>
      <SandboxInstancesLocaleProvider key={localeKey} locale={locale}>
        <SandboxInstancesPage client={client} />
      </SandboxInstancesLocaleProvider>
    </div>
  );
}

/** Page size choices. The server clamps to 1..200. */
const SANDBOX_INSTANCE_PAGE_SIZES = [20, 50, 100] as const;

/** Cap on badges drawn in the capabilities cell before the overflow pill. */
const SANDBOX_CAPABILITY_BADGE_LIMIT = 3;

function messageOf(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * Expiry as a local timestamp.
 *
 * Rendered in the browser's locale and zone because that is the clock the
 * operator booked the expiry against; the wire value stays UTC. An unparsable
 * value shows the raw string rather than "Invalid Date", so a server-side format
 * change is visible instead of disguised.
 */
function formatSandboxExpiry(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function SandboxCapabilityBadges({ capabilities }: { capabilities: readonly (keyof typeof SANDBOX_CAPABILITY_LABEL_KEYS)[] }) {
  const t = useSandboxInstancesT();
  if (capabilities.length === 0) {
    return <span className="sandbox-badge sandbox-badge--default">{t("capability.none")}</span>;
  }
  const shown = capabilities.slice(0, SANDBOX_CAPABILITY_BADGE_LIMIT);
  const overflow = capabilities.length - shown.length;
  return (
    <span className="sandbox-badges">
      {shown.map((capability) => (
        <span className="sandbox-badge" key={capability}>{t(SANDBOX_CAPABILITY_LABEL_KEYS[capability])}</span>
      ))}
      {overflow > 0 ? (
        <span className="sandbox-badge">{t("capability.overflow", { count: overflow })}</span>
      ) : null}
    </span>
  );
}

/**
 * A failed read and a failed command are different facts, and they belong to
 * different surfaces: the read owns the table body — a listing that could not be
 * read is not an empty listing — while a command owns the banner above it. One
 * `string | null` cannot express that, and collapsing the two is how a refused
 * read ends up painted as "you have no VM instances".
 */
type SandboxConsoleError = { kind: "action" | "read"; message: string };

function SandboxInstancesPage({ client }: { client: SandboxAppClient }) {
  const t = useSandboxInstancesT();
  const [rows, setRows] = useState<readonly SandboxInstance[]>([]);
  const [totalItems, setTotalItems] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<number>(SANDBOX_INSTANCE_PAGE_SIZES[0]);
  const [stateFilter, setStateFilter] = useState<SandboxInstanceState | "">("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<SandboxConsoleError | null>(null);
  const [drawer, setDrawer] = useState<{ kind: "create" } | { kind: "edit"; instance: SandboxInstance } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<SandboxInstance | null>(null);
  // Bumped after every mutation. Reloading through a token instead of calling a
  // loader directly means a mutation that also changes `page` (provisioning jumps
  // back to page 1) still results in exactly one request, and there is no second
  // code path that can forget the active filter.
  const [refreshToken, setRefreshToken] = useState(0);

  // Guards a slow response from an abandoned page/filter pair overwriting a newer
  // one: the request that is no longer current resolves after the one that is.
  const requestSeq = useRef(0);

  const refresh = useCallback(() => setRefreshToken((token) => token + 1), []);

  useEffect(() => {
    const seq = requestSeq.current + 1;
    requestSeq.current = seq;
    let cancelled = false;
    setLoading(true);
    client
      .list({
        page,
        pageSize,
        ...(stateFilter ? { sandboxInstanceState: stateFilter } : {}),
      })
      .then((result) => {
        if (cancelled || requestSeq.current !== seq) return;
        setRows([...result.items]);
        setTotalItems(sandboxTotalItems(result.pageInfo));
        setError(null);
      })
      .catch((cause: unknown) => {
        if (cancelled || requestSeq.current !== seq) return;
        setRows([]);
        setTotalItems(0);
        // Bare cause: `error.load` is the frame's own title, so prefixing it here
        // would print the same sentence twice.
        setError({ kind: "read", message: messageOf(cause) });
      })
      .finally(() => {
        if (cancelled || requestSeq.current !== seq) return;
        setLoading(false);
      });
    return () => { cancelled = true; };
    // `t` is not a dependency: the provider remounts the page on a locale change
    // (`key={localeKey}`), so nothing inside this effect reads a localized string.
  }, [client, page, pageSize, refreshToken, stateFilter]);

  const columns = useMemo<DataTableColumn<SandboxInstance>[]>(() => [
    {
      id: "name",
      header: t("column.name"),
      cell: (instance) => (
        <span className="sandbox-name-cell">
          <strong>{instance.sandboxInstanceName}</strong>
          <small title={instance.sandboxInstanceId}>{instance.sandboxInstanceId}</small>
        </span>
      ),
    },
    {
      id: "state",
      header: t("column.state"),
      cell: (instance) => (
        <span className={`status-badge ${SANDBOX_STATE_TONE[instance.sandboxInstanceState]}`.trim()}>
          {t(SANDBOX_STATE_LABEL_KEYS[instance.sandboxInstanceState])}
        </span>
      ),
    },
    {
      id: "profile",
      header: t("column.profile"),
      cell: (instance) => t(SANDBOX_PROFILE_LABEL_KEYS[instance.sandboxInstanceProfile]),
    },
    {
      id: "resources",
      header: t("column.resources"),
      cell: (instance) => (
        <span className="sandbox-resources">
          {t("resources.summary", {
            disk: instance.sandboxInstanceDiskMb,
            memory: instance.sandboxInstanceMemoryMb,
            vcpu: instance.sandboxInstanceVcpuCount,
          })}
        </span>
      ),
    },
    {
      id: "capabilities",
      header: t("column.capabilities"),
      cell: (instance) => (
        <SandboxCapabilityBadges capabilities={instance.sandboxInstanceRequiredCapabilities} />
      ),
    },
    {
      id: "expiresAt",
      header: t("column.expiresAt"),
      cell: (instance) => (
        <span className="sandbox-resources">
          {instance.sandboxInstanceExpiresAt
            ? formatSandboxExpiry(instance.sandboxInstanceExpiresAt)
            : t("expiresAt.never")}
        </span>
      ),
    },
  ], [t]);

  const filterActive = stateFilter !== "";
  /** A read failure belongs to the table body; a command failure to the banner. */
  const readCause = error?.kind === "read" ? error.message : null;
  const actionError = error?.kind === "action" ? error.message : null;

  async function submitDraft(submit: SandboxProvisionFormSubmit) {
    const editingInstance = drawer?.kind === "edit" ? drawer.instance : null;
    setBusy(true);
    setError(null);
    try {
      if (submit.kind === "create") {
        await client.create(submit.input);
        // A new row lands on page 1 of the server's ordering; staying on page 3
        // would look like the provisioning did nothing.
        setPage(1);
      } else if (editingInstance) {
        await client.update(editingInstance.sandboxInstanceId, submit.input);
      }
      setDrawer(null);
      refresh();
    } catch (cause) {
      setError({
        kind: "action",
        message: `${submit.kind === "create" ? t("error.create") : t("error.update")} ${messageOf(cause)}`,
      });
    } finally {
      setBusy(false);
    }
  }

  async function confirmDelete() {
    const target = deleteTarget;
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      await client.remove(target.sandboxInstanceId);
      setDeleteTarget(null);
      // Deleting the last row of a page would otherwise leave the table showing an
      // empty page that the server no longer has.
      if (rows.length === 1 && page > 1) {
        setPage(page - 1);
      } else {
        refresh();
      }
    } catch (cause) {
      setError({ kind: "action", message: `${t("error.delete")} ${messageOf(cause)}` });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="skills-console-page">
      <header className="skills-console-header">
        <div>
          <h2>{t("title")}</h2>
          <p>{t("description")}</p>
        </div>
        <div className="skills-console-header-actions">
          <select
            aria-label={t("filter.state")}
            className="sandbox-state-filter"
            disabled={loading || busy}
            onChange={(event) => {
              setStateFilter(event.target.value as SandboxInstanceState | "");
              setPage(1);
            }}
            value={stateFilter}
          >
            <option value="">{t("filter.allStates")}</option>
            {Object.keys(SANDBOX_STATE_LABEL_KEYS).map((state) => (
              <option key={state} value={state}>
                {t(SANDBOX_STATE_LABEL_KEYS[state as SandboxInstanceState])}
              </option>
            ))}
          </select>
          <button
            className="sdkwork-surface-modal-cancel"
            disabled={loading || busy}
            onClick={refresh}
            type="button"
          >
            {t("refresh")}
          </button>
          <button
            className="skills-console-primary"
            disabled={busy}
            onClick={() => setDrawer({ kind: "create" })}
            type="button"
          >
            {t("create")}
          </button>
        </div>
      </header>
      <p className="skills-console-status">{t("owner.scope")}</p>
      {actionError ? (
        <p className="skills-console-error" role="alert">{actionError}</p>
      ) : null}
      <div className="data-surface">
        <DataTable<SandboxInstance>
          columns={columns}
          density="compact"
          emptyState={readCause !== null ? (
            // An unreadable listing is not an empty listing. `DataTable` paints
            // whatever `emptyState` it is handed as soon as it has no rows, so a
            // refused read would otherwise be rendered as the reassuring "you have
            // none yet" frame — the one reading that hides an expired session or
            // a missing permission behind what looks like a clean, empty account.
            // The frame is the alert here because the raw cause is the only thing
            // that tells an operator 401 from 403 from 500.
            <div className="empty-state" role="alert">
              <h3>{t("error.load")}</h3>
              <p>{t("unavailable.description")}</p>
              <p className="sandbox-error-cause">{readCause}</p>
              <button className="skills-console-primary" onClick={refresh} type="button">
                {t("refresh")}
              </button>
            </div>
          ) : filterActive ? (
            <div className="empty-state">
              <h3>{t("filter.empty.title")}</h3>
              <p>{t("filter.empty.description")}</p>
              <button
                className="skills-console-primary"
                onClick={() => { setStateFilter(""); setPage(1); }}
                type="button"
              >
                {t("filter.clear")}
              </button>
            </div>
          ) : (
            <div className="empty-state">
              <h3>{t("empty.title")}</h3>
              <p>{t("empty.description")}</p>
              <button
                className="skills-console-primary"
                onClick={() => setDrawer({ kind: "create" })}
                type="button"
              >
                {t("empty.action")}
              </button>
            </div>
          )}
          getRowId={(instance) => instance.sandboxInstanceId}
          loading={loading}
          loadingLabel={t("loading")}
          // Server pagination: the collection is unbounded and the listing is
          // already paged on the wire, so reading every row into memory to page it
          // client-side would be wrong at exactly the size where it matters.
          pagination={{
            mode: "server",
            onPageChange: setPage,
            onPageSizeChange: (next) => { setPageSize(next); setPage(1); },
            page,
            pageSize,
            pageSizeOptions: SANDBOX_INSTANCE_PAGE_SIZES,
            rowCount: totalItems,
          }}
          rowActions={(instance) => {
            // A live instance is not deletable (the service answers
            // `InstanceNotDeletable`), so the action is offered as unavailable
            // with the reason rather than as a button that always fails.
            const deletable = isSandboxDeletable(instance.sandboxInstanceState);
            return (
              <div className="skills-console-actions">
                <button
                  disabled={busy}
                  onClick={() => setDrawer({ kind: "edit", instance })}
                  type="button"
                >
                  {t("action.edit")}
                </button>
                <button
                  disabled={busy || !deletable}
                  onClick={() => setDeleteTarget(instance)}
                  title={deletable ? undefined : t("delete.blockedHint")}
                  type="button"
                >
                  {t("action.delete")}
                </button>
              </div>
            );
          }}
          rowActionsLabel={t("column.actions")}
          rows={[...rows]}
          stickyHeader
        />
      </div>

      <SurfaceDrawer
        description={drawer?.kind === "edit"
          ? t("drawer.editDescription", { name: drawer.instance.sandboxInstanceName })
          : t("drawer.createDescription")}
        onClose={() => { if (!busy) setDrawer(null); }}
        open={drawer !== null}
        title={drawer?.kind === "edit" ? t("drawer.editTitle") : t("drawer.createTitle")}
      >
        {drawer ? (
          <SandboxProvisionForm
            busy={busy}
            instance={drawer.kind === "edit" ? drawer.instance : null}
            onCancel={() => { if (!busy) setDrawer(null); }}
            onSubmit={(submit) => { void submitDraft(submit); }}
          />
        ) : null}
      </SurfaceDrawer>

      <ConfirmModal
        busy={busy}
        cancelLabel={t("dialog.cancel")}
        confirmLabel={t("action.delete")}
        description={t("delete.confirmDescription", {
          name: deleteTarget?.sandboxInstanceName ?? "",
        })}
        onCancel={() => { if (!busy) setDeleteTarget(null); }}
        onConfirm={() => { void confirmDelete(); }}
        open={deleteTarget !== null}
        title={t("delete.confirmTitle")}
      />
    </section>
  );
}
