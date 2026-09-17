import {
  Activity,
  BadgeCheck,
  Check,
  ChevronLeft,
  ChevronRight,
  Clipboard,
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
  const [pageInfo, setPageInfo] = useState<WebserverPageInfo>({ page: 1, pageSize: 20, hasMore: false, mode: "offset" });
  const [nextCursor, setNextCursor] = useState<string | undefined>(undefined);
  /** Cursor that loaded the currently displayed page (its start token). */
  const [currentCursor, setCurrentCursor] = useState<string | undefined>(undefined);
  /** Start tokens of pages behind the current one, for cursor-mode back navigation. */
  const [cursorHistory, setCursorHistory] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [filters, setFilters] = useState<Record<string, string>>({});
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [selected, setSelected] = useState<Record<string, unknown>>();
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
        pageSize: 20,
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

  useEffect(() => {
    void load();
  }, [authorized, entry.resource, page]);
  useEffect(() => {
    resetPagination();
    setSelected(undefined);
  }, [entry.resource]);

  const columns = useMemo(
    () => resourceColumns(entry.resource, items),
    [entry.resource, items],
  );
  const resourceLabel = resourceText(t, entry.resource, "label");

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
            <div aria-busy={busy} className="table-frame">
              {busy && items.length > 0 ? <span aria-hidden="true" className="table-loading-bar" /> : null}
              {busy && items.length === 0 ? (
                <div className="empty-state" role="status">
                  <LoaderCircle aria-hidden="true" className="is-spinning" size={20} />
                  <span>{t("table.loading")}</span>
                </div>
              ) : items.length === 0 ? (
                <div className="empty-state">
                  <Inbox aria-hidden="true" size={20} />
                  <span>{t("table.empty")}</span>
                </div>
              ) : (
                <table className="resource-table">
                  <thead>
                    <tr>
                      <th aria-label={t("table.select")} />
                      {columns.map((column) => <th key={column}>{fieldLabel(column, locale)}</th>)}
                    </tr>
                  </thead>
                  <tbody>
                    {items.map((item, index) => (
                      <tr
                        className={selected === item ? "selected" : ""}
                        key={recordKey(item, index)}
                        onClick={() => setSelected(item)}
                      >
                        <td>
                          <input
                            aria-label={t("table.selectRow", { row: index + 1 })}
                            checked={selected === item}
                            readOnly
                            type="radio"
                          />
                        </td>
                        {columns.map((column) => (
                          <td key={column}>{displayValue(item[column], column, entry.resource, locale)}</td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
            {(items.length > 0 || busy || page > 1) ? (
              <footer className="pagination">
                <span>
                  {pageInfo.total === undefined
                    ? t("pagination.page", { page: pageInfo.page })
                    : t("pagination.total", { total: pageInfo.total })}
                </span>
                <button
                  aria-label={t("pagination.previous")}
                  className="icon-button"
                  disabled={busy || (pageInfo.mode === "cursor" ? cursorHistory.length === 0 : page <= 1)}
                  onClick={goToPreviousPage}
                  title={t("pagination.previous")}
                  type="button"
                >
                  <ChevronLeft aria-hidden="true" size={18} />
                </button>
                <button
                  aria-label={t("pagination.next")}
                  className="icon-button"
                  disabled={busy || !pageInfo.hasMore}
                  onClick={goToNextPage}
                  title={t("pagination.next")}
                  type="button"
                >
                  <ChevronRight aria-hidden="true" size={18} />
                </button>
              </footer>
            ) : null}
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

function displayValue(value: unknown, column: string, resource: WebserverResourceKey, locale: WebserverLocale): ReactNode {
  if (value === null || value === undefined) return "-";
  if (resource === "servers" && column === "status") {
    return <span className={`status-badge server-status-${String(value).toLowerCase()}`}>{serverStatus(value, locale)}</span>;
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

function humanize(value: string): string {
  return value.replace(/([a-z])([A-Z])/g, "$1 $2").replaceAll("_", " ");
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

function resourceColumns(
  resource: WebserverResourceKey,
  items: readonly Record<string, unknown>[],
): string[] {
  const available = Array.from(new Set(items.flatMap((item) => Object.keys(item))));
  const preferred: Partial<Record<WebserverResourceKey, readonly string[]>> = {
    nginx: ["id", "configName", "configType", "isActive", "status", "versionNo", "deployedAt", "updatedAt"],
    servers: ["id", "name", "host", "sshPort", "status", "lastHeartbeatAt", "createdAt"],
    audit: ["operatorId", "operatorType", "action", "targetType", "targetUuid", "ipAddress", "createdAt"],
  };
  const ordered = [
    ...(preferred[resource] ?? []).filter((column) => available.includes(column)),
    ...available.filter((column) => !(preferred[resource] ?? []).includes(column)),
  ];
  return ordered.slice(0, 8);
}

function serverStatus(value: unknown, locale: WebserverLocale): string {
  const statuses: Record<WebserverLocale, Record<string, string>> = {
    "en-US": { "0": "Offline", "1": "Online" },
    "zh-CN": { "0": "离线", "1": "在线" },
  };
  return statuses[locale][String(value)] ?? String(value);
}

