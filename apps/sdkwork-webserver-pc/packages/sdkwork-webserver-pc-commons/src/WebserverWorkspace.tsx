import {
  Activity,
  BadgeCheck,
  Check,
  ChevronLeft,
  ChevronRight,
  Clipboard,
  Columns3,
  Filter,
  Inbox,
  Link,
  LoaderCircle,
  LockKeyhole,
  Pause,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Rocket,
  RotateCcw,
  Search,
  Settings2,
  Shield,
  Trash2,
  Unlink,
  Upload,
  X,
} from "lucide-react";
import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { Navigate, Route, Routes, useLocation } from "react-router-dom";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { uuid } from "@sdkwork/utils/id";

import { translateWebserver, type WebserverLocale, type WebserverMessageKey } from "./i18n/index.ts";
import { formatWebserverErrorMessage } from "./error-message.ts";
import {
  canAccessWebserverResource,
  hasPlatformSuperAdminAccess,
  hasWebserverPermission,
  hasWebserverSuperAdminAccess,
} from "./permissions.ts";
import type {
  WebserverPageInfo,
  WebserverPcModuleDefinition,
  WebserverResourceAction,
  WebserverResourceDataSource,
  WebserverResourceFieldOptionPage,
  WebserverResourceFieldOptionValue,
  WebserverResourceFieldOptions,
  WebserverResourceKey,
  WebserverResourceRegistry,
} from "./types.ts";
import { WorkspaceHeader, WorkspaceSidebar, type WorkspaceModuleTab } from "./WebserverWorkspaceChrome.tsx";
import {
  DEFAULT_ADMIN_MODULE_ID,
  adminEntryPathSegment,
  groupAdminModuleEntries,
  resolveAdminModuleDefinition,
  resolveAdminModuleFromPath,
  resolveAdminModuleLandingPath,
} from "./admin-modules.ts";

/**
 * Page-size choices offered by the resource table footer.
 *
 * These are request sizes the resource APIs are expected to accept, not a
 * storage detail: raising the default raises the first-paint page load, so the
 * options start at the historical 20 and only go up.
 */
const DEFAULT_RESOURCE_PAGE_SIZE = 20;
const RESOURCE_PAGE_SIZE_OPTIONS = [20, 50, 100] as const;

export interface WebserverWorkspaceProps {
  locale: WebserverLocale;
  modules: readonly WebserverPcModuleDefinition[];
  notificationsHref?: string;
  onSignOut?(): void;
  permissionScope: readonly string[];
  portalHref?: string;
  /**
   * Registry-driven resources. Optional because a surface may render every one
   * of its entries through `resourceRenderers` (host-bridged pages from another
   * module); an entry with neither a renderer nor a registry source renders its
   * own empty state.
   */
  registry?: WebserverResourceRegistry;
  resourceRenderers?: Partial<Record<WebserverResourceKey, ReactNode>>;
  surface: "app-console" | "backend-admin";
  userLabel?: string;
}

const FIELD_OPTION_PAGE_SIZE = 20;

export function WebserverWorkspace({
  locale,
  modules,
  notificationsHref,
  onSignOut,
  permissionScope,
  portalHref,
  registry = {},
  resourceRenderers,
  surface,
  userLabel,
}: WebserverWorkspaceProps) {
  const { pathname } = useLocation();
  const t = (key: WebserverMessageKey, values?: Record<string, string | number>) =>
    translateWebserver(locale, key, values);
  const entries = useMemo(() => {
    const availableEntries = modules.flatMap((module) => module.entries);
    return availableEntries
      .filter((entry) =>
        surface === "app-console"
        || hasWebserverPermission(permissionScope, entry.permission),
      )
      .sort((a, b) => a.order - b.order);
  }, [modules, permissionScope, surface]);
  const basePath = surface === "backend-admin" ? "/admin" : "/console";
  // Modules group the same flat resource routes into header tabs; the sidebar
  // then lists only the active module's entries. A surface that resolves to a
  // single module (the console) renders no tab bar at all.
  const moduleGroups = useMemo(
    () => groupAdminModuleEntries(basePath, entries),
    [basePath, entries],
  );
  const visibleModuleGroups = moduleGroups.filter((group) => group.entries.length > 0);
  const moduleTabs: readonly WorkspaceModuleTab[] = visibleModuleGroups.length > 1
    ? visibleModuleGroups.map((group) => {
        const module = resolveAdminModuleDefinition(group.moduleId);
        return {
          id: module.id,
          href: resolveAdminModuleLandingPath(basePath, group.entries) ?? basePath,
          label: t(module.labelKey),
          description: t(module.descriptionKey),
        };
      })
    : [];
  const activeModuleId = surface === "backend-admin"
    ? resolveAdminModuleFromPath(pathname)
    : DEFAULT_ADMIN_MODULE_ID;
  const activeModuleEntries = moduleGroups.find((group) => group.moduleId === activeModuleId)?.entries
    ?? [];
  // An operator routed to a module they cannot see lands on the first module
  // that actually has entries, so the fallback can never self-redirect.
  const landingPath = resolveAdminModuleLandingPath(
    basePath,
    activeModuleEntries.length > 0 ? activeModuleEntries : (visibleModuleGroups[0]?.entries ?? []),
  );
  const hasVisibleEntries = visibleModuleGroups.length > 0;
  const adminRole = surface === "backend-admin"
    ? hasPlatformSuperAdminAccess(permissionScope)
      ? t("auth.platformSuperAdmin")
      : hasWebserverSuperAdminAccess(permissionScope)
        ? t("auth.webSuperAdmin")
        : t("auth.webAdministrator")
    : undefined;

  return (
    <div className="app-layout">
      <WorkspaceHeader
        adminRole={adminRole}
        basePath={basePath}
        moduleTabs={moduleTabs}
        notificationsHref={notificationsHref}
        onSignOut={onSignOut}
        portalHref={portalHref}
        surface={surface}
        t={t}
        userLabel={userLabel}
      />
      <WorkspaceSidebar
        basePath={basePath}
        entries={activeModuleEntries}
        surface={surface}
        t={t}
      />
      <main className="workspace">
        {hasVisibleEntries ? (
          <Routes>
            {/* Every visible entry keeps a route regardless of the active
                module, so cross-module deep links stay resolvable. */}
            {entries.map((entry) => (
              <Route
                key={entry.resource}
                path={`/${adminEntryPathSegment(entry)}/*`}
                element={resourceRenderers?.[entry.resource] ?? (
                  <ResourcePage
                    entry={entry}
                    locale={locale}
                    permissionScope={permissionScope}
                    source={registry[entry.resource]}
                    surface={surface}
                  />
                )}
              />
            ))}
            <Route
              path="*"
              element={<Navigate to={landingPath ?? basePath} replace />}
            />
          </Routes>
        ) : (
          <SurfaceAccessState locale={locale} />
        )}
      </main>
    </div>
  );
}

function SurfaceAccessState({ locale }: { locale: WebserverLocale }) {
  const t = (key: WebserverMessageKey, values: Record<string, string | number> = {}) => (
    translateWebserver(locale, key, values)
  );
  return (
    <section className="surface-access-state" role="alert">
      <Shield aria-hidden="true" size={22} />
      <h1>{t("access.title")}</h1>
      <p>{t("access.description")}</p>
    </section>
  );
}

function ResourcePage({
  entry,
  locale,
  permissionScope,
  source,
  surface,
}: {
  entry: { permission: string; resource: WebserverResourceKey };
  locale: WebserverLocale;
  permissionScope: readonly string[];
  source?: WebserverResourceDataSource;
  surface: "app-console" | "backend-admin";
}) {
  const t = (key: WebserverMessageKey, values?: Record<string, string | number>) =>
    translateWebserver(locale, key, values);
  const authorized = canAccessWebserverResource(surface, permissionScope, entry.permission);
  // Scope-bound pages were an `applications`-era concept: the selector picked
  // the application (`siteId`) that a configuration, source-version, or
  // deployment page then read. No resource on either surface declares
  // `requiresScope` any more, so a page carries no scope state.
  const [items, setItems] = useState<readonly Record<string, unknown>[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<number>(DEFAULT_RESOURCE_PAGE_SIZE);
  const [pageInfo, setPageInfo] = useState<WebserverPageInfo>({ page: 1, pageSize: DEFAULT_RESOURCE_PAGE_SIZE, hasMore: false, mode: "offset" });
  const [nextCursor, setNextCursor] = useState<string | undefined>(undefined);
  /** Cursor that loaded the currently displayed page (its start token). */
  const [currentCursor, setCurrentCursor] = useState<string | undefined>(undefined);
  /** Start tokens of pages behind the current one, for cursor-mode back navigation. */
  const [cursorHistory, setCursorHistory] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [filters, setFilters] = useState<Record<string, string>>({});
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [selected, setSelected] = useState<Record<string, unknown>>();
  /**
   * Operator column choices for this resource, keyed by column id. Only the
   * columns the operator actually toggled are present; everything else follows
   * the column plan's default visibility.
   */
  const [columnOverrides, setColumnOverrides] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [action, setAction] = useState<WebserverResourceAction>();
  const visibleActions = useMemo(
    () => source?.actions.filter((candidate) =>
      !candidate.permission
      || hasWebserverPermission(permissionScope, candidate.permission),
    ) ?? [],
    [permissionScope, source],
  );
  /** Resets both pagination modes to their first page. */
  function resetPagination(): void {
    setPage(1);
    setNextCursor(undefined);
    setCursorHistory([]);
    setCurrentCursor(undefined);
  }

  async function load(filterValues: Readonly<Record<string, string>> = filters, cursorOverride?: string | null): Promise<void> {
    if (!authorized || !source) {
      setItems([]);
      return;
    }
    // `null` forces the first page even when the state still holds a stale
    // cursor; `undefined` continues from the current cursor/page state.
    const cursor = cursorOverride === null ? undefined : (cursorOverride === undefined ? nextCursor : cursorOverride);
    setCurrentCursor(cursor);
    setBusy(true);
    setError(undefined);
    try {
      const result = await source.load({
        cursor,
        filters: source.filters?.length ? filterValues : undefined,
        page,
        pageSize,
        search: search.trim() || undefined,
      });
      setItems(result.items);
      setPageInfo(result.pageInfo);
      setNextCursor(result.pageInfo.nextCursor);
    } catch (caught) {
      setError(formatWebserverErrorMessage(caught, t));
    } finally {
      setBusy(false);
    }
  }

  /** Advances to the next page: cursor pages push the current page start
   *  onto the back-stack; offset pages increment `page`. */
  function goToNextPage(): void {
    if (busy) return;
    if (pageInfo.mode === "cursor") {
      const next = pageInfo.nextCursor;
      if (!next) return;
      setCursorHistory((history) => [...history, currentCursor ?? ""]);
      setCurrentCursor(next);
      setNextCursor(next);
      void load(undefined, next);
    } else if (pageInfo.hasMore) {
      setPage((value) => value + 1);
    }
  }

  /** Returns to the previous page: cursor pages pop the back-stack;
   *  offset pages decrement `page`. */
  function goToPreviousPage(): void {
    if (busy) return;
    if (pageInfo.mode === "cursor") {
      const history = [...cursorHistory];
      const previous = history.pop();
      if (previous === undefined) return;
      setCursorHistory(history);
      const previousCursor = previous === "" ? undefined : previous;
      setCurrentCursor(previousCursor);
      setNextCursor(previousCursor);
      // `null` forces the first page explicitly: the state closure may still
      // hold the stale forward cursor when `previousCursor` is undefined.
      void load(undefined, previousCursor === undefined ? null : previousCursor);
    } else {
      setPage((value) => Math.max(1, value - 1));
    }
  }

  /**
   * Absolute jump used by the table's numbered page list.
   *
   * Offset pages translate a page number into a request directly. Cursor pages
   * cannot: a keyset backend only walks forwards from an opaque token, so a
   * jump to an arbitrary page is not expressible at all. Cursor mode therefore
   * honours only the one step the framework can express — forward to the next
   * page — and ignores the rest, rather than requesting a page whose token it
   * does not have.
   */
  function goToPage(nextPage: number): void {
    if (busy) return;
    if (pageInfo.mode === "cursor") {
      if (nextPage > page) goToNextPage();
      else if (nextPage < page) goToPreviousPage();
      return;
    }
    if (nextPage < 1 || nextPage === page) return;
    setPage(nextPage);
  }

  /** Rows-per-page selection. Changes the window size, so it restarts at page 1. */
  function goToPageSize(nextPageSize: number): void {
    if (busy || nextPageSize === pageSize) return;
    setPageSize(nextPageSize);
    resetPagination();
  }

  useEffect(() => {
    void load();
  }, [authorized, entry.resource, page, pageSize]);

  useEffect(() => {
    resetPagination();
    setSelected(undefined);
    setColumnOverrides(readColumnPreferences(entry.resource));
  }, [entry.resource]);

  const columns = useMemo(
    () => resourceColumns(entry.resource, items),
    [entry.resource, items],
  );
  /**
   * Columns the collapsed row renders.
   *
   * A plan that moves fields into the record's detail row leaves them out of the
   * row — and out of the column menu with it, since re-adding a detail field as
   * a column is the very "widen the table until it curls" shape the detail row
   * replaces.
   */
  const rowColumns = useMemo(() => columns.filter((column) => !column.detailOnly), [columns]);
  /**
   * Whether this resource shows a record's remaining fields in its own row.
   *
   * Derived from the plan rather than declared separately: a resource is
   * expandable exactly when it has fields the row does not carry.
   */
  const expandable = rowColumns.length < columns.length;
  /**
   * Operations the expanded record can run on itself.
   *
   * Selection-scoped actions only: one that needs no row (create) belongs to the
   * toolbar, and one whose `availableWhen` rejects this record's posture must
   * not be offered next to that record — a "Restore routing" button on an
   * instance already in rotation is an invitation to a failed request.
   */
  const rowScopedActions = (item: Record<string, unknown>) =>
    visibleActions.filter(
      (candidate) => candidate.requiresSelection && actionAvailable(candidate, item),
    );
  /**
   * Effective column visibility. The persisted operator choice wins, so hiding a
   * column is sticky; every column the operator never touched shows, because the
   * plan is what says which fields a row carries and it already leaves out the
   * ones a row cannot hold.
   */
  const columnVisibility = useMemo(() => {
    const visibility: Record<string, boolean> = {};
    for (const column of rowColumns) {
      visibility[column.id] = columnOverrides[column.id] ?? true;
    }
    return visibility;
  }, [rowColumns, columnOverrides]);
  const tableColumns = useMemo<DataTableColumn<Record<string, unknown>>[]>(
    () => rowColumns.map((column) => ({
      id: column.id,
      header: resourceFieldLabel(entry.resource, column.id, locale),
      cell: (item: Record<string, unknown>) => displayValue(column.read(item), column.id, entry.resource, locale),
    })),
    [rowColumns, entry.resource, locale],
  );
  /**
   * Scroll envelope: the sum of the planned column widths.
   *
   * `table-layout: auto` under `width: 100%` shrinks columns to whatever the
   * viewport allows instead of overflowing it, which is how a twelve-column
   * fleet table turns into twelve clipped columns. Giving the table a width
   * floor is what makes the viewport scroll instead.
   */
  const tableMinWidth = useMemo(
    () => rowColumns.reduce((total, column) => total + (column.width ?? 0), 0),
    [rowColumns],
  );
  const resourceLabel = resourceText(t, entry.resource, "label");

  function toggleColumn(id: string, visible: boolean): void {
    setColumnOverrides((current) => {
      const next = { ...current, [id]: visible };
      writeColumnPreferences(entry.resource, next);
      return next;
    });
  }

  return (
    <section aria-label={resourceLabel} className="resource-page">
      <div className="resource-commandbar">
        <div className="resource-identity">
          <h1>{resourceLabel}</h1>
        </div>
        {authorized ? (
          <>
            <div className="resource-query">
              <form
                className="search-box"
                onSubmit={(event) => {
                  event.preventDefault();
                  resetPagination();
                  void load(undefined, null);
                }}
                role="search"
              >
                <Search aria-hidden="true" size={16} />
                <input
                  aria-label={t("toolbar.search")}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder={t("toolbar.search")}
                  value={search}
                />
              </form>
              {source?.filters?.length ? (
                <button
                  aria-expanded={filtersOpen}
                  className="secondary-button"
                  onClick={() => setFiltersOpen((value) => !value)}
                  type="button"
                >
                  <Filter aria-hidden="true" size={16} />
                  {t("toolbar.filters")}
                  {activeFilterCount(filters) > 0 ? <span className="filter-count">{activeFilterCount(filters)}</span> : null}
                </button>
              ) : null}
            </div>
            <div className="actions">
              {visibleActions.map((candidate) => (
                <button
                  className={candidate.dangerous
                    ? "danger-button"
                    : candidate.requiresSelection
                      ? "secondary-button"
                      : "command-button"}
                  disabled={busy
                    || (candidate.requiresSelection && !selected)
                    || !actionAvailable(candidate, selected)}
                  key={candidate.id}
                  onClick={() => setAction(candidate)}
                  type="button"
                >
                  <ActionIcon action={candidate} />
                  {actionText(t, entry.resource, candidate)}
                </button>
              ))}
              <button
                aria-label={t("toolbar.refresh")}
                className="icon-button refresh-button"
                disabled={busy}
                onClick={() => void load()}
                title={t("toolbar.refresh")}
                type="button"
              >
                <RefreshCw aria-hidden="true" className={busy ? "is-spinning" : undefined} size={17} />
              </button>
              {/*
                Column menu. Fleet tables carry more columns than any viewport
                shows at once, so the operator - not the page author - decides
                which ones are on screen; the choice is persisted per resource.
                A native `details` disclosure keeps it keyboard- and
                screen-reader-navigable without a popover dependency.

                It lists the row's columns only. A field the plan moved into the
                record's detail row is not a candidate here: the detail row is
                where it is read, at full length and without costing every other
                row a column.
              */}
              {rowColumns.length > 0 ? (
                <details className="column-menu">
                  <summary className="secondary-button">
                    <Columns3 aria-hidden="true" size={16} />
                    {t("toolbar.columns")}
                  </summary>
                  <div aria-label={t("toolbar.columns")} className="column-menu-list" role="group">
                    {rowColumns.map((column) => (
                      <label key={column.id}>
                        <input
                          checked={columnVisibility[column.id] !== false}
                          onChange={(event) => toggleColumn(column.id, event.target.checked)}
                          type="checkbox"
                        />
                        <span>{resourceFieldLabel(entry.resource, column.id, locale)}</span>
                      </label>
                    ))}
                  </div>
                </details>
              ) : null}
            </div>
          </>
        ) : null}
      </div>

      {!authorized ? (
        <div className="resource-access-state" role="status">
          <LockKeyhole aria-hidden="true" size={22} />
          <div>
            <strong>{t("access.resource.title")}</strong>
            <p>{t("access.resource.description")}</p>
          </div>
        </div>
      ) : (
        <>
          {filtersOpen && source?.filters?.length ? (
            <form
              className="filter-bar"
              onSubmit={(event) => {
                event.preventDefault();
                setPage(1);
                void load();
              }}
            >
              {source.filters.map((filter) => (
                <label key={filter.id}>
                  <span>{fieldLabel(filter.id, locale)}</span>
                  {filter.type === "select" ? (
                    <select
                      onChange={(event) => setFilters((current) => ({ ...current, [filter.id]: event.target.value }))}
                      value={filters[filter.id] ?? ""}
                    >
                      <option value="">{t("filters.all")}</option>
                      {filter.fieldOptions?.map((option) => (
                        <option key={String(optionValue(option))} value={String(optionValue(option))}>
                          {optionLabel(option, filter.id, locale)}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      onChange={(event) => setFilters((current) => ({ ...current, [filter.id]: event.target.value }))}
                      type={filter.type}
                      value={filters[filter.id] ?? ""}
                    />
                  )}
                </label>
              ))}
              <div className="filter-actions">
                <button
                  className="secondary-button"
                  disabled={activeFilterCount(filters) === 0}
                  onClick={() => {
                    setFilters({});
                    resetPagination();
                    void load({}, null);
                  }}
                  type="button"
                >
                  {t("filters.reset")}
                </button>
                <button className="command-button" type="submit">{t("filters.apply")}</button>
              </div>
            </form>
          ) : null}
          {error ? (
            <div className="error-banner" role="alert">
              {error}
              <button
                aria-label={t("toolbar.dismiss")}
                className="icon-button"
                onClick={() => setError(undefined)}
                title={t("toolbar.dismiss")}
                type="button"
              >
                <X aria-hidden="true" size={16} />
              </button>
            </div>
          ) : null}
          <div className="data-surface">
            {/* The data surface is the framework's `DataTable` rather than a
                hand-rolled `<table>`: it owns the surface chrome, compact row
                density, sticky header, row selection, and the whole pagination
                footer in one place.

                Pagination stays server-driven. The page's cursor/offset
                machinery is the only thing that knows how this resource
                paginates, so `mode: "server"` forbids the table from slicing
                rows it never fetched. `hasMore` is what makes cursor pages
                work: they publish no total, so the table renders a page ordinal
                and drives Next from the backend's own flag instead of
                inventing a page count. Offset pages additionally pass `rowCount`
                and get the numbered page list. */}
            <DataTable<Record<string, unknown>>
              columnVisibility={columnVisibility}
              columns={tableColumns}
              density="compact"
              emptyState={busy ? (
                <div className="empty-state" role="status">
                  <LoaderCircle aria-hidden="true" className="is-spinning" size={20} />
                  <span>{t("table.loading")}</span>
                </div>
              ) : (
                <div className="empty-state">
                  <Inbox aria-hidden="true" size={20} />
                  <span>{t("table.empty")}</span>
                </div>
              )}
              getRowId={(item, index) => recordKey(item, index)}
              getRowExpandLabel={expandable
                ? (item, index, expanded) => t(expanded ? "table.collapseRow" : "table.expandRow", {
                  name: rowLabel(item, index),
                })
                : undefined}
              loading={busy && items.length === 0}
              // An expandable row's click is the disclosure, so it cannot also
              // mean "select": the checkbox stays the selection control.
              onRowClick={expandable ? undefined : (item) => setSelected(item)}
              onSelectedRowIdsChange={(ids) => {
                const [nextId] = ids;
                setSelected(nextId === undefined
                  ? undefined
                  : items.find((item, index) => recordKey(item, index) === String(nextId)));
              }}
              onSortingChange={undefined}
              pagination={{
                hasMore: pageInfo.hasMore,
                mode: "server",
                onPageChange: goToPage,
                onPageSizeChange: goToPageSize,
                page,
                pageSize: pageInfo.pageSize,
                pageSizeOptions: RESOURCE_PAGE_SIZE_OPTIONS,
                rowCount: pageInfo.total,
              }}
              renderExpandedRow={expandable
                ? (item) => (
                  <ResourceDetail
                    actions={rowScopedActions(item)}
                    item={item}
                    locale={locale}
                    onAction={(candidate, row) => {
                      setSelected(row);
                      setAction(candidate);
                    }}
                    resource={entry.resource}
                    t={t}
                  />
                )
                : undefined}
              rowActionsLabel={t("table.actions")}
              rowDetailLabel={t("table.rowDetail")}
              rows={items as Record<string, unknown>[]}
              selectable
              selectedRowIds={selected === undefined ? [] : [recordKey(selected, items.indexOf(selected))]}
              slotProps={tableMinWidth > 0
                ? { table: { style: { minWidth: `${tableMinWidth}px` } } }
                : undefined}
              sortingMode="server"
              stickyHeader
            />
          </div>
          {action ? (
            <ActionDialog
              action={action}
              label={resolveActionLabel(t, entry.resource, action, selected)}
              locale={locale}
              onClose={() => setAction(undefined)}
              onComplete={() => {
                setAction(undefined);
                void load();
              }}
              onRefresh={() => void load()}
              selected={selected}
            />
          ) : null}
        </>
      )}
    </section>
  );
}

function ActionIcon({ action }: { action: WebserverResourceAction }) {
  const iconProps = { "aria-hidden": true, size: 15 } as const;
  if (action.id.includes("rollback")) return <RotateCcw {...iconProps} />;
  if (action.id.includes("delete")) return <Trash2 {...iconProps} />;
  if (action.id.includes("pause")) return <Pause {...iconProps} />;
  if (action.id.includes("activate")) return <Play {...iconProps} />;
  if (action.id.includes("verify")) return <BadgeCheck {...iconProps} />;
  if (action.id === "bind") return <Link {...iconProps} />;
  if (action.id === "unbind") return <Unlink {...iconProps} />;
  if (action.id.includes("certificate")) return <Shield {...iconProps} />;
  if (action.id.includes("deploy") || action.id.includes("publish")) return <Rocket {...iconProps} />;
  if (action.id.includes("reload") || action.id.includes("renew")) return <RefreshCw {...iconProps} />;
  if (action.id.includes("update")) return <Pencil {...iconProps} />;
  if (action.id.includes("create")) return <Plus {...iconProps} />;
  if (action.id.includes("diagnostic")) return <Activity {...iconProps} />;
  return <Settings2 {...iconProps} />;
}

function ActionDialog({
  action,
  label,
  locale,
  onClose,
  onComplete,
  onRefresh,
  selected,
}: {
  action: WebserverResourceAction;
  label: string;
  locale: WebserverLocale;
  onClose(): void;
  onComplete(): void;
  onRefresh(): void;
  selected?: Record<string, unknown>;
}) {
  const t = (key: WebserverMessageKey, values: Record<string, string | number> = {}) => (
    translateWebserver(locale, key, values)
  );
  const [body, setBody] = useState<Record<string, unknown>>(() => initialActionBody(action, selected));
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [file, setFile] = useState<File>();
  const [fieldOptions, setFieldOptions] = useState<WebserverResourceFieldOptions>(action.fieldOptions ?? {});
  const [idempotencyKey] = useState(() => uuid());
  const [optionsBusy, setOptionsBusy] = useState(Boolean(action.loadFieldOptions));
  const [progress, setProgress] = useState(0);
  const [result, setResult] = useState<Record<string, unknown>>();
  const [copiedField, setCopiedField] = useState<string>();
  const abortControllerRef = useRef<AbortController | undefined>(undefined);
  const dialogRef = useRef<HTMLFormElement>(null);
  const submitInFlightRef = useRef(false);
  const confirmationRequired = Boolean(action.dangerous || action.requiresConfirmation);

  function closeDialog(): void {
    if (busy && !action.dismissibleWhileBusy) return;
    if (busy) {
      abortControllerRef.current?.abort();
      onRefresh();
    }
    onClose();
  }

  const renderField = (name: string, value: unknown, required = false) => {
    const commonProps = {
      locale,
      name,
      onChange: (next: unknown, relatedValues?: Readonly<Record<string, number | string>>) => setBody((current) => ({
        ...current,
        [name]: next,
        ...relatedValues,
      })),
      readOnly: action.readOnlyFields?.includes(name),
      required,
      value,
    };
    if (action.loadFieldOptionPage && action.paginatedFields?.includes(name)) {
      return (
        <PaginatedField
          {...commonProps}
          actionBody={body}
          key={name}
          loadPage={action.loadFieldOptionPage}
          maximumSelections={action.fieldSelectionLimits?.[name]}
          multiple={action.multipleFields?.includes(name)}
          selectedItem={selected}
        />
      );
    }
    return (
      <Field
        {...commonProps}
        key={name}
        multiple={action.multipleFields?.includes(name)}
        options={fieldOptions[name]}
      />
    );
  };

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    const initialFocus = dialogRef.current?.querySelector<HTMLElement>(
      ".form-grid input:not([disabled]), .form-grid select:not([disabled]), .form-grid textarea:not([disabled]), .confirm-check input:not([disabled]), button[type='submit']:not([disabled])",
    );
    initialFocus?.focus();
    // Mark the invoker so restore survives workspace re-renders that replace
    // the captured node (the list may reload while the dialog is open).
    previousFocus?.setAttribute("data-sdkwork-dialog-return", "");
    return () => queueMicrotask(() => {
      const marked = document.querySelector<HTMLElement>("[data-sdkwork-dialog-return]");
      const target = marked ?? (previousFocus?.isConnected ? previousFocus : undefined);
      target?.focus();
      marked?.removeAttribute("data-sdkwork-dialog-return");
      previousFocus?.removeAttribute("data-sdkwork-dialog-return");
    });
  }, []);

  useEffect(() => {
    document.body.classList.add("dialog-open");
    return () => document.body.classList.remove("dialog-open");
  }, []);

  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || (busy && !action.dismissibleWhileBusy)) return;
      event.preventDefault();
      closeDialog();
    };
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [action.dismissibleWhileBusy, busy, onClose, onRefresh]);

  useEffect(() => () => abortControllerRef.current?.abort(), []);

  useEffect(() => {
    if (!action.loadFieldOptions) return undefined;
    let active = true;
    setOptionsBusy(true);
    setError(undefined);
    void action.loadFieldOptions({ body: initialActionBody(action, selected), selectedItem: selected })
      .then((loadedOptions) => {
        if (!active) return;
        const mergedOptions = { ...action.fieldOptions, ...loadedOptions };
        setFieldOptions(mergedOptions);
        setBody((current) => {
          const next = { ...current };
          for (const [name, options] of Object.entries(mergedOptions)) {
            if ((next[name] === "" || next[name] === undefined) && options.length > 0) {
              const firstOption = options[0];
              next[name] = optionValue(firstOption);
              Object.assign(next, optionRelatedValues(firstOption));
            }
          }
          return next;
        });
      })
      .catch((caught) => {
        if (active) setError(formatWebserverErrorMessage(caught, t, { fallbackKey: "error.options" }));
      })
      .finally(() => {
        if (active) setOptionsBusy(false);
      });
    return () => {
      active = false;
    };
  }, [action, selected]);

  async function executeAction(): Promise<void> {
    if (submitInFlightRef.current) return;
    if (
      (confirmationRequired && !confirmed)
      || (action.requiresFile && !file)
    ) return;
    submitInFlightRef.current = true;
    const abortController = new AbortController();
    abortControllerRef.current = abortController;
    setBusy(true);
    setError(undefined);
    setProgress(0);
    try {
      const response = await action.execute({
        body,
        file,
        idempotencyKey,
        onProgress: (value) => setProgress(Math.max(0, Math.min(100, Math.round(value)))),
        selectedItem: selected,
        signal: abortController.signal,
      });
      if (action.resultFields?.length && isRecord(response)) {
        setResult(response);
        onRefresh();
        return;
      }
      onComplete();
    } catch (caught) {
      setError(formatWebserverErrorMessage(caught, t));
    } finally {
      submitInFlightRef.current = false;
      if (abortControllerRef.current === abortController) abortControllerRef.current = undefined;
      setBusy(false);
    }
  }

  async function submit(event: FormEvent): Promise<void> {
    event.preventDefault();
    await executeAction();
  }

  return (
    <div
      className="dialog-backdrop"
      onKeyDown={(event) => trapDialogFocus(event, dialogRef.current ?? event.currentTarget)}
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) closeDialog();
      }}
      role="presentation"
    >
      <form
        aria-labelledby="action-title"
        aria-modal="true"
        className="dialog"
        onSubmit={(event) => void submit(event)}
        ref={dialogRef}
        role="dialog"
      >
        <header>
          <div className="dialog-title-group">
            <h2 id="action-title">{label}</h2>
          </div>
          <button
            aria-label={t("dialog.close")}
            className="icon-button"
            disabled={busy && !action.dismissibleWhileBusy}
            onClick={closeDialog}
            title={t("dialog.close")}
            type="button"
          >
            <X aria-hidden="true" size={18} />
          </button>
        </header>
        {result ? (
          <div className="dialog-scroll">
            <div className="operation-result" role="status">
              <div className="result-notice"><Check aria-hidden="true" size={18} />{t("dialog.operationComplete")}</div>
              {"agentToken" in result ? <div className="warning">{t("dialog.oneTimeCredential")}</div> : null}
              <dl>
                {action.resultFields?.map((field) => field in result ? (
                  <div key={field}>
                    <dt>{fieldLabel(field, locale)}</dt>
                    <dd>
                      <code>{String(result[field] ?? "-")}</code>
                      <button
                        aria-label={t("dialog.copyField")}
                        className="icon-button"
                        onClick={() => {
                          void navigator.clipboard.writeText(String(result[field] ?? ""));
                          setCopiedField(field);
                        }}
                        title={t("dialog.copyField")}
                        type="button"
                      >
                        {copiedField === field ? <Check aria-hidden="true" size={16} /> : <Clipboard aria-hidden="true" size={16} />}
                      </button>
                    </dd>
                  </div>
                ) : null)}
              </dl>
            </div>
          </div>
        ) : null}
        {!result ? (
          <div className="dialog-scroll">
            {confirmationRequired ? <div className="warning">{t("dialog.warning")}</div> : null}
            <div className="form-grid">
              {Object.entries(body).map(([name, value]) => renderField(
                name,
                value,
                action.requiredFields?.includes(name),
              ))}
            </div>
            {action.requiresFile ? (
              <label className="file-field">
                <span><Upload aria-hidden="true" size={16} />{t("dialog.file")}</span>
                <input
                  accept={action.acceptedFileTypes}
                  disabled={busy}
                  onChange={(event) => setFile(event.target.files?.[0])}
                  type="file"
                />
              </label>
            ) : null}
            {busy && action.requiresFile ? (
              <div className="upload-progress" role="status">
                <div>
                  <span>{t("dialog.uploadProgress")}</span>
                  <strong>{progress}%</strong>
                </div>
                <progress aria-label={t("dialog.uploadProgress")} max={100} value={progress} />
              </div>
            ) : null}
            {confirmationRequired ? (
              <label className="confirm-check">
                <input
                  checked={confirmed}
                  onChange={(event) => setConfirmed(event.target.checked)}
                  type="checkbox"
                />
                {t("dialog.confirmRisk")}
              </label>
            ) : null}
            {error ? <div className="error-banner" role="alert">{error}</div> : null}
          </div>
        ) : null}
        {result ? (
          <footer>
            <button className="command-button" onClick={closeDialog} type="button">{t("dialog.close")}</button>
          </footer>
        ) : <footer>
          <button className="secondary-button" disabled={busy && !action.dismissibleWhileBusy} onClick={closeDialog} type="button">{t("dialog.cancel")}</button>
          <button
            className={action.dangerous ? "danger-button" : "command-button"}
            disabled={busy
              || optionsBusy
              || Boolean(confirmationRequired && !confirmed)
              || Boolean(action.requiresFile && !file)
              || hasMissingRequiredFields(body, action.requiredFields)
              || hasUnavailableOptions(body, fieldOptions, action.paginatedFields)}
            type="submit"
          >
            {busy ? <><LoaderCircle aria-hidden="true" className="is-spinning" size={16} />{t("dialog.submitting")}</> : t("dialog.confirm")}
          </button>
        </footer>}
      </form>
    </div>
  );
}

function trapDialogFocus(event: ReactKeyboardEvent<HTMLElement>, dialog: HTMLElement): void {
  if (event.key !== "Tab") return;
  const focusable = Array.from(dialog.querySelectorAll<HTMLElement>(
    "button:not([disabled]), input:not([disabled]):not([tabindex='-1']), select:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex='-1'])",
  )).filter((element) => element.getAttribute("aria-hidden") !== "true");
  if (focusable.length === 0) return;
  // Sequential navigation (not only boundary wrapping): jsdom and embedded
  // webviews do not implement native Tab focus movement, so the trap itself
  // moves focus to the next/previous focusable and wraps at the ends. This
  // keeps keyboard order deterministic in every environment.
  const currentIndex = focusable.indexOf(document.activeElement as HTMLElement);
  event.preventDefault();
  if (event.shiftKey) {
    (currentIndex <= 0 ? focusable[focusable.length - 1] : focusable[currentIndex - 1]).focus();
  } else if (currentIndex === -1 || currentIndex === focusable.length - 1) {
    focusable[0].focus();
  } else {
    focusable[currentIndex + 1].focus();
  }
}

function PaginatedField({
  actionBody,
  loadPage,
  locale,
  maximumSelections,
  multiple = false,
  name,
  onChange,
  readOnly = false,
  required = false,
  selectedItem,
  value,
}: {
  actionBody: Record<string, unknown>;
  loadPage: NonNullable<WebserverResourceAction["loadFieldOptionPage"]>;
  locale: WebserverLocale;
  maximumSelections?: number;
  multiple?: boolean;
  name: string;
  onChange(value: unknown, relatedValues?: Readonly<Record<string, number | string>>): void;
  readOnly?: boolean;
  required?: boolean;
  selectedItem?: Record<string, unknown>;
  value: unknown;
}) {
  const t = (key: WebserverMessageKey, values: Record<string, string | number> = {}) => (
    translateWebserver(locale, key, values)
  );
  const inputId = useId();
  const [page, setPage] = useState(1);
  const [requestVersion, setRequestVersion] = useState(0);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string>();
  const [optionPage, setOptionPage] = useState<WebserverResourceFieldOptionPage>({
    options: [],
    pageInfo: { hasMore: false, page: 1, pageSize: FIELD_OPTION_PAGE_SIZE },
  });
  const [selectedOptions, setSelectedOptions] = useState<Readonly<Record<string, WebserverResourceFieldOptionValue>>>({});
  const contextRef = useRef({ actionBody, selectedItem });
  contextRef.current = { actionBody, selectedItem };
  const selectedValues = useMemo(() => [...new Set(
    (Array.isArray(value) ? value : value === undefined || value === null || value === "" ? [] : [value])
      .map((item) => String(item)),
  )], [value]);
  const selectedValueKey = JSON.stringify(selectedValues);

  useEffect(() => {
    const abortController = new AbortController();
    let active = true;
    setBusy(true);
    setError(undefined);
    setOptionPage((current) => ({
      options: [],
      pageInfo: { ...current.pageInfo, hasMore: false, page, pageSize: FIELD_OPTION_PAGE_SIZE },
    }));
    const context = contextRef.current;
    void loadPage(name, {
      body: context.actionBody,
      page,
      pageSize: FIELD_OPTION_PAGE_SIZE,
      selectedItem: context.selectedItem,
      signal: abortController.signal,
    }).then((loaded) => {
      if (!active) return;
      setOptionPage({
        options: loaded.options,
        pageInfo: {
          ...loaded.pageInfo,
          page: loaded.pageInfo.page > 0 ? loaded.pageInfo.page : page,
          pageSize: loaded.pageInfo.pageSize > 0 ? loaded.pageInfo.pageSize : FIELD_OPTION_PAGE_SIZE,
        },
      });
    }).catch((caught) => {
      if (!active || abortController.signal.aborted) return;
      setError(formatWebserverErrorMessage(caught, t, { fallbackKey: "error.options" }));
    }).finally(() => {
      if (active) setBusy(false);
    });
    return () => {
      active = false;
      abortController.abort();
    };
  }, [loadPage, name, page, requestVersion, selectedItem]);

  useEffect(() => {
    const selectedSet = new Set(selectedValues);
    setSelectedOptions((current) => {
      const next: Record<string, WebserverResourceFieldOptionValue> = {};
      for (const selectedValue of selectedValues) {
        if (current[selectedValue] !== undefined) next[selectedValue] = current[selectedValue];
      }
      for (const option of optionPage.options) {
        const key = String(optionValue(option));
        if (selectedSet.has(key)) next[key] = option;
      }
      return next;
    });
  }, [optionPage.options, selectedValueKey]);

  const displayedOptions = useMemo(() => {
    const currentValues = new Set(optionPage.options.map((option) => String(optionValue(option))));
    return [
      ...selectedValues.flatMap((selectedValue) => {
        const option = selectedOptions[selectedValue];
        return option !== undefined && !currentValues.has(selectedValue) ? [option] : [];
      }),
      ...optionPage.options,
    ];
  }, [optionPage.options, selectedOptions, selectedValueKey]);
  const selectedSet = new Set(selectedValues);
  const selectionFull = maximumSelections !== undefined && selectedValues.length >= maximumSelections;
  const stateMessage = busy
    ? t("dialog.optionsLoading")
    : error
      ? error
      : optionPage.options.length === 0
        ? t("dialog.optionsEmpty")
        : t("dialog.optionsPage", { page: optionPage.pageInfo.page });

  return (
    <div className={`paginated-option-field${multiple ? " paginated-option-field-multiple" : ""}`} data-field={name}>
      <label className="field-label-row" htmlFor={inputId}>
        <span>{fieldLabel(name, locale)}</span>
        {required ? <small aria-hidden="true" className="field-required" data-label={locale === "zh-CN" ? "必填" : "Required"} /> : null}
      </label>
      {multiple ? (
        <div aria-label={fieldLabel(name, locale)} className="paginated-option-list" role="group">
          {displayedOptions.length === 0 ? (
            <div className="paginated-option-empty">-</div>
          ) : displayedOptions.map((option) => {
            const optionKey = String(optionValue(option));
            const selected = selectedSet.has(optionKey);
            const optionDisabled = (selectionFull && !selected) || readOnly;
            return (
              <label
                className={`paginated-option-item${selected ? " selected" : ""}${optionDisabled ? " disabled" : ""}`}
                key={optionKey}
              >
                <input
                  aria-label={optionLabel(option, name, locale)}
                  checked={selected}
                  disabled={optionDisabled}
                  onChange={() => {
                    const nextValues = selected
                      ? selectedValues.filter((value) => value !== optionKey)
                      : [...selectedValues, optionKey];
                    if (maximumSelections !== undefined && nextValues.length > maximumSelections) return;
                    const optionLookup = new Map(displayedOptions.map((candidate) => [String(optionValue(candidate)), candidate]));
                    setSelectedOptions(Object.fromEntries(nextValues.flatMap((nextValue) => {
                      const candidate = optionLookup.get(nextValue);
                      return candidate === undefined ? [] : [[nextValue, candidate]];
                    })));
                    onChange(nextValues);
                  }}
                  type="checkbox"
                  value={optionKey}
                />
                <span className="paginated-option-label" title={optionLabel(option, name, locale)}>
                  {optionLabel(option, name, locale)}
                </span>
              </label>
            );
          })}
        </div>
      ) : (
        <select
          aria-label={fieldLabel(name, locale)}
          aria-required={required}
          disabled={readOnly || (displayedOptions.length === 0 && (busy || Boolean(error) || optionPage.options.length === 0))}
          id={inputId}
          onChange={(event) => {
            const selectedOption = displayedOptions.find((option) => String(optionValue(option)) === event.target.value)
              ?? event.target.value;
            onChange(optionValue(selectedOption), optionRelatedValues(selectedOption));
          }}
          value={selectedValues[0] ?? ""}
        >
          {displayedOptions.length === 0 ? <option value="">-</option> : null}
          {displayedOptions.map((option) => {
            const optionKey = String(optionValue(option));
            return (
              <option key={optionKey} value={optionKey}>
                {optionLabel(option, name, locale)}
              </option>
            );
          })}
        </select>
      )}
      <div className="paginated-option-footer">
        <span aria-live="polite" className={error ? "paginated-option-error" : undefined} role={error ? "alert" : "status"}>
          {busy ? <LoaderCircle aria-hidden="true" className="is-spinning" size={14} /> : null}
          {stateMessage}
        </span>
        {maximumSelections === undefined ? null : (
          <span>{t("dialog.optionsSelected", { count: selectedValues.length, limit: maximumSelections })}</span>
        )}
        <div className="paginated-option-controls">
          {error ? (
            <button
              aria-label={t("dialog.optionsRetry")}
              className="icon-button"
              onClick={() => setRequestVersion((current) => current + 1)}
              title={t("dialog.optionsRetry")}
              type="button"
            >
              <RefreshCw aria-hidden="true" size={16} />
            </button>
          ) : null}
          <button
            aria-label={t("pagination.previous")}
            className="icon-button"
            disabled={busy || page <= 1}
            onClick={() => setPage((current) => Math.max(1, current - 1))}
            title={t("pagination.previous")}
            type="button"
          >
            <ChevronLeft aria-hidden="true" size={16} />
          </button>
          <button
            aria-label={t("pagination.next")}
            className="icon-button"
            disabled={busy || !optionPage.pageInfo.hasMore}
            onClick={() => setPage((current) => current + 1)}
            title={t("pagination.next")}
            type="button"
          >
            <ChevronRight aria-hidden="true" size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}

function Field({
  locale,
  multiple = false,
  name,
  onChange,
  options,
  readOnly = false,
  required = false,
  value,
}: {
  locale: WebserverLocale;
  multiple?: boolean;
  name: string;
  onChange(value: unknown, relatedValues?: Readonly<Record<string, number | string>>): void;
  options?: readonly WebserverResourceFieldOptionValue[];
  readOnly?: boolean;
  required?: boolean;
  value: unknown;
}) {
  if (typeof value === "boolean") {
    return (
      <label className="checkbox-field" data-field={name}>
        <input aria-label={fieldLabel(name, locale)} checked={value} disabled={readOnly} onChange={(event) => onChange(event.target.checked)} type="checkbox" />
        <span>{fieldLabel(name, locale)}</span>
      </label>
    );
  }
  if (options) {
    const selectedValues = Array.isArray(value)
      ? value.map((item) => String(item))
      : [String(value ?? "")];
    return (
      <label data-field={name}>
        <span className="field-label-row">
          {fieldLabel(name, locale)}
          {required ? <small aria-hidden="true" className="field-required" data-label={locale === "zh-CN" ? "必填" : "Required"} /> : null}
        </span>
        <select
          aria-label={fieldLabel(name, locale)}
          aria-required={required}
          onChange={(event) => {
            if (multiple) {
              onChange(Array.from(event.target.selectedOptions).map((selected) => {
                const option = options.find((candidate) => String(optionValue(candidate)) === selected.value)
                  ?? selected.value;
                return optionValue(option);
              }));
              return;
            }
            const selectedOption = options.find((option) => String(optionValue(option)) === event.target.value)
              ?? event.target.value;
            onChange(optionValue(selectedOption), optionRelatedValues(selectedOption));
          }}
          disabled={readOnly || options.length === 0}
          multiple={multiple}
          size={multiple ? Math.min(6, Math.max(3, options.length)) : undefined}
          value={multiple ? selectedValues : selectedValues[0]}
        >
          {options.length === 0 ? <option value="">-</option> : null}
          {options.map((option) => (
            <option key={String(optionValue(option))} value={String(optionValue(option))}>
              {optionLabel(option, name, locale)}
            </option>
          ))}
        </select>
      </label>
    );
  }
  if (typeof value === "number") {
    return (
      <label data-field={name}>
        <span className="field-label-row">
          {fieldLabel(name, locale)}
          {required ? <small aria-hidden="true" className="field-required" data-label={locale === "zh-CN" ? "必填" : "Required"} /> : null}
        </span>
        <input aria-label={fieldLabel(name, locale)} aria-required={required} onChange={(event) => onChange(Number(event.target.value))} readOnly={readOnly} type="number" value={value} />
      </label>
    );
  }
  const text = String(value ?? "");
  const characterLimit = textFieldCharacterLimit(name);
  const multiline = name === "description"
    || name === "fullDescription"
    || name === "releaseNotes"
    || name.toLowerCase().includes("content");
  const wide = multiline || name === "shortDescription";
  const updateText = (next: string) => onChange(
    characterLimit === undefined
      ? next
      : Array.from(next).slice(0, characterLimit).join(""),
  );
  return (
    <label className={wide ? "form-field-wide" : undefined} data-field={name}>
      <span className="field-label-row">
        {fieldLabel(name, locale)}
        {required ? <small aria-hidden="true" className="field-required" data-label={locale === "zh-CN" ? "必填" : "Required"} /> : null}
      </span>
      {multiline ? (
        <textarea
          aria-label={fieldLabel(name, locale)}
          aria-required={required}
          onChange={(event) => updateText(event.target.value)}
          readOnly={readOnly}
          rows={name === "description" ? 2 : 4}
          value={text}
        />
      ) : (
        <input
          aria-label={fieldLabel(name, locale)}
          aria-required={required}
          autoComplete="off"
          onChange={(event) => updateText(event.target.value)}
          readOnly={readOnly}
          type={name.toLowerCase().endsWith("url") ? "url" : sensitive(name) ? "password" : "text"}
          value={text}
        />
      )}
      {characterLimit === undefined ? null : (
        <small className="field-character-limit">
          {Array.from(text).length} / {characterLimit}
        </small>
      )}
    </label>
  );
}

function textFieldCharacterLimit(name: string): number | undefined {
  switch (name) {
    case "shortDescription":
    case "category":
      return 80;
    case "fullDescription":
    case "releaseNotes":
      return 4_000;
    default:
      return undefined;
  }
}

function resourceText(
  t: (key: WebserverMessageKey) => string,
  resource: WebserverResourceKey,
  field: "label" | "description",
  fallback?: string,
): string {
  const key = `resource.${resource}.${field}` as WebserverMessageKey;
  const translated = t(key);
  if (translated && translated !== key) {
    return translated;
  }
  const trimmed = fallback?.trim();
  if (trimmed) {
    return trimmed;
  }
  return field === "label" ? resource : "";
}

function resolveActionLabel(
  t: (key: WebserverMessageKey) => string,
  resource: WebserverResourceKey,
  action: WebserverResourceAction,
  selectedItem?: Record<string, unknown>,
): string {
  const resolvedKey = action.resolveActionLabelKey?.({ selectedItem })
    ?? (`action.${resource}.${action.id}` as WebserverMessageKey);
  return t(resolvedKey as WebserverMessageKey);
}

function actionText(
  t: (key: WebserverMessageKey) => string,
  resource: WebserverResourceKey,
  action: WebserverResourceAction,
): string {
  return resolveActionLabel(t, resource, action);
}

function recordKey(item: Record<string, unknown>, index: number): string {
  return String(
    item.id
    ?? item.siteId
    ?? item.domainId
    ?? item.certificateId
    ?? item.deploymentId
    ?? item.configId
    ?? item.serverId
    ?? item.auditLogId
    ?? index,
  );
}

/**
 * Cell renderer for registry-driven tables.
 *
 * Cluster surfaces get their own branch first: their wire values are machine
 * codes (`1`, `HEALTHY`, `IN_SYNC`), raw counters (uptime seconds) and ISO
 * instants, and rendering them verbatim is what made a fleet table read like a
 * JSON dump instead of an operations console.
 */
function displayValue(value: unknown, column: string, resource: WebserverResourceKey, locale: WebserverLocale): ReactNode {
  if (value === null || value === undefined || value === "") return "-";
  if (resource === "servers" && column === "status") {
    return <span className={`status-badge server-status-${String(value).toLowerCase()}`}>{serverStatus(value, locale)}</span>;
  }
  if (resource.startsWith("cluster-")) {
    const clusterCell = clusterCellValue(value, column, resource, locale);
    if (clusterCell !== undefined) return clusterCell;
  }
  const codedLabel = codedValueLabel(column, value, locale);
  if (codedLabel) return codedLabel;
  if (typeof value === "boolean") return booleanLabel(value, locale);
  if (column.toLowerCase().includes("status")) {
    return <span className={`status-badge status-${String(value).toLowerCase()}`}>{String(value)}</span>;
  }
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

interface ClusterCellTone {
  label: Record<WebserverLocale, string>;
  tone: "ok" | "warn" | "danger" | "neutral";
}

const CLUSTER_POSITIVE: ClusterCellTone["tone"] = "ok";
const CLUSTER_WARNING: ClusterCellTone["tone"] = "warn";
const CLUSTER_DANGER: ClusterCellTone["tone"] = "danger";
const CLUSTER_NEUTRAL: ClusterCellTone["tone"] = "neutral";

/**
 * Machine code to badge mapping for the cluster surfaces, keyed
 * `resource:column`.
 *
 * The same column name carries different code spaces per resource (an instance
 * `status` is 0..5, a host `status` is 0..4, a cluster `status` is 0..1), so the
 * mapping is keyed by both rather than by the column alone — one shared table
 * would label a maintenance instance as "Offline".
 */
const CLUSTER_CELL_TONES: Record<string, Record<string, ClusterCellTone>> = {
  "cluster-instances:status": {
    "0": { label: { "en-US": "Offline", "zh-CN": "离线" }, tone: CLUSTER_DANGER },
    "1": { label: { "en-US": "Online", "zh-CN": "在线" }, tone: CLUSTER_POSITIVE },
    "2": { label: { "en-US": "Starting", "zh-CN": "启动中" }, tone: CLUSTER_WARNING },
    "3": { label: { "en-US": "Stopping", "zh-CN": "停止中" }, tone: CLUSTER_WARNING },
    "4": { label: { "en-US": "Error", "zh-CN": "异常" }, tone: CLUSTER_DANGER },
    "5": { label: { "en-US": "Maintenance", "zh-CN": "维护中" }, tone: CLUSTER_NEUTRAL },
  },
  "cluster-hosts:status": {
    "0": { label: { "en-US": "Offline", "zh-CN": "离线" }, tone: CLUSTER_DANGER },
    "1": { label: { "en-US": "Online", "zh-CN": "在线" }, tone: CLUSTER_POSITIVE },
    "2": { label: { "en-US": "Deploying", "zh-CN": "部署中" }, tone: CLUSTER_WARNING },
    "3": { label: { "en-US": "Error", "zh-CN": "异常" }, tone: CLUSTER_DANGER },
    "4": { label: { "en-US": "Maintenance", "zh-CN": "维护中" }, tone: CLUSTER_NEUTRAL },
  },
  "cluster-clusters:status": {
    "0": { label: { "en-US": "Inactive", "zh-CN": "未启用" }, tone: CLUSTER_NEUTRAL },
    "1": { label: { "en-US": "Active", "zh-CN": "已启用" }, tone: CLUSTER_POSITIVE },
  },
  "cluster-instances:healthState": {
    HEALTHY: { label: { "en-US": "Healthy", "zh-CN": "健康" }, tone: CLUSTER_POSITIVE },
    DEGRADED: { label: { "en-US": "Degraded", "zh-CN": "降级" }, tone: CLUSTER_WARNING },
    UNHEALTHY: { label: { "en-US": "Unhealthy", "zh-CN": "不健康" }, tone: CLUSTER_DANGER },
    UNKNOWN: { label: { "en-US": "Unknown", "zh-CN": "未知" }, tone: CLUSTER_NEUTRAL },
  },
  "cluster-instances:routingState": {
    ROUTING: { label: { "en-US": "In rotation", "zh-CN": "承载流量" }, tone: CLUSTER_POSITIVE },
    CORDONED: { label: { "en-US": "Cordoned", "zh-CN": "已隔离" }, tone: CLUSTER_WARNING },
    DRAINING: { label: { "en-US": "Draining", "zh-CN": "排水迁移中" }, tone: CLUSTER_WARNING },
    EJECTED: { label: { "en-US": "Ejected", "zh-CN": "已摘除" }, tone: CLUSTER_DANGER },
  },
  "cluster-instances:syncStatus": {
    UNKNOWN: { label: { "en-US": "Unknown", "zh-CN": "未知" }, tone: CLUSTER_NEUTRAL },
    IN_SYNC: { label: { "en-US": "In sync", "zh-CN": "已同步" }, tone: CLUSTER_POSITIVE },
    PENDING: { label: { "en-US": "Pending", "zh-CN": "待同步" }, tone: CLUSTER_WARNING },
    FAILED: { label: { "en-US": "Failed", "zh-CN": "同步失败" }, tone: CLUSTER_DANGER },
  },
  "cluster-instances:joinMode": {
    LAN: { label: { "en-US": "LAN", "zh-CN": "局域网" }, tone: CLUSTER_NEUTRAL },
    TUNNEL: { label: { "en-US": "Tunnel", "zh-CN": "隧道" }, tone: CLUSTER_NEUTRAL },
  },
  "cluster-hosts:joinMode": {
    LAN: { label: { "en-US": "LAN", "zh-CN": "局域网" }, tone: CLUSTER_NEUTRAL },
    TUNNEL: { label: { "en-US": "Tunnel", "zh-CN": "隧道" }, tone: CLUSTER_NEUTRAL },
  },
  "cluster-instances:role": {
    GATEWAY: { label: { "en-US": "Gateway", "zh-CN": "网关" }, tone: CLUSTER_NEUTRAL },
    MANAGEMENT: { label: { "en-US": "Management", "zh-CN": "管理面" }, tone: CLUSTER_NEUTRAL },
    DATA_PLANE: { label: { "en-US": "Data plane", "zh-CN": "数据面" }, tone: CLUSTER_NEUTRAL },
    WORKER: { label: { "en-US": "Worker", "zh-CN": "工作节点" }, tone: CLUSTER_NEUTRAL },
    OTHER: { label: { "en-US": "Other", "zh-CN": "其他" }, tone: CLUSTER_NEUTRAL },
  },
  "cluster-clusters:lbStrategy": {
    round_robin: { label: { "en-US": "Round robin", "zh-CN": "轮询" }, tone: CLUSTER_NEUTRAL },
    weighted_round_robin: { label: { "en-US": "Weighted round robin", "zh-CN": "加权轮询" }, tone: CLUSTER_NEUTRAL },
    least_connections: { label: { "en-US": "Least connections", "zh-CN": "最小连接数" }, tone: CLUSTER_NEUTRAL },
    random: { label: { "en-US": "Random", "zh-CN": "随机" }, tone: CLUSTER_NEUTRAL },
    random_two_choices: { label: { "en-US": "Random two choices", "zh-CN": "随机二选一" }, tone: CLUSTER_NEUTRAL },
    ip_hash: { label: { "en-US": "IP hash", "zh-CN": "IP 哈希" }, tone: CLUSTER_NEUTRAL },
    consistent_hash: { label: { "en-US": "Consistent hash", "zh-CN": "一致性哈希" }, tone: CLUSTER_NEUTRAL },
  },
  "cluster-events:severity": {
    INFO: { label: { "en-US": "Info", "zh-CN": "提示" }, tone: CLUSTER_NEUTRAL },
    WARNING: { label: { "en-US": "Warning", "zh-CN": "警告" }, tone: CLUSTER_WARNING },
    ERROR: { label: { "en-US": "Error", "zh-CN": "错误" }, tone: CLUSTER_DANGER },
  },
};

/**
 * Cluster-surface cell rendering, limited to the columns it owns; anything it
 * does not recognise returns `undefined` so the generic renderer still runs.
 */
function clusterCellValue(
  value: unknown,
  column: string,
  resource: WebserverResourceKey,
  locale: WebserverLocale,
): ReactNode | undefined {
  const tone = CLUSTER_CELL_TONES[`${resource}:${column}`]?.[String(value)];
  if (tone) {
    return <span className={`status-badge cluster-tone-${tone.tone}`}>{tone.label[locale]}</span>;
  }
  if (column === "uptime") {
    return formatUptime(value, locale);
  }
  if (clusterInstantColumns.has(column)) {
    return formatInstantValue(value, locale);
  }
  return undefined;
}

/** Cluster columns whose wire value is an RFC 3339 instant. */
const clusterInstantColumns = new Set([
  "lastHeartbeatAt",
  "lastOnlineAt",
  "processStartedAt",
  "occurredAt",
  "createdAt",
  "updatedAt",
]);

/**
 * Uptime as a coarse duration (`12d 4h`, `45m`).
 *
 * Micro-precision is noise on a fleet page, and a raw second count is
 * unreadable: what the operator asks of this column is "how long has this slot
 * been up", not the exact second.
 */
function formatUptime(value: unknown, locale: WebserverLocale): string {
  const seconds = Number(value);
  if (!Number.isFinite(seconds) || seconds < 0) return "-";
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const units = locale === "zh-CN"
    ? { day: "天", hour: "小时", minute: "分", second: "秒" }
    : { day: "d", hour: "h", minute: "m", second: "s" };
  if (days > 0) return `${days}${units.day} ${hours}${units.hour}`;
  if (hours > 0) return `${hours}${units.hour} ${minutes}${units.minute}`;
  if (minutes > 0) return `${minutes}${units.minute}`;
  return `${Math.floor(seconds)}${units.second}`;
}

/** RFC 3339 instant in the operator's locale; unparseable values pass through unchanged. */
function formatInstantValue(value: unknown, locale: WebserverLocale): string {
  const parsed = new Date(String(value));
  if (Number.isNaN(parsed.getTime())) return String(value);
  return parsed.toLocaleString(locale === "zh-CN" ? "zh-CN" : "en-US", { hour12: false });
}

function humanize(value: string): string {
  return value.replace(/([a-z])([A-Z])/g, "$1 $2").replaceAll("_", " ");
}

/**
 * Column labels that only hold on one resource.
 *
 * `fieldLabel` is keyed on the field name alone, so `name` is "Application
 * name" - correct where it was first needed, wrong everywhere the same wire
 * field means something else. A cluster instance called an application is a
 * header that misdescribes the column, so the resource wins when it has its own
 * wording.
 */
const RESOURCE_COLUMN_LABELS: Partial<
  Record<WebserverResourceKey, Record<string, Record<WebserverLocale, string>>>
> = {
  "cluster-instances": {
    name: { "en-US": "Instance", "zh-CN": "实例" },
    hostName: { "en-US": "Host", "zh-CN": "宿主" },
  },
  "cluster-clusters": {
    name: { "en-US": "Cluster", "zh-CN": "集群" },
  },
  // `detail` is generic enough that it means something different on every
  // resource, so its wording belongs to the resource rather than the field.
  "cluster-events": {
    detail: { "en-US": "Event payload", "zh-CN": "事件载荷" },
  },
};

function resourceFieldLabel(
  resource: WebserverResourceKey,
  column: string,
  locale: WebserverLocale,
): string {
  return RESOURCE_COLUMN_LABELS[resource]?.[column]?.[locale] ?? fieldLabel(column, locale);
}

function fieldLabel(value: string, locale: WebserverLocale): string {
  const labels: Record<WebserverLocale, Partial<Record<string, string>>> = {
    "en-US": {
      action: "Action",
      agentToken: "Node credential",
      applicationId: "Application",
      applicationName: "Application",
      applicationType: "Application type",
      hasSourceVersion: "Source code",
      appConfigPath: "Application manifest path",
      artifactDriveUri: "Package",
      artifactHash: "Package hash",
      artifactSize: "Package size",
      checkInterval: "Check interval (seconds)",
      checkType: "Check type",
      checkUrl: "Check URL",
      commitHash: "Commit hash",
      completedAt: "Completed at",
      configContent: "Configuration",
      configName: "Configuration name",
      configType: "Configuration type",
      createdAt: "Created at",
      deployedAt: "Deployed at",
      deployType: "Deployment method",
      deploymentConfigPath: "Deployment config path",
      description: "Description",
      shortDescription: "Short description",
      fullDescription: "Full description",
      releaseNotes: "Release notes",
      category: "Category",
      keywords: "Keywords",
      supportUrl: "Support URL",
      privacyPolicyUrl: "Privacy policy URL",
      publicRoot: "Public root",
      officialWebsiteUrl: "Official website URL",
      durationMs: "Duration",
      environment: "Environment",
      endDate: "End date",
      host: "Host",
      id: "ID",
      isActive: "Active",
      isSecret: "Secret value",
      key: "Variable name",
      name: "Application name",
      operatorId: "Operator ID",
      operatorType: "Operator type",
      checkedAt: "Last checked at",
      expiresAt: "Expires at",
      retryCount: "Retry count",
      rollbackFromDeploymentId: "Restored from",
      siteType: "Runtime type",
      sourceType: "Source type",
      sourceVersionId: "Source version",
      sourceVersionRetentionLimit: "Versions retained",
      sourceRef: "Source ref",
      spaFallback: "SPA fallback",
      sshPort: "SSH port",
      startDate: "Start date",
      startedAt: "Started at",
      status: "Status",
      retained: "Retained",
      configSnapshot: "Standard configuration",
      timeoutMs: "Timeout (ms)",
      targetType: "Target type",
      targetUuid: "Target ID",
      tenantScopeHash: "Tenant scope hash",
      updatedAt: "Updated at",
      value: "Variable value",
      versionNo: "Version",
      versionTag: "Version",
      desiredSyncVersion: "Desired version",
      appliedSyncVersion: "Applied version",
      ipAddress: "IP address",
      lastHeartbeatAt: "Last heartbeat",
      // Cluster inventory columns (cluster-clusters / cluster-hosts /
      // cluster-instances / cluster-events). Composite ids (`bindAddress`,
      // `capacity`, `platform`, `routingState`) are resolved by the column plan
      // rather than by a single wire field.
      arch: "Architecture",
      bindAddress: "Listen address",
      bindHost: "Bind host",
      bindPort: "Bind port",
      buildVersion: "Build version",
      capacity: "Capacity",
      clusterId: "Cluster",
      code: "Code",
      cpuCores: "CPU cores",
      cpuModel: "CPU model",
      daemonVersion: "Agent version",
      draining: "Draining",
      ejected: "Ejected",
      eventType: "Event type",
      healthState: "Health",
      heartbeatIntervalSeconds: "Heartbeat interval (s)",
      hostCount: "Hosts",
      hostId: "Host",
      hostName: "Host",
      hostname: "Hostname",
      instanceCount: "Instances",
      instanceId: "Instance",
      joinMode: "Join mode",
      kernelVersion: "Kernel",
      lastOnlineAt: "Last online",
      lbStrategy: "Load balancing",
      localIps: "Local addresses",
      macAddresses: "MAC addresses",
      machineCode: "Machine code",
      memoryTotalMb: "Memory",
      message: "Message",
      occurredAt: "Occurred at",
      offlineThresholdSeconds: "Offline threshold (s)",
      onlineInstanceCount: "Online instances",
      osName: "Operating system",
      osVersion: "OS version",
      platform: "Platform",
      processPid: "PID",
      processStartedAt: "Process started",
      publicEndpoint: "Public endpoint",
      qualityScore: "Quality score",
      remoteIp: "Remote address",
      restartCount: "Restarts",
      role: "Role",
      routingEnabled: "Routing enabled",
      routingState: "Routing",
      routingWeight: "Routing weight",
      servedDomains: "Served domains",
      severity: "Severity",
      syncStatus: "Config sync",
      tunnelRouteDomain: "Tunnel domain",
      uptime: "Uptime",
    },
    "zh-CN": {
      action: "操作动作",
      agentToken: "节点凭据",
      applicationId: "应用",
      applicationName: "应用",
      applicationType: "应用类型",
      hasSourceVersion: "源码",
      appConfigPath: "应用清单路径",
      artifactDriveUri: "发布包",
      artifactHash: "发布包哈希",
      artifactSize: "包大小",
      checkInterval: "检查间隔（秒）",
      checkType: "检查方式",
      checkUrl: "检查地址",
      commitHash: "提交哈希",
      completedAt: "完成时间",
      configContent: "配置内容",
      configName: "配置名称",
      configType: "配置类型",
      createdAt: "创建时间",
      deployedAt: "发布时间",
      deployType: "发布方式",
      deploymentConfigPath: "部署配置路径",
      description: "描述",
      shortDescription: "简短说明",
      fullDescription: "完整说明",
      releaseNotes: "版本说明",
      category: "应用分类",
      keywords: "关键词（逗号分隔）",
      supportUrl: "支持服务地址",
      privacyPolicyUrl: "隐私政策地址",
      publicRoot: "静态资源根目录",
      officialWebsiteUrl: "官方网站",
      durationMs: "耗时",
      environment: "发布环境",
      endDate: "结束日期",
      host: "主机",
      id: "ID",
      isActive: "已激活",
      isSecret: "敏感变量",
      key: "变量名",
      name: "应用名称",
      operatorId: "操作人 ID",
      operatorType: "操作人类型",
      checkedAt: "最后检查时间",
      expiresAt: "过期时间",
      retryCount: "重试次数",
      rollbackFromDeploymentId: "还原来源版本",
      siteType: "运行类型",
      sourceType: "源码类型",
      sourceVersionId: "源码版本",
      sourceVersionRetentionLimit: "保留版本数",
      sourceRef: "源码分支",
      spaFallback: "SPA 回退页面",
      sshPort: "SSH 端口",
      startDate: "开始日期",
      startedAt: "开始时间",
      status: "状态",
      retained: "保留中",
      configSnapshot: "标准配置",
      timeoutMs: "超时时间（毫秒）",
      targetType: "目标类型",
      targetUuid: "目标 ID",
      tenantScopeHash: "租户范围哈希",
      updatedAt: "更新时间",
      value: "变量值",
      versionNo: "版本",
      versionTag: "版本号",
      desiredSyncVersion: "期望版本",
      appliedSyncVersion: "应用版本",
      ipAddress: "IP 地址",
      lastHeartbeatAt: "最后心跳",
      // 集群资产列（cluster-clusters / cluster-hosts / cluster-instances /
      // cluster-events）。合成列（bindAddress / capacity / platform /
      // routingState）由列计划计算，不对应单一接口字段。
      arch: "架构",
      bindAddress: "监听地址",
      bindHost: "绑定地址",
      bindPort: "绑定端口",
      buildVersion: "构建版本",
      capacity: "规格",
      clusterId: "所属集群",
      code: "集群编码",
      cpuCores: "CPU 核数",
      cpuModel: "CPU 型号",
      daemonVersion: "Agent 版本",
      draining: "排水迁移",
      ejected: "已摘除",
      eventType: "事件类型",
      healthState: "健康状态",
      heartbeatIntervalSeconds: "心跳间隔（秒）",
      hostCount: "主机数",
      hostId: "所在主机",
      hostName: "主机",
      hostname: "主机名",
      instanceCount: "实例数",
      instanceId: "实例",
      joinMode: "接入方式",
      kernelVersion: "内核版本",
      lastOnlineAt: "最后在线",
      lbStrategy: "负载均衡策略",
      localIps: "内网地址",
      macAddresses: "MAC 地址",
      machineCode: "机器指纹",
      memoryTotalMb: "内存",
      message: "事件内容",
      occurredAt: "发生时间",
      offlineThresholdSeconds: "离线判定阈值（秒）",
      onlineInstanceCount: "在线实例",
      osName: "操作系统",
      osVersion: "系统版本",
      platform: "运行平台",
      processPid: "进程 PID",
      processStartedAt: "进程启动时间",
      publicEndpoint: "对外端点",
      qualityScore: "服务质量",
      remoteIp: "远端地址",
      restartCount: "重启次数",
      role: "角色",
      routingEnabled: "参与路由",
      routingState: "路由状态",
      routingWeight: "路由权重",
      servedDomains: "承载域名",
      severity: "级别",
      syncStatus: "配置同步",
      tunnelRouteDomain: "隧道域名",
      uptime: "运行时长",
    },
  };
  return labels[locale][value] ?? humanize(value);
}

function sensitive(value: string): boolean {
  return /secret|password|token|private|key/i.test(value);
}

function actionAvailable(
  action: WebserverResourceAction,
  selectedItem: Record<string, unknown> | undefined,
): boolean {
  return action.availableWhen?.({ body: action.bodyTemplate, selectedItem }) ?? true;
}

function optionValue(option: WebserverResourceFieldOptionValue): number | string {
  return typeof option === "object" ? option.value : option;
}

function optionRelatedValues(
  option: WebserverResourceFieldOptionValue,
): Readonly<Record<string, number | string>> | undefined {
  return typeof option === "object" ? option.relatedValues : undefined;
}

function optionLabel(option: WebserverResourceFieldOptionValue, name: string, locale: WebserverLocale): string {
  if (typeof option === "object") return option.label;
  return codedValueLabel(name, option, locale) ?? String(option);
}

function codedValueLabel(name: string, value: unknown, locale: WebserverLocale): string | undefined {
  const labels: Record<WebserverLocale, Partial<Record<string, string>>> = {
    "en-US": {
      "certType:1": "Let's Encrypt",
      "certType:2": "Custom certificate",
      "certType:3": "Self-signed certificate",
      "configType:1": "Global",
      "configType:2": "Site",
      "configType:3": "Domain",
      "configType:4": "Custom",
      "lbStrategy:consistent_hash": "Consistent hash",
      "lbStrategy:ip_hash": "IP hash",
      "lbStrategy:least_connections": "Least connections",
      "lbStrategy:random": "Random",
      "lbStrategy:random_two_choices": "Power of two choices",
      "lbStrategy:round_robin": "Round robin",
      "lbStrategy:weighted_round_robin": "Weighted round robin",
      "targetType:site": "Application",
      "targetType:domain": "Domain",
      "targetType:deployment": "Deployment",
      "targetType:certificate": "Certificate",
      "targetType:nginx_config": "Nginx configuration",
      "targetType:server": "Server",
    },
    "zh-CN": {
      "certType:1": "Let's Encrypt",
      "certType:2": "自定义证书",
      "certType:3": "自签名证书",
      "configType:1": "全局配置",
      "configType:2": "应用配置",
      "configType:3": "域名配置",
      "configType:4": "自定义配置",
      "lbStrategy:consistent_hash": "一致性哈希",
      "lbStrategy:ip_hash": "IP 哈希",
      "lbStrategy:least_connections": "最少连接",
      "lbStrategy:random": "随机",
      "lbStrategy:random_two_choices": "两次随机择优",
      "lbStrategy:round_robin": "轮询",
      "lbStrategy:weighted_round_robin": "加权轮询",
      "targetType:site": "应用",
      "targetType:domain": "域名",
      "targetType:deployment": "发布",
      "targetType:certificate": "证书",
      "targetType:nginx_config": "Nginx 配置",
      "targetType:server": "服务器",
    },
  };
  return labels[locale][`${name}:${String(value)}`];
}

function booleanLabel(value: boolean, locale: WebserverLocale): string {
  return locale === "zh-CN" ? (value ? "是" : "否") : (value ? "Yes" : "No");
}

function hasUnavailableOptions(
  body: Record<string, unknown>,
  fieldOptions: WebserverResourceFieldOptions,
  paginatedFields: readonly string[] | undefined,
): boolean {
  return Object.entries(fieldOptions).some(([name, options]) => (
    name in body
    && !paginatedFields?.includes(name)
    && options.length === 0
  ));
}

function hasMissingRequiredFields(
  body: Record<string, unknown>,
  requiredFields: readonly string[] | undefined,
): boolean {
  return requiredFields?.some((field) => {
    const value = body[field];
    return value === undefined
      || value === null
      || (typeof value === "string" && !value.trim())
      || (Array.isArray(value) && value.length === 0);
  }) ?? false;
}

function initialActionBody(
  action: WebserverResourceAction,
  selected: Record<string, unknown> | undefined,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(action.bodyTemplate).map(([field, fallback]) => [
      field,
      selected?.[field] !== undefined ? selected[field] : fallback,
    ]),
  );
}

function activeFilterCount(filters: Readonly<Record<string, string>>): number {
  return Object.values(filters).filter((value) => value.trim()).length;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * One column of a registry-driven resource table.
 *
 * `read` is a resolver rather than a field name because several columns an
 * operator actually needs are composites of more than one wire field: an
 * instance's listening address is `bindHost` + `bindPort`, its routing posture
 * is `routingEnabled` + `draining` + `ejected` together, and its live-count
 * summary is `cpuCores` + `memoryTotalMb`. A field-per-column model would
 * either drop those facts or spend three near-empty columns stating one.
 */
interface WebserverResourceColumn {
  /** Stable column id: the label i18n suffix, the visibility key, and the cell formatter selector. */
  id: string;
  /**
   * The field belongs to the record's detail row, not to a table column.
   *
   * A resource planned this way is expandable: the collapsed row carries only
   * the fields an operator compares across records, and everything else is
   * rendered in the row's own detail — which is why a `detailOnly` field is not
   * offered in the column menu either. Widening a fleet table until each column
   * is clipped is the failure mode this exists to prevent.
   *
   * This replaced a `defaultVisible: false` flag, which kept a secondary column
   * one click away in the column menu. That state is what the detail row now
   * covers, for the fields a row cannot carry at any width.
   */
  detailOnly?: boolean;
  read(item: Record<string, unknown>): unknown;
  /**
   * Wire keys this column's `read` consumes, when they are not the column `id`.
   *
   * A composite column turns several wire fields into one cell (`bindHost` +
   * `bindPort` -> "0.0.0.0:3800"). The detail row appends payload keys the plan
   * does not mention, so that a field the API started returning is never
   * silently dropped - but that fallback cannot tell "the plan forgot this
   * field" from "the plan already composed this field into a cell", and would
   * render the same fact twice under a worse name. Declaring the sources is what
   * separates the two.
   */
  sources?: readonly string[];
  /**
   * Preferred width in px. Not a hard constraint: it is summed into the table's
   * scroll envelope (`min-width`), which is what keeps a wide table readable
   * instead of letting `table-layout: auto` squeeze every column into the
   * viewport and clip the values.
   */
  width?: number;
}

/**
 * Hand-authored column plans for the cluster surfaces.
 *
 * These resources previously fell through to field-order inference, which put
 * the first eight keys of the JSON payload on screen in payload order: the
 * instance list rendered `id, clusterId, hostId, hostName, name, role,
 * environment, processPid` and hid liveness, health, routing and sync - i.e.
 * every column an operator opens the page for.
 *
 * A plan therefore states the columns explicitly, and splits them in two:
 *
 * - the **row** carries what an operator compares *across* records - identity,
 *   placement, and the state they triage on. Every planned column is visible by
 *   default; a column that would have to start hidden to keep the row readable
 *   is not a row column at all.
 * - `detailOnly` fields describe **one** record - versions, thresholds, PIDs,
 *   addresses, timestamps. They render in that record's own detail row, which
 *   opens on a click (`expandable` is derived from this split, see
 *   `ResourcePage`), and they are deliberately not offered in the column menu:
 *   re-adding one as a column is exactly the "widen the table until it curls"
 *   shape the detail row replaces.
 */
const RESOURCE_COLUMN_PLANS: Partial<Record<WebserverResourceKey, readonly WebserverResourceColumn[]>> = {
  "cluster-instances": [
    // The row answers "which instance is where, and is it healthy": the slot an
    // operator routes by, the host it runs on, and the four state codes they
    // scan for an anomaly. Runtimes, versions, weights and timestamps describe
    // this one instance, so they open with it.
    { id: "name", read: (item) => item.name, width: 190 },
    { id: "hostName", read: (item) => item.hostName ?? item.hostId, width: 150 },
    { id: "bindAddress", read: (item) => bindAddress(item), sources: ["bindHost", "bindPort"], width: 150 },
    { id: "role", read: (item) => item.role, width: 110 },
    { id: "status", read: (item) => item.status, width: 100 },
    { id: "healthState", read: (item) => item.healthState, width: 110 },
    { id: "routingState", read: (item) => routingState(item), sources: ["routingEnabled", "draining", "ejected"], width: 120 },
    { id: "syncStatus", read: (item) => item.syncStatus, width: 110 },
    { id: "joinMode", read: (item) => item.joinMode, detailOnly: true },
    { id: "buildVersion", read: (item) => item.buildVersion, detailOnly: true },
    { id: "environment", read: (item) => item.environment, detailOnly: true },
    { id: "uptime", read: (item) => item.uptimeSeconds, sources: ["uptimeSeconds"], detailOnly: true },
    { id: "lastHeartbeatAt", read: (item) => item.lastHeartbeatAt, detailOnly: true },
    { id: "processStartedAt", read: (item) => item.processStartedAt, detailOnly: true },
    { id: "lastOnlineAt", read: (item) => item.lastOnlineAt, detailOnly: true },
    { id: "qualityScore", read: (item) => item.qualityScore, detailOnly: true },
    { id: "restartCount", read: (item) => item.restartCount, detailOnly: true },
    { id: "routingWeight", read: (item) => item.routingWeight, detailOnly: true },
    { id: "processPid", read: (item) => item.processPid, detailOnly: true },
    { id: "publicEndpoint", read: (item) => item.publicEndpoint, detailOnly: true },
    { id: "clusterId", read: (item) => item.clusterId, detailOnly: true },
    { id: "hostId", read: (item) => item.hostId, detailOnly: true },
    { id: "id", read: (item) => item.id, detailOnly: true },
    { id: "createdAt", read: (item) => item.createdAt, detailOnly: true },
    { id: "updatedAt", read: (item) => item.updatedAt, detailOnly: true },
  ],
  "cluster-hosts": [
    // A host's row is its machine: what it is, what it runs, how much it has,
    // and whether it is up. Addresses, agent version, hardware identifiers and
    // timestamps are facts about one machine.
    { id: "hostname", read: (item) => item.hostname, width: 180 },
    { id: "clusterId", read: (item) => item.clusterId, width: 150 },
    { id: "platform", read: (item) => platformLabel(item), sources: ["osName", "osVersion"], width: 200 },
    { id: "arch", read: (item) => item.arch, width: 90 },
    { id: "capacity", read: (item) => capacityLabel(item), sources: ["cpuCores", "memoryTotalMb"], width: 130 },
    { id: "status", read: (item) => item.status, width: 110 },
    { id: "instanceCount", read: (item) => item.instanceCount, width: 90 },
    { id: "remoteIp", read: (item) => item.remoteIp, detailOnly: true },
    { id: "localIps", read: (item) => addressList(item.localIps), detailOnly: true },
    { id: "daemonVersion", read: (item) => item.daemonVersion, detailOnly: true },
    { id: "joinMode", read: (item) => item.joinMode, detailOnly: true },
    { id: "lastHeartbeatAt", read: (item) => item.lastHeartbeatAt, detailOnly: true },
    { id: "machineCode", read: (item) => item.machineCode, detailOnly: true },
    { id: "cpuModel", read: (item) => item.cpuModel, detailOnly: true },
    { id: "macAddresses", read: (item) => addressList(item.macAddresses), detailOnly: true },
    { id: "kernelVersion", read: (item) => item.kernelVersion, detailOnly: true },
    { id: "tunnelRouteDomain", read: (item) => item.tunnelRouteDomain, detailOnly: true },
    { id: "id", read: (item) => item.id, detailOnly: true },
    { id: "createdAt", read: (item) => item.createdAt, detailOnly: true },
    { id: "updatedAt", read: (item) => item.updatedAt, detailOnly: true },
  ],
  "cluster-clusters": [
    // The row is the fleet summary: what identifies a cluster and the three
    // counters an operator triages by. Thresholds, balancing strategy, served
    // domains, description and timestamps describe one cluster rather than
    // comparing many, so they live in that record's detail row.
    { id: "name", read: (item) => item.name, width: 200 },
    { id: "code", read: (item) => item.code, width: 160 },
    { id: "status", read: (item) => item.status, width: 110 },
    { id: "instanceCount", read: (item) => item.instanceCount, width: 100 },
    { id: "onlineInstanceCount", read: (item) => item.onlineInstanceCount, width: 130 },
    { id: "hostCount", read: (item) => item.hostCount, width: 100 },
    { id: "lbStrategy", read: (item) => item.lbStrategy, detailOnly: true },
    { id: "heartbeatIntervalSeconds", read: (item) => item.heartbeatIntervalSeconds, detailOnly: true },
    { id: "offlineThresholdSeconds", read: (item) => item.offlineThresholdSeconds, detailOnly: true },
    { id: "servedDomains", read: (item) => item.servedDomains, detailOnly: true },
    { id: "description", read: (item) => item.description, detailOnly: true },
    { id: "id", read: (item) => item.id, detailOnly: true },
    { id: "createdAt", read: (item) => item.createdAt, detailOnly: true },
    { id: "updatedAt", read: (item) => item.updatedAt, detailOnly: true },
  ],
  // An event log is read as a timeline: when it happened, how bad it is, what
  // kind of thing it is, and what it says. The identifiers an event points at
  // are what an operator copies into another page, not what they scan a log for
  // - and an event scoped above the instance level carries two of them empty, so
  // inline they are three columns of `-` between the reader and the message.
  // The payload is the opposite case: it nests, so no cell can hold it.
  "cluster-events": [
    { id: "occurredAt", read: (item) => item.occurredAt, width: 170 },
    { id: "severity", read: (item) => item.severity, width: 110 },
    { id: "eventType", read: (item) => item.eventType, width: 200 },
    { id: "message", read: (item) => item.message, width: 340 },
    { id: "clusterId", read: (item) => item.clusterId, detailOnly: true },
    { id: "hostId", read: (item) => item.hostId, detailOnly: true },
    { id: "instanceId", read: (item) => item.instanceId, detailOnly: true },
    { id: "detail", read: (item) => item.detail, detailOnly: true },
    { id: "id", read: (item) => item.id, detailOnly: true },
    { id: "createdAt", read: (item) => item.createdAt, detailOnly: true },
  ],
};

/**
 * Preferred field order for resources that have no explicit plan.
 *
 * Ordering only: the column set still follows the payload, because the
 * workspace has no other description of a resource owned by another SDK.
 */
const PREFERRED_FIELD_ORDER: Partial<Record<WebserverResourceKey, readonly string[]>> = {
  nginx: ["id", "configName", "configType", "isActive", "status", "versionNo", "deployedAt", "updatedAt"],
  servers: ["id", "name", "host", "sshPort", "status", "lastHeartbeatAt", "createdAt"],
  audit: ["operatorId", "operatorType", "action", "targetType", "targetUuid", "ipAddress", "createdAt"],
};

/**
 * Column ceiling for inferred columns. An inferred table is a fallback, and a
 * wide payload rendered as-is is unreadable; a resource that genuinely needs
 * more earns an explicit plan above.
 */
const INFERRED_COLUMN_LIMIT = 8;

/**
 * Storage key prefix for per-resource column choices.
 *
 * Namespaced by resource so the choice survives navigation and reload, and so a
 * change to one table's plan cannot silently rewrite another table's view.
 */
const COLUMN_PREFERENCE_PREFIX = "sdkwork.webserver.pc.table-columns.";

/**
 * Reads the operator's column choices for one resource.
 *
 * Best-effort by design: a blocked or full `localStorage` (private browsing,
 * quota) must degrade to "no preference recorded" — the plan defaults then apply
 * — rather than surface as a page failure.
 */
function readColumnPreferences(resource: WebserverResourceKey): Record<string, boolean> {
  if (typeof window === "undefined") return {};
  try {
    const raw = window.localStorage.getItem(`${COLUMN_PREFERENCE_PREFIX}${resource}`);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed as Record<string, unknown>)
        .filter((entry): entry is [string, boolean] => typeof entry[1] === "boolean"),
    );
  } catch {
    return {};
  }
}

function writeColumnPreferences(resource: WebserverResourceKey, preferences: Record<string, boolean>): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(`${COLUMN_PREFERENCE_PREFIX}${resource}`, JSON.stringify(preferences));
  } catch {
    // A view preference that cannot be stored is not a page error.
  }
}

function resourceColumns(
  resource: WebserverResourceKey,
  items: readonly Record<string, unknown>[],
): WebserverResourceColumn[] {
  const planned = RESOURCE_COLUMN_PLANS[resource];
  if (planned) return [...planned];
  const available = Array.from(new Set(items.flatMap((item) => Object.keys(item))));
  const preferred = PREFERRED_FIELD_ORDER[resource] ?? [];
  return [
    ...preferred.filter((column) => available.includes(column)),
    ...available.filter((column) => !preferred.includes(column)),
  ]
    .slice(0, INFERRED_COLUMN_LIMIT)
    .map((id) => ({ id, read: (item: Record<string, unknown>) => item[id] }));
}

/**
 * Fields of a record that only its detail row shows.
 *
 * Empty for every resource whose plan has no `detailOnly` field, and an empty
 * result is what tells the page the resource is not expandable: a table whose
 * plan fits inline stays a plain table.
 *
 * The payload's own keys are appended after the planned fields. A plan can fall
 * behind the API, and the detail row is the one surface that shows a whole
 * record - silently dropping a field the API started returning would make the
 * page lie about what it received. What is *not* appended is a key the plan
 * already accounts for: either it has a column of its own, or a composite column
 * consumes it (`sources`), in which case listing it again would print one fact
 * twice.
 */
function resourceDetailColumns(
  resource: WebserverResourceKey,
  item: Record<string, unknown>,
): WebserverResourceColumn[] {
  const plan = RESOURCE_COLUMN_PLANS[resource];
  if (!plan) return [];
  const detail = plan.filter((column) => column.detailOnly);
  if (detail.length === 0) return [];
  const accounted = new Set<string>();
  for (const column of plan) {
    // A column whose id is the wire key consumes that key.
    accounted.add(column.id);
    for (const source of column.sources ?? []) accounted.add(source);
  }
  return [
    ...detail,
    ...Object.keys(item)
      .filter((key) => !accounted.has(key))
      .map((id) => ({ id, read: (row: Record<string, unknown>) => row[id] })),
  ];
}

/**
 * Detail-row cell value: the table formatter, except that a list is spelled out.
 *
 * The table formatter bounds a list (`a, b, c +2`) because a row has no room for
 * more, but the detail row exists precisely to read the whole record, so
 * truncating there would defeat the reason the operator expanded it.
 */
function detailValue(
  value: unknown,
  column: string,
  resource: WebserverResourceKey,
  locale: WebserverLocale,
): ReactNode {
  if (Array.isArray(value)) {
    const entries = value.filter((entry): entry is string => typeof entry === "string" && entry.length > 0);
    return entries.length > 0 ? entries.join(", ") : "-";
  }
  if (column === "id" && typeof value === "string" && value.length > 0) return <code>{value}</code>;
  // A structured payload nests, and `JSON.stringify` on one line turns it into a
  // wall of delimiters. The detail row is the only surface that ever shows one,
  // so it is worth line breaks and indentation.
  if (isRecord(value)) {
    return <pre className="resource-detail-json">{JSON.stringify(value, null, 2)}</pre>;
  }
  return displayValue(value, column, resource, locale);
}

/**
 * Detail of one record, revealed by expanding its row.
 *
 * It carries the record's full field set plus the row-scoped operations, which
 * is the whole point of expanding: the operator who opened one cluster can act
 * on that cluster without first ticking a checkbox to tell the toolbar which row
 * they meant.
 */
function ResourceDetail({
  actions,
  item,
  locale,
  onAction,
  resource,
  t,
}: {
  actions: readonly WebserverResourceAction[];
  item: Record<string, unknown>;
  locale: WebserverLocale;
  onAction(action: WebserverResourceAction, item: Record<string, unknown>): void;
  resource: WebserverResourceKey;
  t: (key: WebserverMessageKey, values?: Record<string, string | number>) => string;
}) {
  const fields = resourceDetailColumns(resource, item);
  return (
    <div className="resource-detail" data-resource={resource}>
      <dl className="resource-detail-grid">
        {fields.map((field) => {
          const value = field.read(item);
          return (
            <div className={isRecord(value) ? "resource-detail-block" : undefined} key={field.id}>
              <dt>{resourceFieldLabel(resource, field.id, locale)}</dt>
              <dd>{detailValue(value, field.id, resource, locale)}</dd>
            </div>
          );
        })}
      </dl>
      {actions.length > 0 ? (
        <div className="resource-detail-actions" role="group" aria-label={t("table.actions")}>
          {actions.map((action) => (
            <button
              className={action.dangerous ? "danger-button" : "secondary-button"}
              key={action.id}
              onClick={() => onAction(action, item)}
              type="button"
            >
              <ActionIcon action={action} />
              {resolveActionLabel(t, resource, action, item)}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/**
 * Human handle of a row, for the detail control's accessible name.
 *
 * A screen reader announcing "Expand row 3" names nothing an operator can check
 * against the list, so the row's own label wins and the record key is only the
 * last resort. `eventType` is in the list for the same reason: an event has no
 * name, and "Expand evt-2" leaves the listener with an identifier they cannot
 * match against anything they can hear.
 */
function rowLabel(item: Record<string, unknown>, index: number): string {
  for (const key of ["name", "hostname", "title", "eventType"]) {
    const value = item[key];
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return recordKey(item, index);
}

/** Listening address of an instance: the slot identity an operator routes by. */
function bindAddress(item: Record<string, unknown>): string | undefined {
  const port = item.bindPort;
  if (port === null || port === undefined || port === "") return undefined;
  const host = typeof item.bindHost === "string" && item.bindHost.trim() ? item.bindHost.trim() : "0.0.0.0";
  return `${host}:${port}`;
}

/**
 * Routing posture as one value.
 *
 * The three routing flags are one operational state, not three:
 * drain supersedes cordon (a draining instance is also out of the pool), and an
 * ejected instance was taken out by the prober rather than by a person. Showing
 * them as separate boolean columns makes the operator read a truth table.
 */
function routingState(item: Record<string, unknown>): string {
  if (item.draining === true) return "DRAINING";
  if (item.ejected === true) return "EJECTED";
  if (item.routingEnabled === false) return "CORDONED";
  return "ROUTING";
}

/** `OS version · kernel` as one host platform cell. */
function platformLabel(item: Record<string, unknown>): string | undefined {
  const name = typeof item.osName === "string" ? item.osName : "";
  const version = typeof item.osVersion === "string" ? item.osVersion : "";
  const parts = [name, version].filter((part) => part.length > 0);
  return parts.length > 0 ? parts.join(" ") : undefined;
}

/** Host capacity as `N cores · M GiB`, the shape fleet pages are read in. */
function capacityLabel(item: Record<string, unknown>): string | undefined {
  const parts: string[] = [];
  if (typeof item.cpuCores === "number") parts.push(`${item.cpuCores} vCPU`);
  if (typeof item.memoryTotalMb === "number" && item.memoryTotalMb > 0) {
    const gib = item.memoryTotalMb / 1024;
    parts.push(`${gib >= 10 ? Math.round(gib) : gib.toFixed(1)} GiB`);
  }
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

/** Renders an address/domain list as a single cell, bounded so one long list cannot dominate the row. */
function addressList(value: unknown): string | undefined {
  if (!Array.isArray(value) || value.length === 0) return undefined;
  const items = value.filter((entry): entry is string => typeof entry === "string" && entry.length > 0);
  if (items.length === 0) return undefined;
  const head = items.slice(0, 3).join(", ");
  return items.length > 3 ? `${head} +${items.length - 3}` : head;
}

function serverStatus(value: unknown, locale: WebserverLocale): string {
  const statuses: Record<WebserverLocale, Record<string, string>> = {
    "en-US": { "0": "Offline", "1": "Online" },
    "zh-CN": { "0": "离线", "1": "在线" },
  };
  return statuses[locale][String(value)] ?? String(value);
}

