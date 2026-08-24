import {
  Activity,
  AppWindow,
  BadgeCheck,
  Check,
  ChevronLeft,
  ChevronRight,
  Clipboard,
  Filter,
  FileArchive,
  FolderOpen,
  GitBranch,
  Inbox,
  Image,
  ImagePlus,
  Images,
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
  WandSparkles,
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
import { Navigate, Route, Routes } from "react-router-dom";
import { uuid } from "@sdkwork/utils/id";

import { translateWebserver, type WebserverLocale, type WebserverMessageKey } from "./i18n/index.ts";
import {
  APPLICATION_PREVIEW_LIMIT,
  ApplicationMediaValidationError,
  validateApplicationMediaFile,
  validateApplicationPreviewCount,
  type ApplicationMediaFieldErrors,
  type ApplicationStoreListingInput,
  type ApplicationSubmissionInput,
} from "./application-media.ts";
import {
  applicationStoreListing,
  storeListingBody,
} from "./application-store-submission.ts";
import {
  APPLICATION_GIT_REPOSITORY_MAX_LENGTH,
  isValidApplicationGitRepositoryUrl,
} from "./application-source-repository.ts";
import { formatWebserverErrorMessage } from "./error-message.ts";
import {
  canAccessWebserverResource,
  hasPlatformSuperAdminAccess,
  hasWebserverPermission,
  hasWebserverSuperAdminAccess,
} from "./permissions.ts";
import type {
  ApplicationDeploymentSourceMode,
  WebserverPageInfo,
  WebserverPcModuleDefinition,
  WebserverResourceAction,
  WebserverResourceActionContext,
  WebserverResourceDataSource,
  WebserverResourceFieldOptionPage,
  WebserverResourceFieldOptionValue,
  WebserverResourceFieldOptions,
  WebserverResourceKey,
  WebserverResourceRegistry,
} from "./types.ts";
import { WorkspaceHeader, WorkspaceSidebar } from "./WebserverWorkspaceChrome.tsx";

export interface WebserverWorkspaceProps {
  locale: WebserverLocale;
  modules: readonly WebserverPcModuleDefinition[];
  notificationsHref?: string;
  onSignOut?(): void;
  permissionScope: readonly string[];
  portalHref?: string;
  registry: WebserverResourceRegistry;
  resourceRenderers?: Partial<Record<WebserverResourceKey, ReactNode>>;
  surface: "app-console" | "backend-admin";
  userLabel?: string;
}

type ApplicationWizardStep = 0 | 1 | 2 | 3 | 4;

const FIELD_OPTION_PAGE_SIZE = 20;

export function WebserverWorkspace({
  locale,
  modules,
  notificationsHref,
  onSignOut,
  permissionScope,
  portalHref,
  registry,
  resourceRenderers,
  surface,
  userLabel,
}: WebserverWorkspaceProps) {
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
  const defaultResource = entries[0]?.resource;
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
        notificationsHref={notificationsHref}
        onSignOut={onSignOut}
        portalHref={portalHref}
        surface={surface}
        t={t}
        userLabel={userLabel}
      />
      <WorkspaceSidebar basePath={basePath} entries={entries} surface={surface} t={t} />
      <main className="workspace">
        {defaultResource ? (
          <Routes>
            {entries.map((entry) => (
              <Route
                key={entry.resource}
                path={`/${entry.resource}/*`}
                element={resourceRenderers?.[entry.resource] ?? (
                  <ResourcePage
                    entry={entry}
                    locale={locale}
                    permissionScope={permissionScope}
                    registry={registry}
                    source={registry[entry.resource]}
                    surface={surface}
                  />
                )}
              />
            ))}
            <Route path="*" element={<Navigate to={`${basePath}/${defaultResource}`} replace />} />
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
  registry,
  source,
  surface,
}: {
  entry: { permission: string; resource: WebserverResourceKey };
  locale: WebserverLocale;
  permissionScope: readonly string[];
  registry: WebserverResourceRegistry;
  source?: WebserverResourceDataSource;
  surface: "app-console" | "backend-admin";
}) {
  const t = (key: WebserverMessageKey, values?: Record<string, string | number>) =>
    translateWebserver(locale, key, values);
  const authorized = canAccessWebserverResource(surface, permissionScope, entry.permission);
  const scopeKind = source?.scopeKind ?? "application";
  const scopeSource = registry.applications;
  const scopeStorageKey = `sdkwork.webserver.${scopeKind}Id`;
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
  const [scopeId, setScopeId] = useState(() => sessionStorage.getItem(scopeStorageKey) ?? "");
  const [scopeOptions, setScopeOptions] = useState<readonly ScopeOption[]>([]);
  const [scopeBusy, setScopeBusy] = useState(false);
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
  const applicationRowActions = useMemo(
    () => isApplicationResource(entry.resource)
      ? visibleActions.filter((candidate) => isApplicationRowAction(candidate))
      : [],
    [entry.resource, visibleActions],
  );
  const commandActions = useMemo(
    () => visibleActions.filter((candidate) => !applicationRowActions.includes(candidate)),
    [applicationRowActions, visibleActions],
  );

  function persistScope(value: string): void {
    setScopeId(value);
    resetPagination();
    setSelected(undefined);
    if (value) sessionStorage.setItem(scopeStorageKey, value);
    else sessionStorage.removeItem(scopeStorageKey);
  }

  /** Resets both pagination modes to their first page. */
  function resetPagination(): void {
    setPage(1);
    setNextCursor(undefined);
    setCursorHistory([]);
    setCurrentCursor(undefined);
  }

  async function load(filterValues: Readonly<Record<string, string>> = filters, cursorOverride?: string | null): Promise<void> {
    if (!authorized || !source || (source.requiresScope && !scopeId)) {
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
        scopeId: scopeId || undefined,
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
    if (!authorized || !source?.requiresScope || !scopeSource) {
      setScopeOptions([]);
      return undefined;
    }
    let active = true;
    setScopeBusy(true);
    void scopeSource.load({ page: 1, pageSize: 100 })
      .then((result) => {
        if (!active) return;
        const options = result.items
          .map((item) => scopeOption(item, scopeKind))
          .filter((option): option is ScopeOption => Boolean(option));
        setScopeOptions(options);
        const nextScopeId = options.some((option) => option.id === scopeId)
          ? scopeId
          : options[0]?.id ?? "";
        persistScope(nextScopeId);
      })
      .catch((caught) => {
        if (active) setError(formatWebserverErrorMessage(caught, t));
      })
      .finally(() => {
        if (active) setScopeBusy(false);
      });
    return () => {
      active = false;
    };
  }, [authorized, entry.resource, scopeSource]);

  useEffect(() => {
    void load();
  }, [authorized, entry.resource, page, scopeId]);
  useEffect(() => {
    resetPagination();
    setSelected(undefined);
  }, [entry.resource]);

  const columns = useMemo(
    () => resourceColumns(entry.resource, items),
    [entry.resource, items],
  );
  const scopeLabel = t(scopeKind === "application" ? "toolbar.application" : "toolbar.application");
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
              {source?.requiresScope ? (
                <label className="scope-selector">
                  <AppWindow aria-hidden="true" size={16} />
                  <select
                    aria-label={scopeLabel}
                    disabled={scopeBusy || scopeOptions.length === 0}
                    onChange={(event) => persistScope(event.target.value)}
                    value={scopeId}
                  >
                    {scopeOptions.length === 0 ? (
                      <option value="">{scopeBusy ? t("scope.loading") : t("scope.none")}</option>
                    ) : null}
                    {scopeOptions.map((option) => (
                      <option key={option.id} value={option.id}>{option.label}</option>
                    ))}
                  </select>
                </label>
              ) : null}
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
              {commandActions.map((candidate) => (
                <button
                  className={candidate.dangerous
                    ? "danger-button"
                    : candidate.requiresSelection
                      ? "secondary-button"
                      : "command-button"}
                  disabled={busy
                    || (candidate.requiresSelection && !selected)
                    || (candidate.requiresScope && !scopeId)
                    || !actionAvailable(candidate, selected, scopeId)}
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
          {source?.requiresScope && !scopeId ? (
            <div className="empty-state empty-state-standalone">
              <AppWindow aria-hidden="true" size={20} />
              <span>{t("scope.none.description")}</span>
            </div>
          ) : (
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
                  <table className={applicationRowActions.length > 0 ? "resource-table has-row-actions" : "resource-table"}>
                    <thead>
                      <tr>
                        <th aria-label={t("table.select")} />
                        {columns.map((column) => <th key={column}>{fieldLabel(column, locale)}</th>)}
                        {applicationRowActions.length > 0 ? (
                          <th className="row-actions-column">{t("table.actions")}</th>
                        ) : null}
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
                          {applicationRowActions.length > 0 ? (
                            <td className="row-actions-cell">
                              <div className="row-actions">
                                {applicationRowActions.map((candidate) => {
                                  const label = actionText(t, entry.resource, candidate);
                                  const rowLabel = t("table.rowAction", {
                                    action: label,
                                    name: recordLabel(item, index),
                                  });
                                  return (
                                    <button
                                      aria-label={rowLabel}
                                      className={`row-action-button${candidate.dangerous ? " row-action-button-danger" : ""}`}
                                      disabled={busy || !actionAvailable(candidate, item, scopeId)}
                                      key={candidate.id}
                                      onClick={(event) => {
                                        event.stopPropagation();
                                        setSelected(item);
                                        setAction(candidate);
                                      }}
                                      title={rowLabel}
                                      type="button"
                                    >
                                      <ActionIcon action={candidate} />
                                    </button>
                                  );
                                })}
                              </div>
                            </td>
                          ) : null}
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
          )}
          {action ? (
            <ActionDialog
              action={action}
              label={actionText(t, entry.resource, action)}
              locale={locale}
              onClose={() => setAction(undefined)}
              onComplete={() => {
                setAction(undefined);
                void load();
              }}
              onRefresh={() => void load()}
              scopeId={scopeId || undefined}
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
  if (action.id === "update-source") return <Upload {...iconProps} />;
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

function isApplicationResource(resource: WebserverResourceKey): boolean {
  return resource === "applications";
}

function isApplicationRowAction(action: WebserverResourceAction): boolean {
  return action.id === "update"
    || action.id === "update-source"
    || action.id === "publish"
    || action.id === "delete";
}

function ActionDialog({
  action,
  label,
  locale,
  onClose,
  onComplete,
  onRefresh,
  scopeId,
  selected,
}: {
  action: WebserverResourceAction;
  label: string;
  locale: WebserverLocale;
  onClose(): void;
  onComplete(): void;
  onRefresh(): void;
  scopeId?: string;
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
  const [files, setFiles] = useState<readonly File[]>([]);
  const [fieldOptions, setFieldOptions] = useState<WebserverResourceFieldOptions>(action.fieldOptions ?? {});
  const [idempotencyKey] = useState(() => uuid());
  const [optionsBusy, setOptionsBusy] = useState(Boolean(action.loadFieldOptions));
  const [progress, setProgress] = useState(0);
  const [result, setResult] = useState<Record<string, unknown>>();
  const [copiedField, setCopiedField] = useState<string>();
  const [sourceInputMode, setSourceInputMode] = useState<ApplicationDeploymentSourceMode>("archive");
  const [sourceRepository, setSourceRepository] = useState("");
  const existingStoreListing = applicationStoreListing(selected?.storeListing);
  const [applicationSubmission, setApplicationSubmission] = useState<ApplicationSubmissionInput>(() => ({
    coverMode: action.applicationSubmission === "update" ? "keep" : "remove",
    iconMode: action.applicationSubmission === "update" && existingStoreListing?.icon ? "keep" : "default",
    previewFiles: [],
    previewsMode: action.applicationSubmission === "update" ? "keep" : "remove",
  }));
  const [mediaErrors, setMediaErrors] = useState<ApplicationMediaFieldErrors>({});
  const abortControllerRef = useRef<AbortController | undefined>(undefined);
  const dialogRef = useRef<HTMLFormElement>(null);
  const applicationStageRef = useRef<HTMLDivElement>(null);
  const submitInFlightRef = useRef(false);
  const confirmationRequired = Boolean(action.dangerous || action.requiresConfirmation);
  const sourceInputRequired = Boolean(action.sourceInput);
  const applicationCreationWizard = action.applicationSubmission === "create";
  const applicationEditDrawer = action.applicationSubmission === "update";
  const sourceUpdateAction = action.id === "update-source";
  const applicationDrawer = applicationCreationWizard || applicationEditDrawer;
  const [applicationStep, setApplicationStep] = useState<ApplicationWizardStep>(0);
  const [furthestApplicationStep, setFurthestApplicationStep] = useState<ApplicationWizardStep>(0);
  const applicationIdentityFields = ["name", "description", "applicationType", "siteType"] as const;
  const applicationListingFields = [
    "shortDescription",
    "fullDescription",
    "releaseNotes",
    "category",
    "keywords",
    "supportUrl",
    "privacyPolicyUrl",
    "officialWebsiteUrl",
  ] as const;
  const applicationSourceFields = ["versionTag"] as const;
  const applicationConfigurationFields = [
    "environment",
    "sourceVersionRetentionLimit",
    "appConfigPath",
    "deploymentConfigPath",
    "publicRoot",
    "spaFallback",
  ] as const;
  const applicationRequiredFields: readonly string[] = [
    "name",
    "versionTag",
    "appConfigPath",
    "deploymentConfigPath",
    "publicRoot",
    "spaFallback",
  ];

  function closeDialog(): void {
    if (busy && !action.dismissibleWhileBusy) return;
    if (busy) {
      abortControllerRef.current?.abort();
      onRefresh();
    }
    onClose();
  }

  const sourceInputError = (): WebserverMessageKey | undefined => {
    if (!sourceInputRequired) return undefined;
    if (sourceInputMode === "git") {
      if (!sourceRepository.trim()) {
        return applicationCreationWizard
          ? "dialog.applicationGitRepositoryRequired"
          : "dialog.gitRepositoryRequired";
      }
      if (!isValidApplicationGitRepositoryUrl(sourceRepository)) {
        return "dialog.applicationGitRepositoryInvalid";
      }
      return undefined;
    }
    if (files.length > 0) return undefined;
    return applicationCreationWizard
      ? "dialog.applicationSourceRequired"
      : "dialog.sourceRequired";
  };

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
          scopeId={scopeId}
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

  const renderFields = (fields?: readonly string[], className?: string) => {
    const names = (fields ?? Object.keys(body)).filter((name) => applicationCreationWizard || name in body);
    return (
      <div className={`form-grid${className ? ` ${className}` : ""}`}>
        {names.map((name) => renderField(
          name,
          body[name],
          applicationCreationWizard && applicationRequiredFields.includes(name),
        ))}
      </div>
    );
  };

  const applicationStepError = (step: ApplicationWizardStep): WebserverMessageKey | undefined => {
    if (step === 0) {
      return hasMissingRequiredFields(body, ["name"])
        ? "dialog.applicationBasicRequired"
        : undefined;
    }
    if (step === 1) {
      return !applicationSubmissionReady(applicationSubmission, mediaErrors)
        ? "dialog.applicationMediaRequired"
        : undefined;
    }
    if (step === 3) {
      return hasMissingRequiredFields(body, [
        "appConfigPath",
        "deploymentConfigPath",
        "publicRoot",
        "spaFallback",
      ]) ? "dialog.applicationConfigRequired" : undefined;
    }
    if (step === 2) {
      const versionMissing = hasMissingRequiredFields(body, ["versionTag"]);
      const sourceError = sourceInputError();
      if (versionMissing && sourceError) return "dialog.applicationReleaseRequired";
      if (versionMissing) return "dialog.applicationVersionRequired";
      if (sourceError) return sourceError;
    }
    return undefined;
  };

  const applicationStepReady = (step: ApplicationWizardStep): boolean => !applicationStepError(step);

  const focusApplicationStep = (step: ApplicationWizardStep, invalid = false): void => {
    queueMicrotask(() => {
      const selector = step === 0
        ? "[data-field='name'] input:not([disabled])"
        : step === 2 && invalid
          ? hasMissingRequiredFields(body, ["versionTag"])
            ? "[data-field='versionTag'] input:not([disabled])"
            : sourceInputMode === "git"
              ? ".source-repository-input:not([disabled])"
              : ".source-file-trigger:not([disabled])"
          : ".form-grid input:not([disabled]), .form-grid select:not([disabled]), .form-grid textarea:not([disabled]), .source-file-trigger:not([disabled]), .source-repository-input:not([disabled]), [data-application-step-heading]";
      applicationStageRef.current?.querySelector<HTMLElement>(selector)?.focus();
    });
  };

  const moveToApplicationStep = (nextStep: ApplicationWizardStep): void => {
    if (nextStep > applicationStep) {
      for (let step = 0; step < nextStep; step += 1) {
        const checkedStep = step as ApplicationWizardStep;
        const errorKey = applicationStepError(checkedStep);
        if (!errorKey) continue;
        setApplicationStep(checkedStep);
        setError(t(errorKey));
        focusApplicationStep(checkedStep, true);
        return;
      }
    }
    setError(undefined);
    setApplicationStep(nextStep);
    setFurthestApplicationStep((current) => Math.max(current, nextStep) as ApplicationWizardStep);
  };

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    const initialFocus = dialogRef.current?.querySelector<HTMLElement>(
      ".form-grid input:not([disabled]), .form-grid select:not([disabled]), .form-grid textarea:not([disabled]), .source-file-trigger:not([disabled]), .source-repository-input:not([disabled]), .confirm-check input:not([disabled]), button[type='submit']:not([disabled])",
    );
    initialFocus?.focus();
    return () => queueMicrotask(() => {
      if (previousFocus?.isConnected) previousFocus.focus();
    });
  }, []);

  useEffect(() => {
    if (!applicationCreationWizard) return;
    focusApplicationStep(applicationStep);
  }, [applicationCreationWizard, applicationStep]);

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
    void action.loadFieldOptions({ body: initialActionBody(action, selected), scopeId, selectedItem: selected })
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
  }, [action, scopeId, selected]);

  useEffect(() => {
    if (!action.loadSourceInputDefaults) return undefined;
    let active = true;
    void action.loadSourceInputDefaults({
      body: initialActionBody(action, selected),
      scopeId,
      selectedItem: selected,
    }).then((defaults) => {
      if (!active) return;
      if (defaults.mode) setSourceInputMode(defaults.mode);
      if (defaults.repository) setSourceRepository(defaults.repository);
    }).catch(() => {
      // Loading a previous Git source is an optional convenience; manual source selection remains available.
    });
    return () => {
      active = false;
    };
  }, [action, scopeId, selected]);

  async function submit(event: FormEvent): Promise<void> {
    event.preventDefault();
    if (submitInFlightRef.current) return;
    if (applicationCreationWizard && applicationStep < 4) {
      moveToApplicationStep((applicationStep + 1) as ApplicationWizardStep);
      return;
    }
    if (
      (confirmationRequired && !confirmed)
      || (action.requiresFile && !file)
      || Boolean(sourceInputError())
      || (action.applicationSubmission && !applicationSubmissionReady(applicationSubmission, mediaErrors))
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
        applicationSubmission: action.applicationSubmission ? applicationSubmission : undefined,
        file,
        files,
        idempotencyKey,
        onProgress: (value) => setProgress(Math.max(0, Math.min(100, Math.round(value)))),
        scopeId,
        selectedItem: selected,
        signal: abortController.signal,
        sourceInputMode,
        sourceRepository: sourceInputMode === "git" ? sourceRepository : undefined,
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

  return (
    <div
      className={`dialog-backdrop${applicationDrawer ? " application-creation-drawer-backdrop" : ""}`}
      onKeyDown={(event) => trapDialogFocus(event, dialogRef.current ?? event.currentTarget)}
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) closeDialog();
      }}
      role="presentation"
    >
      <form
        aria-labelledby="action-title"
        aria-modal="true"
        className={`dialog${action.applicationSubmission ? " application-submission-dialog" : ""}${applicationDrawer ? " application-creation-dialog application-creation-drawer" : ""}${sourceUpdateAction ? " source-update-dialog" : ""}`}
        data-testid={applicationCreationWizard
          ? "application-creation-drawer"
          : applicationEditDrawer
            ? "application-edit-drawer"
            : sourceUpdateAction
              ? "application-source-update-dialog"
              : undefined}
        onSubmit={(event) => void submit(event)}
        ref={dialogRef}
        role="dialog"
      >
        <header>
          <div className="dialog-title-group">
            <h2 id="action-title">{label}</h2>
            {applicationCreationWizard ? (
              <span>{t("dialog.applicationStepCount", { current: applicationStep + 1, total: 5 })}</span>
            ) : null}
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
        {!result && !applicationDrawer ? (
          <div className="dialog-scroll">
            {confirmationRequired ? <div className="warning">{t("dialog.warning")}</div> : null}
            <div className="form-grid">
              {Object.entries(body).map(([name, value]) => renderField(
                name,
                value,
                action.requiredFields?.includes(name),
              ))}
            </div>
            {action.applicationSubmission ? (
              <ApplicationSubmissionFields
                disabled={busy}
                errors={mediaErrors}
                existing={existingStoreListing}
                locale={locale}
                onChange={setApplicationSubmission}
                onErrors={setMediaErrors}
                submission={applicationSubmission}
              />
            ) : null}
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
            {action.sourceInput ? (
              <ApplicationSourcePicker
                busy={busy}
                files={files}
                locale={locale}
                mode={sourceInputMode}
                onFilesChange={setFiles}
                onModeChange={setSourceInputMode}
                onRepositoryChange={setSourceRepository}
                repository={sourceRepository}
              />
            ) : null}
            {busy && (action.requiresFile || action.sourceInput) ? (
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
        {!result && applicationCreationWizard ? (
          <div className="application-wizard-workspace">
            <ApplicationWizardProgress
              currentStep={applicationStep}
              furthestStep={furthestApplicationStep}
              locale={locale}
              onStepChange={moveToApplicationStep}
            />
            <div className="application-wizard-stage" data-testid="application-wizard-stage" ref={applicationStageRef}>
              <div className="application-wizard-stage-content">
                {applicationStep === 0 ? (
                  <section className="application-step-pane application-basics-step" aria-labelledby="application-basics-title">
                    <div className="application-step-heading">
                      <div>
                        <h3 data-application-step-heading id="application-basics-title" tabIndex={-1}>{t("dialog.applicationBasics")}</h3>
                      </div>
                      <AppWindow aria-hidden="true" size={19} />
                    </div>
                    {renderFields(applicationIdentityFields, "application-identity-fields")}
                  </section>
                ) : null}
                {applicationStep === 1 ? (
                  <section className="application-step-pane application-media-step" aria-labelledby="application-media-title">
                    <div className="application-step-heading">
                      <div>
                        <h3 data-application-step-heading id="application-media-title" tabIndex={-1}>{t("dialog.applicationMedia")}</h3>
                      </div>
                      <Images aria-hidden="true" size={19} />
                    </div>
                    <ApplicationSubmissionFields
                      compact
                      disabled={busy}
                      errors={mediaErrors}
                      existing={existingStoreListing}
                      locale={locale}
                      onChange={setApplicationSubmission}
                      onErrors={setMediaErrors}
                      submission={applicationSubmission}
                    />
                    <div className="application-listing-subsection">
                      <div className="application-step-heading application-subsection-heading">
                        <h4>{t("dialog.applicationListing")}</h4>
                        <Pencil aria-hidden="true" size={16} />
                      </div>
                      {renderFields(applicationListingFields, "application-listing-fields")}
                    </div>
                  </section>
                ) : null}
                {applicationStep === 2 ? (
                  <section className="application-step-pane application-source-version-step" aria-labelledby="application-source-version-title">
                    <div className="application-step-heading">
                      <div>
                        <h3 data-application-step-heading id="application-source-version-title" tabIndex={-1}>{t("dialog.applicationStepSource")}</h3>
                      </div>
                      <FileArchive aria-hidden="true" size={19} />
                    </div>
                    <div className="application-release-layout">
                      <section className="application-release-settings">
                        {renderFields(applicationSourceFields, "application-release-fields")}
                      </section>
                      {action.sourceInput ? (
                        <section className="application-source-step" aria-labelledby="application-source-title">
                          <div className="application-step-heading application-subsection-heading">
                            <div>
                              <h4 id="application-source-title">{t("dialog.applicationSource")}</h4>
                            </div>
                            <span className="required-mark">{t("dialog.mediaRequired")}</span>
                          </div>
                          <ApplicationSourcePicker
                            busy={busy}
                            files={files}
                            locale={locale}
                            mode={sourceInputMode}
                            onFilesChange={(nextFiles) => {
                              setFiles(nextFiles);
                              setError(undefined);
                            }}
                            onModeChange={(nextMode) => {
                              setSourceInputMode(nextMode);
                              setError(undefined);
                            }}
                            onRepositoryChange={(repository) => {
                              setSourceRepository(repository);
                              setError(undefined);
                            }}
                            repository={sourceRepository}
                          />
                        </section>
                      ) : null}
                    </div>
                  </section>
                ) : null}
                {applicationStep === 3 ? (
                  <section className="application-step-pane application-configuration-step" aria-labelledby="application-configuration-title">
                    <div className="application-step-heading">
                      <div>
                        <h3 data-application-step-heading id="application-configuration-title" tabIndex={-1}>{t("dialog.applicationDeploymentConfig")}</h3>
                      </div>
                      <Settings2 aria-hidden="true" size={19} />
                    </div>
                    {renderFields(applicationConfigurationFields, "application-configuration-fields")}
                  </section>
                ) : null}
                {applicationStep === 4 ? (
                  <ApplicationCreationReview
                    body={body}
                    fieldOptions={fieldOptions}
                    files={files}
                    locale={locale}
                    sourceRepository={sourceRepository}
                    sourceInputMode={sourceInputMode}
                    submission={applicationSubmission}
                  />
                ) : null}
              </div>
              <div className="application-wizard-status" aria-live="polite">
                {busy && (action.requiresFile || action.sourceInput) ? (
                  <div className="upload-progress" role="status">
                    <div>
                      <span>{t("dialog.uploadProgress")}</span>
                      <strong>{progress}%</strong>
                    </div>
                    <progress aria-label={t("dialog.uploadProgress")} max={100} value={progress} />
                  </div>
                ) : null}
                {error ? <div className="error-banner" role="alert">{error}</div> : null}
              </div>
            </div>
          </div>
        ) : null}
        {!result && applicationEditDrawer ? (
          <div className="application-edit-drawer-content">
            <div className="application-edit-drawer-scroll">
              <section className="application-step-pane application-basics-step" aria-labelledby="application-edit-basics-title">
                <div className="application-step-heading">
                  <div>
                    <h3 id="application-edit-basics-title">{t("dialog.applicationBasics")}</h3>
                  </div>
                  <AppWindow aria-hidden="true" size={19} />
                </div>
                {renderFields(applicationIdentityFields, "application-identity-fields")}
              </section>
              <section className="application-step-pane application-media-step" aria-labelledby="application-edit-media-title">
                <div className="application-step-heading">
                  <div>
                    <h3 id="application-edit-media-title">{t("dialog.applicationMedia")}</h3>
                  </div>
                  <Images aria-hidden="true" size={19} />
                </div>
                <ApplicationSubmissionFields
                  compact
                  disabled={busy}
                  errors={mediaErrors}
                  existing={existingStoreListing}
                  locale={locale}
                  onChange={setApplicationSubmission}
                  onErrors={setMediaErrors}
                  submission={applicationSubmission}
                />
                <div className="application-listing-subsection">
                  <div className="application-step-heading application-subsection-heading">
                    <h4>{t("dialog.applicationListing")}</h4>
                    <Pencil aria-hidden="true" size={16} />
                  </div>
                  {renderFields(applicationListingFields, "application-listing-fields")}
                </div>
              </section>
            </div>
            <div className="application-wizard-status" aria-live="polite">
              {error ? <div className="error-banner" role="alert">{error}</div> : null}
            </div>
          </div>
        ) : null}
        {result ? (
          <footer>
            <button className="command-button" onClick={closeDialog} type="button">{t("dialog.close")}</button>
          </footer>
        ) : applicationCreationWizard ? (
          <footer className="application-wizard-footer">
            <button className="secondary-button" disabled={busy && !action.dismissibleWhileBusy} onClick={closeDialog} type="button">{t("dialog.cancel")}</button>
            <div className="application-wizard-navigation">
              {applicationStep > 0 ? (
                <button
                  className="secondary-button"
                  disabled={busy}
                  onClick={() => moveToApplicationStep((applicationStep - 1) as ApplicationWizardStep)}
                  type="button"
                >
                  <ChevronLeft aria-hidden="true" size={16} />
                  {t("dialog.back")}
                </button>
              ) : null}
              {applicationStep < 4 ? (
                <button
                  className="command-button"
                  disabled={busy || optionsBusy}
                  onClick={() => moveToApplicationStep((applicationStep + 1) as ApplicationWizardStep)}
                  type="button"
                >
                  {applicationStep === 3 ? t("dialog.review") : t("dialog.next")}
                  <ChevronRight aria-hidden="true" size={16} />
                </button>
              ) : (
                <button
                  className="command-button"
                  disabled={busy
                    || optionsBusy
                    || !applicationStepReady(0)
                    || !applicationStepReady(1)
                    || !applicationStepReady(2)
                    || !applicationStepReady(3)
                    || hasUnavailableOptions(body, fieldOptions, action.paginatedFields)}
                  type="submit"
                >
                  {busy ? <><LoaderCircle aria-hidden="true" className="is-spinning" size={16} />{t("dialog.submitting")}</> : t("dialog.createApplication")}
                </button>
              )}
            </div>
          </footer>
        ) : applicationEditDrawer ? (
          <footer className="application-edit-drawer-footer">
            <button className="secondary-button" disabled={busy && !action.dismissibleWhileBusy} onClick={closeDialog} type="button">{t("dialog.cancel")}</button>
            <button
              className="command-button"
              disabled={busy
                || optionsBusy
                || !applicationSubmissionReady(applicationSubmission, mediaErrors)
                || hasMissingRequiredFields(body, action.requiredFields)
                || hasUnavailableOptions(body, fieldOptions, action.paginatedFields)}
              type="submit"
            >
              {busy ? <><LoaderCircle aria-hidden="true" className="is-spinning" size={16} />{t("dialog.submitting")}</> : t("dialog.confirm")}
            </button>
          </footer>
        ) : <footer>
          <button className="secondary-button" disabled={busy && !action.dismissibleWhileBusy} onClick={closeDialog} type="button">{t("dialog.cancel")}</button>
          <button
            className={action.dangerous ? "danger-button" : "command-button"}
            disabled={busy
              || optionsBusy
              || Boolean(confirmationRequired && !confirmed)
              || Boolean(action.requiresFile && !file)
              || Boolean(sourceInputError())
              || Boolean(action.applicationSubmission && !applicationSubmissionReady(applicationSubmission, mediaErrors))
              || hasMissingRequiredFields(body, action.requiredFields)
              || hasUnavailableOptions(body, fieldOptions, action.paginatedFields)}
            type="submit"
          >
            {busy ? <><LoaderCircle aria-hidden="true" className="is-spinning" size={16} />{t("dialog.submitting")}</> : sourceUpdateAction ? (
              sourceInputMode === "git" ? (
                <><RefreshCw aria-hidden="true" size={16} />{t("dialog.refreshGitRepository")}</>
              ) : (
                <><Upload aria-hidden="true" size={16} />{t("dialog.uploadNewCode")}</>
              )
            ) : t("dialog.confirm")}
          </button>
        </footer>}
      </form>
    </div>
  );
}

function ApplicationSourcePicker({
  busy,
  files,
  locale,
  mode,
  onFilesChange,
  onModeChange,
  onRepositoryChange,
  repository,
}: {
  busy: boolean;
  files: readonly File[];
  locale: WebserverLocale;
  mode: ApplicationDeploymentSourceMode;
  onFilesChange(files: readonly File[]): void;
  onModeChange(mode: ApplicationDeploymentSourceMode): void;
  onRepositoryChange(repository: string): void;
  repository: string;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const selectionId = useId();
  const t = (key: WebserverMessageKey, values: Record<string, string | number> = {}) => translateWebserver(locale, key, values);

  useEffect(() => {
    const input = inputRef.current;
    if (!input) return;
    if (mode === "directory") {
      input.setAttribute("webkitdirectory", "");
      input.setAttribute("directory", "");
      return;
    }
    input.removeAttribute("webkitdirectory");
    input.removeAttribute("directory");
  }, [mode]);

  return (
    <div className="source-picker">
      <div aria-label={t("dialog.sourceMode")} className="source-mode-toggle" role="group">
        <button
          aria-pressed={mode === "archive"}
          className={mode === "archive" ? "active" : ""}
          disabled={busy}
          onClick={() => {
            onModeChange("archive");
            onFilesChange([]);
          }}
          type="button"
        >
          <FileArchive aria-hidden="true" size={16} />
          {t("dialog.sourceArchive")}
        </button>
        <button
          aria-pressed={mode === "directory"}
          className={mode === "directory" ? "active" : ""}
          disabled={busy}
          onClick={() => {
            onModeChange("directory");
            onFilesChange([]);
          }}
          type="button"
        >
          <FolderOpen aria-hidden="true" size={16} />
          {t("dialog.sourceDirectory")}
        </button>
        <button
          aria-pressed={mode === "git"}
          className={mode === "git" ? "active" : ""}
          disabled={busy}
          onClick={() => {
            onModeChange("git");
            onFilesChange([]);
          }}
          type="button"
        >
          <GitBranch aria-hidden="true" size={16} />
          {t("dialog.sourceGit")}
        </button>
      </div>
      {mode === "git" ? (
        <label className="source-repository-control">
          <span className="source-file-heading">
            <GitBranch aria-hidden="true" size={16} />
            {t("dialog.sourceGitRepository")}
          </span>
          <input
            aria-label={t("dialog.sourceGitRepository")}
            autoComplete="off"
            className="source-repository-input"
            disabled={busy}
            maxLength={APPLICATION_GIT_REPOSITORY_MAX_LENGTH}
            onChange={(event) => onRepositoryChange(event.target.value)}
            placeholder="https://github.com/organization/repository.git"
            spellCheck={false}
            type="url"
            value={repository}
          />
        </label>
      ) : <div className="source-file-control">
        <div className="source-file-heading">
          <Upload aria-hidden="true" size={16} />
          <span>{t("dialog.sourceSelect")}</span>
        </div>
        <div className="source-file-row">
          <button
            className="secondary-button source-file-trigger"
            disabled={busy}
            onClick={() => inputRef.current?.click()}
            type="button"
          >
            {mode === "archive"
              ? <FileArchive aria-hidden="true" size={16} />
              : <FolderOpen aria-hidden="true" size={16} />}
            {mode === "archive"
              ? t("dialog.sourceChooseArchive")
              : t("dialog.sourceChooseDirectory")}
          </button>
          <span className="source-selection" id={selectionId} role="status">
            {files.length === 0
              ? t("dialog.sourceNone")
              : mode === "archive"
                ? files[0].name
                : t("dialog.sourceSelectionCount", { count: files.length })}
          </span>
        </div>
        <input
          accept={mode === "archive" ? ".zip,application/zip" : undefined}
          data-testid="application-source-input"
          disabled={busy}
          hidden
          key={mode}
          multiple={mode === "directory"}
          onChange={(event) => {
            const selectedFiles = Array.from(event.target.files ?? []);
            onFilesChange(mode === "archive" ? selectedFiles.slice(0, 1) : selectedFiles);
          }}
          ref={inputRef}
          type="file"
        />
      </div>}
    </div>
  );
}

function ApplicationWizardProgress({
  currentStep,
  furthestStep,
  locale,
  onStepChange,
}: {
  currentStep: ApplicationWizardStep;
  furthestStep: ApplicationWizardStep;
  locale: WebserverLocale;
  onStepChange(step: ApplicationWizardStep): void;
}) {
  const t = (key: WebserverMessageKey, values: Record<string, string | number> = {}) => translateWebserver(locale, key, values);
  const steps = [
    t("dialog.applicationStepBasics"),
    t("dialog.applicationStepMedia"),
    t("dialog.applicationStepSource"),
    t("dialog.applicationStepConfig"),
    t("dialog.applicationStepReview"),
  ];
  return (
    <nav aria-label={t("dialog.applicationSteps")} className="application-wizard-progress" data-testid="application-wizard-progress">
      <ol>
        {steps.map((step, index) => (
          <li
            className={index === currentStep ? "active" : index < currentStep ? "complete" : index <= furthestStep ? "visited" : undefined}
            key={step}
          >
            <button
              aria-label={`${index + 1}. ${step}`}
              aria-current={index === currentStep ? "step" : undefined}
              disabled={index > furthestStep}
              onClick={() => onStepChange(index as ApplicationWizardStep)}
              type="button"
            >
              <span className="application-wizard-step-number">
                {index < currentStep ? <Check aria-hidden="true" size={14} /> : index + 1}
              </span>
              <span className="application-wizard-step-label">{step}</span>
            </button>
          </li>
        ))}
      </ol>
    </nav>
  );
}

function ApplicationCreationReview({
  body,
  fieldOptions,
  files,
  locale,
  sourceRepository,
  sourceInputMode,
  submission,
}: {
  body: Readonly<Record<string, unknown>>;
  fieldOptions: WebserverResourceFieldOptions;
  files: readonly File[];
  locale: WebserverLocale;
  sourceRepository: string;
  sourceInputMode: ApplicationDeploymentSourceMode;
  submission: ApplicationSubmissionInput;
}) {
  const t = (key: WebserverMessageKey, values: Record<string, string | number> = {}) => translateWebserver(locale, key, values);
  const bodyFields = [
    "name",
    "description",
    "applicationType",
    "siteType",
    "shortDescription",
    "fullDescription",
    "releaseNotes",
    "category",
    "keywords",
    "supportUrl",
    "privacyPolicyUrl",
    "officialWebsiteUrl",
    "versionTag",
    "environment",
    "sourceVersionRetentionLimit",
    "appConfigPath",
    "deploymentConfigPath",
    "publicRoot",
    "spaFallback",
  ];
  const mediaMode = (mode: string): string => {
    switch (mode) {
      case "default":
        return t("dialog.mediaDefault");
      case "upload":
      case "replace":
        return t("dialog.mediaUpload");
      case "keep":
        return t("dialog.mediaKeep");
      case "remove":
        return t("dialog.mediaNone");
      default:
        return mode;
    }
  };
  const displayBodyValue = (field: string): string => {
    const value = body[field];
    const options = fieldOptions[field];
    if (options) {
      const match = options.find((option) => String(optionValue(option)) === String(value));
      if (match !== undefined) return optionLabel(match, field, locale);
    }
    return String(value ?? "-") || "-";
  };
  const sourceSummary = sourceInputMode === "git"
    ? sourceRepository || t("dialog.sourceNone")
    : files.length === 0
      ? t("dialog.sourceNone")
      : sourceInputMode === "archive"
        ? files[0].name
        : t("dialog.sourceSelectionCount", { count: files.length });

  return (
    <section aria-labelledby="application-review-title" className="application-review">
      <div className="application-step-heading">
        <div>
          <h3 data-application-step-heading id="application-review-title" tabIndex={-1}>{t("dialog.applicationReview")}</h3>
        </div>
        <BadgeCheck aria-hidden="true" size={19} />
      </div>
      <dl className="application-review-grid">
        {bodyFields.map((field) => (
          <div className={field === "description" || field === "fullDescription" || field === "releaseNotes" ? "wide" : undefined} data-field={field} key={field}>
            <dt>{fieldLabel(field, locale)}</dt>
            <dd>{displayBodyValue(field)}</dd>
          </div>
        ))}
        <div>
          <dt>{t("dialog.mediaIcon")}</dt>
          <dd>{mediaMode(submission.iconMode)}</dd>
        </div>
        <div>
          <dt>{t("dialog.mediaCover")}</dt>
          <dd>{mediaMode(submission.coverMode)}</dd>
        </div>
        <div>
          <dt>{t("dialog.mediaPreviews")}</dt>
          <dd>{submission.previewsMode === "replace"
            ? t("dialog.mediaSelectedCount", { count: submission.previewFiles.length })
            : mediaMode(submission.previewsMode)}</dd>
        </div>
        <div className="wide">
          <dt>{t("dialog.applicationSource")}</dt>
          <dd>{sourceSummary}</dd>
        </div>
      </dl>
    </section>
  );
}

function ApplicationSubmissionFields({
  compact = false,
  disabled,
  errors,
  existing,
  locale,
  onChange,
  onErrors,
  submission,
}: {
  compact?: boolean;
  disabled: boolean;
  errors: ApplicationMediaFieldErrors;
  existing?: ApplicationStoreListingInput;
  locale: WebserverLocale;
  onChange(value: ApplicationSubmissionInput): void;
  onErrors(value: ApplicationMediaFieldErrors): void;
  submission: ApplicationSubmissionInput;
}) {
  const validationVersion = useRef(0);
  const t = (key: WebserverMessageKey, values: Record<string, string | number> = {}) => translateWebserver(locale, key, values);
  const setFieldError = (field: keyof ApplicationMediaFieldErrors, value: string | undefined): void => {
    onErrors({ ...errors, [field]: value });
  };
  const iconPreview = useFilePreview(submission.iconFile);
  const coverPreview = useFilePreview(submission.coverFile);
  const previewImages = useFilePreviews(submission.previewFiles);
  const iconInputRef = useRef<HTMLInputElement>(null);
  const coverInputRef = useRef<HTMLInputElement>(null);
  const previewsInputRef = useRef<HTMLInputElement>(null);

  async function selectFile(role: "icon" | "cover", file: File | undefined): Promise<void> {
    const version = ++validationVersion.current;
    onChange({
      ...submission,
      ...(role === "icon" ? { iconFile: file, iconMode: "upload" as const } : { coverFile: file, coverMode: "upload" as const }),
    });
    if (!file) {
      setFieldError(role, t(role === "icon" ? "dialog.mediaIconRequired" : "dialog.mediaCoverRequired"));
      return;
    }
    setFieldError(role, t("dialog.mediaValidating"));
    try {
      await validateApplicationMediaFile(role, file);
      if (validationVersion.current === version) setFieldError(role, undefined);
    } catch (error) {
      if (validationVersion.current === version) setFieldError(role, mediaValidationMessage(error, t));
    }
  }

  async function selectPreviews(files: readonly File[], append = false): Promise<void> {
    const currentFiles = append && submission.previewsMode === "replace" ? submission.previewFiles : [];
    const nextFiles = mergeApplicationPreviewFiles(currentFiles, files);
    const version = ++validationVersion.current;
    setFieldError("previews", t("dialog.mediaValidating"));
    try {
      validateApplicationPreviewCount(nextFiles);
      if (nextFiles.length === 0) throw new Error("PREVIEW_REQUIRED");
      for (const file of files) {
        try {
          await validateApplicationMediaFile("preview", file);
        } catch (error) {
          const finalIndex = nextFiles.findIndex((candidate) => candidate === file);
          if (validationVersion.current === version) {
            setFieldError("previews", mediaValidationMessage(error, t, finalIndex >= 0 ? finalIndex : undefined));
          }
          return;
        }
      }
      if (validationVersion.current === version) {
        onChange({ ...submission, previewFiles: nextFiles, previewsMode: "replace" });
        setFieldError("previews", undefined);
      }
    } catch (error) {
      if (validationVersion.current === version) setFieldError("previews", mediaValidationMessage(error, t));
    }
  }

  function removePreview(index: number): void {
    validationVersion.current += 1;
    setFieldError("previews", undefined);
    const previewFiles = submission.previewFiles.filter((_, candidateIndex) => candidateIndex !== index);
    onChange({
      ...submission,
      previewFiles,
      previewsMode: previewFiles.length > 0 ? "replace" : "remove",
    });
  }

  function movePreview(index: number, offset: -1 | 1): void {
    const targetIndex = index + offset;
    if (targetIndex < 0 || targetIndex >= submission.previewFiles.length) return;
    validationVersion.current += 1;
    setFieldError("previews", undefined);
    const previewFiles = [...submission.previewFiles];
    [previewFiles[index], previewFiles[targetIndex]] = [previewFiles[targetIndex], previewFiles[index]];
    onChange({ ...submission, previewFiles, previewsMode: "replace" });
  }

  function removePrimaryMedia(role: "icon" | "cover"): void {
    validationVersion.current += 1;
    setFieldError(role, undefined);
    onChange({
      ...submission,
      ...(role === "icon"
        ? { iconFile: undefined, iconMode: "default" as const }
        : { coverFile: undefined, coverMode: "remove" as const }),
    });
  }

  if (compact) {
    const chooseMedia = (role: "icon" | "cover" | "previews"): void => {
      if (role === "icon") {
        queueMicrotask(() => iconInputRef.current?.click());
        return;
      }
      if (role === "cover") {
        queueMicrotask(() => coverInputRef.current?.click());
        return;
      }
      queueMicrotask(() => previewsInputRef.current?.click());
    };

    return (
      <section aria-labelledby="application-store-assets" className="application-store-assets application-store-assets-compact">
        <div className="application-store-heading">
          <div>
            <h3 id="application-store-assets">{t("dialog.mediaTitle")}</h3>
          </div>
          <BadgeCheck aria-hidden="true" size={18} />
        </div>
        <div className="application-media-primary-grid">
          <section className="application-media-flat-field">
            <div className="application-media-flat-heading">
              <Image aria-hidden="true" size={17} />
              <div>
                <strong>{t("dialog.mediaIcon")}</strong>
                <small>1:1 PNG</small>
              </div>
              <span className="required-mark">{t("dialog.mediaRequired")}</span>
            </div>
            <div className="application-media-placeholder-wrap application-media-placeholder-wrap-icon">
              <button
                aria-label={t("dialog.mediaIconUpload")}
                className={`application-media-flat-preview application-media-flat-preview-icon${iconPreview ? " has-image" : ""}`}
                disabled={disabled}
                onClick={() => chooseMedia("icon")}
                title={t("dialog.mediaIconUpload")}
                type="button"
              >
                {iconPreview ? <img alt={t("dialog.mediaIcon")} src={iconPreview} /> : submission.iconMode === "keep" ? <BadgeCheck aria-hidden="true" size={22} /> : <ImagePlus aria-hidden="true" size={22} />}
                {iconPreview ? null : <span>{submission.iconMode === "keep" ? t("dialog.mediaKeep") : t("dialog.mediaUpload")}</span>}
              </button>
              {submission.iconFile ? (
                <button
                  aria-label={t("dialog.mediaIconRemove")}
                  className="application-media-remove"
                  disabled={disabled}
                  onClick={() => removePrimaryMedia("icon")}
                  title={t("dialog.mediaIconRemove")}
                  type="button"
                >
                  <Trash2 aria-hidden="true" size={14} />
                </button>
              ) : null}
            </div>
            <input
              accept="image/png"
              aria-label={t("dialog.mediaIcon")}
              data-testid="application-icon-input"
              disabled={disabled}
              hidden
              onChange={(event) => {
                const file = event.target.files?.[0];
                event.target.value = "";
                void selectFile("icon", file);
              }}
              ref={iconInputRef}
              type="file"
            />
            {errors.icon ? <div className="media-validation" role="alert">{errors.icon}</div> : null}
          </section>
          <section className="application-media-flat-field">
            <div className="application-media-flat-heading">
              <Image aria-hidden="true" size={17} />
              <div>
                <strong>{t("dialog.mediaCover")}</strong>
                <small>1024 x 500 PNG, JPEG, WebP</small>
              </div>
            </div>
            <div className="application-media-placeholder-wrap application-media-placeholder-wrap-cover">
              <button
                aria-label={t("dialog.mediaCoverUpload")}
                className={`application-media-flat-preview application-media-flat-preview-cover${coverPreview ? " has-image" : ""}`}
                disabled={disabled}
                onClick={() => chooseMedia("cover")}
                title={t("dialog.mediaCoverUpload")}
                type="button"
              >
                {coverPreview ? <img alt={t("dialog.mediaCover")} src={coverPreview} /> : submission.coverMode === "keep" ? <BadgeCheck aria-hidden="true" size={22} /> : <ImagePlus aria-hidden="true" size={22} />}
                {coverPreview ? null : <span>{submission.coverMode === "keep" ? t("dialog.mediaKeep") : t("dialog.mediaUpload")}</span>}
              </button>
              {submission.coverFile ? (
                <button
                  aria-label={t("dialog.mediaCoverRemove")}
                  className="application-media-remove"
                  disabled={disabled}
                  onClick={() => removePrimaryMedia("cover")}
                  title={t("dialog.mediaCoverRemove")}
                  type="button"
                >
                  <Trash2 aria-hidden="true" size={14} />
                </button>
              ) : null}
            </div>
            <input
              accept="image/png,image/jpeg,image/webp"
              aria-label={t("dialog.mediaCover")}
              data-testid="application-cover-input"
              disabled={disabled}
              hidden
              onChange={(event) => {
                const file = event.target.files?.[0];
                event.target.value = "";
                void selectFile("cover", file);
              }}
              ref={coverInputRef}
              type="file"
            />
            {errors.cover ? <div className="media-validation" role="alert">{errors.cover}</div> : null}
          </section>
        </div>
        <section className="application-preview-manager">
          <div className="application-preview-manager-heading">
            <div className="application-media-flat-heading">
              <Images aria-hidden="true" size={17} />
              <div>
                <strong>{t("dialog.mediaPreviews")}</strong>
                <small>{t("dialog.mediaPreviewLimit")}</small>
              </div>
            </div>
            <div className="application-preview-manager-actions">
              <span className="application-preview-count" aria-live="polite">
                {t("dialog.mediaPreviewCountStatus", { count: submission.previewFiles.length, limit: APPLICATION_PREVIEW_LIMIT })}
              </span>
              <div aria-label={t("dialog.mediaPreviewsMode")} className="media-mode-toggle" role="group">
                {existing?.previews?.length ? <MediaModeButton active={submission.previewsMode === "keep"} disabled={disabled} icon={<BadgeCheck aria-hidden="true" size={15} />} label={t("dialog.mediaKeep")} onClick={() => { validationVersion.current += 1; setFieldError("previews", undefined); onChange({ ...submission, previewFiles: [], previewsMode: "keep" }); }} /> : null}
                <MediaModeButton active={submission.previewsMode === "remove"} disabled={disabled} icon={<X aria-hidden="true" size={15} />} label={t("dialog.mediaNone")} onClick={() => { validationVersion.current += 1; setFieldError("previews", undefined); onChange({ ...submission, previewFiles: [], previewsMode: "remove" }); }} />
              </div>
            </div>
          </div>
          <div aria-label={t("dialog.mediaPreviews")} className="application-preview-strip" role="list">
            {previewImages.map((url, index) => (
              <article
                aria-label={submission.previewFiles[index]?.name}
                className="application-preview-item"
                key={`${url}-${index}`}
                role="listitem"
              >
                <img alt="" src={url} />
                <span className="application-preview-sequence">{index + 1}</span>
                <div className="application-preview-item-actions">
                  <button
                    aria-label={t("dialog.mediaPreviewMoveBefore", { index: index + 1 })}
                    disabled={disabled || index === 0}
                    onClick={() => movePreview(index, -1)}
                    title={t("dialog.mediaPreviewMoveBefore", { index: index + 1 })}
                    type="button"
                  >
                    <ChevronLeft aria-hidden="true" size={14} />
                  </button>
                  <button
                    aria-label={t("dialog.mediaPreviewMoveAfter", { index: index + 1 })}
                    disabled={disabled || index === previewImages.length - 1}
                    onClick={() => movePreview(index, 1)}
                    title={t("dialog.mediaPreviewMoveAfter", { index: index + 1 })}
                    type="button"
                  >
                    <ChevronRight aria-hidden="true" size={14} />
                  </button>
                  <button
                    aria-label={t("dialog.mediaPreviewRemove", { index: index + 1 })}
                    disabled={disabled}
                    onClick={() => removePreview(index)}
                    title={t("dialog.mediaPreviewRemove", { index: index + 1 })}
                    type="button"
                  >
                    <Trash2 aria-hidden="true" size={14} />
                  </button>
                </div>
              </article>
            ))}
            {submission.previewsMode === "keep" ? (
              <div className="application-preview-stored" role="listitem">
                <BadgeCheck aria-hidden="true" size={19} />
                <span>{t("dialog.mediaStoredCount", { count: existing?.previews?.length ?? 0 })}</span>
              </div>
            ) : null}
            {submission.previewFiles.length < APPLICATION_PREVIEW_LIMIT ? (
              <button
                aria-label={t("dialog.mediaPreviewAdd")}
                className="application-preview-add"
                disabled={disabled}
                onClick={() => chooseMedia("previews")}
                title={t("dialog.mediaPreviewAdd")}
                type="button"
              >
                <Plus aria-hidden="true" size={20} />
                <span>{t("dialog.mediaPreviewAdd")}</span>
              </button>
            ) : null}
          </div>
          <input
            accept="image/png,image/jpeg,image/webp"
            aria-label={t("dialog.mediaPreviews")}
            data-testid="application-preview-input"
            disabled={disabled}
            hidden
            multiple
            onChange={(event) => {
              const selectedFiles = Array.from(event.target.files ?? []);
              event.target.value = "";
              void selectPreviews(selectedFiles, true);
            }}
            ref={previewsInputRef}
            type="file"
          />
          {errors.previews ? <div className="media-validation" role="alert">{errors.previews}</div> : null}
        </section>
      </section>
    );
  }

  return (
    <section aria-labelledby="application-store-assets" className="application-store-assets">
      <div className="application-store-heading">
        <div>
          <h3 id="application-store-assets">{t("dialog.mediaTitle")}</h3>
          <span>{t("dialog.mediaSubtitle")}</span>
        </div>
        <BadgeCheck aria-hidden="true" size={19} />
      </div>
      <div className="application-media-field">
        <div className="application-media-label">
          <Image aria-hidden="true" size={17} />
          <strong>{t("dialog.mediaIcon")}</strong>
          <span className="required-mark">{t("dialog.mediaRequired")}</span>
          <small>1:1 PNG</small>
        </div>
        <div aria-label={t("dialog.mediaIconMode")} className="media-mode-toggle" role="group">
          {existing?.icon ? (
            <MediaModeButton
              active={submission.iconMode === "keep"}
              disabled={disabled}
              icon={<BadgeCheck aria-hidden="true" size={15} />}
              label={t("dialog.mediaKeep")}
              onClick={() => {
                validationVersion.current += 1;
                setFieldError("icon", undefined);
                onChange({ ...submission, iconFile: undefined, iconMode: "keep" });
              }}
            />
          ) : null}
          <MediaModeButton
            active={submission.iconMode === "default"}
            disabled={disabled}
            icon={<WandSparkles aria-hidden="true" size={15} />}
            label={t("dialog.mediaDefault")}
            onClick={() => {
              validationVersion.current += 1;
              setFieldError("icon", undefined);
              onChange({ ...submission, iconFile: undefined, iconMode: "default" });
            }}
          />
          <MediaModeButton
            active={submission.iconMode === "upload"}
            disabled={disabled}
            icon={<Upload aria-hidden="true" size={15} />}
            label={t("dialog.mediaUpload")}
            onClick={() => onChange({ ...submission, iconMode: "upload" })}
          />
        </div>
        {submission.iconMode === "default" ? (
          <div className="default-icon-preview" aria-label={t("dialog.mediaDefaultPreview")}>
            <WandSparkles aria-hidden="true" size={22} />
          </div>
        ) : null}
        {submission.iconMode === "keep" ? <div className="stored-media-state"><BadgeCheck aria-hidden="true" size={16} />{t("dialog.mediaStored")}</div> : null}
        {submission.iconMode === "upload" ? (
          <label className="media-file-input">
            <span>{t("dialog.mediaIcon")}</span>
            <input
              accept="image/png"
              aria-label={t("dialog.mediaIcon")}
              disabled={disabled}
              onChange={(event) => void selectFile("icon", event.target.files?.[0])}
              type="file"
            />
            {iconPreview ? <img alt="" src={iconPreview} /> : null}
          </label>
        ) : null}
        {errors.icon ? <div className="media-validation" role="alert">{errors.icon}</div> : null}
      </div>
      <div className="application-media-field">
        <div className="application-media-label">
          <Image aria-hidden="true" size={17} />
          <strong>{t("dialog.mediaCover")}</strong>
          <small>1024 x 500 PNG, JPEG, WebP</small>
        </div>
        <div aria-label={t("dialog.mediaCoverMode")} className="media-mode-toggle" role="group">
          {existing?.cover ? <MediaModeButton active={submission.coverMode === "keep"} disabled={disabled} icon={<BadgeCheck aria-hidden="true" size={15} />} label={t("dialog.mediaKeep")} onClick={() => { validationVersion.current += 1; setFieldError("cover", undefined); onChange({ ...submission, coverFile: undefined, coverMode: "keep" }); }} /> : null}
          <MediaModeButton active={submission.coverMode === "remove"} disabled={disabled} icon={<X aria-hidden="true" size={15} />} label={t("dialog.mediaNone")} onClick={() => { validationVersion.current += 1; setFieldError("cover", undefined); onChange({ ...submission, coverFile: undefined, coverMode: "remove" }); }} />
          <MediaModeButton active={submission.coverMode === "upload"} disabled={disabled} icon={<Upload aria-hidden="true" size={15} />} label={t("dialog.mediaUpload")} onClick={() => onChange({ ...submission, coverMode: "upload" })} />
        </div>
        {submission.coverMode === "keep" ? <div className="stored-media-state"><BadgeCheck aria-hidden="true" size={16} />{t("dialog.mediaStored")}</div> : null}
        {submission.coverMode === "upload" ? (
          <label className="media-file-input media-file-input-wide">
            <span>{t("dialog.mediaCover")}</span>
            <input accept="image/png,image/jpeg,image/webp" aria-label={t("dialog.mediaCover")} disabled={disabled} onChange={(event) => void selectFile("cover", event.target.files?.[0])} type="file" />
            {coverPreview ? <img alt="" src={coverPreview} /> : null}
          </label>
        ) : null}
        {errors.cover ? <div className="media-validation" role="alert">{errors.cover}</div> : null}
      </div>
      <div className="application-media-field">
        <div className="application-media-label">
          <Images aria-hidden="true" size={17} />
          <strong>{t("dialog.mediaPreviews")}</strong>
          <small>{t("dialog.mediaPreviewLimit")}</small>
        </div>
        <div aria-label={t("dialog.mediaPreviewsMode")} className="media-mode-toggle" role="group">
          {existing?.previews?.length ? <MediaModeButton active={submission.previewsMode === "keep"} disabled={disabled} icon={<BadgeCheck aria-hidden="true" size={15} />} label={t("dialog.mediaKeep")} onClick={() => { validationVersion.current += 1; setFieldError("previews", undefined); onChange({ ...submission, previewFiles: [], previewsMode: "keep" }); }} /> : null}
          <MediaModeButton active={submission.previewsMode === "remove"} disabled={disabled} icon={<X aria-hidden="true" size={15} />} label={t("dialog.mediaNone")} onClick={() => { validationVersion.current += 1; setFieldError("previews", undefined); onChange({ ...submission, previewFiles: [], previewsMode: "remove" }); }} />
          <MediaModeButton active={submission.previewsMode === "replace"} disabled={disabled} icon={<Upload aria-hidden="true" size={15} />} label={t("dialog.mediaUpload") } onClick={() => onChange({ ...submission, previewsMode: "replace" })} />
        </div>
        {submission.previewsMode === "keep" ? <div className="stored-media-state"><BadgeCheck aria-hidden="true" size={16} />{t("dialog.mediaStoredCount", { count: existing?.previews?.length ?? 0 })}</div> : null}
        {submission.previewsMode === "replace" ? (
          <label className="media-file-input media-file-input-wide">
            <span>{t("dialog.mediaPreviews")}</span>
            <input accept="image/png,image/jpeg,image/webp" aria-label={t("dialog.mediaPreviews")} disabled={disabled} multiple onChange={(event) => void selectPreviews(Array.from(event.target.files ?? []))} type="file" />
            {previewImages.length ? <div className="media-preview-grid">{previewImages.map((url, index) => <img alt="" key={`${url}-${index}`} src={url} />)}</div> : null}
          </label>
        ) : null}
        {errors.previews ? <div className="media-validation" role="alert">{errors.previews}</div> : null}
      </div>
    </section>
  );
}

function MediaModeButton({ active, disabled, icon, label, onClick }: { active: boolean; disabled: boolean; icon: ReactNode; label: string; onClick(): void }) {
  return (
    <button
      aria-label={label}
      aria-pressed={active}
      className={active ? "active" : ""}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

function mergeApplicationPreviewFiles(
  currentFiles: readonly File[],
  selectedFiles: readonly File[],
): readonly File[] {
  const identities = new Set(currentFiles.map(applicationPreviewFileIdentity));
  return [
    ...currentFiles,
    ...selectedFiles.filter((file) => {
      const identity = applicationPreviewFileIdentity(file);
      if (identities.has(identity)) return false;
      identities.add(identity);
      return true;
    }),
  ];
}

function applicationPreviewFileIdentity(file: File): string {
  return `${file.name}:${file.size}:${file.type}:${file.lastModified}`;
}

function useFilePreview(file: File | undefined): string | undefined {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    if (!file || !URL.createObjectURL) {
      setUrl(undefined);
      return undefined;
    }
    const next = URL.createObjectURL(file);
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [file]);
  return url;
}

function useFilePreviews(files: readonly File[]): readonly string[] {
  const [urls, setUrls] = useState<readonly string[]>([]);
  useEffect(() => {
    if (!URL.createObjectURL) {
      setUrls([]);
      return undefined;
    }
    const next = files.map((file) => URL.createObjectURL(file));
    setUrls(next);
    return () => next.forEach((url) => URL.revokeObjectURL(url));
  }, [files]);
  return urls;
}

function mediaValidationMessage(
  error: unknown,
  t: (key: WebserverMessageKey, values?: Record<string, string | number>) => string,
  previewIndex?: number,
): string {
  const code = error instanceof Error ? error.message : "";
  const details = error instanceof ApplicationMediaValidationError ? error.details : undefined;
  const keys: Record<string, WebserverMessageKey> = {
    COVER_DIMENSIONS: "dialog.mediaCoverDimensions",
    ICON_DIMENSIONS: "dialog.mediaIconDimensions",
    ICON_SIZE: "dialog.mediaIconSize",
    ICON_TYPE: "dialog.mediaIconType",
    IMAGE_DECODE: "dialog.mediaDecode",
    IMAGE_INSPECTION_UNAVAILABLE: "dialog.mediaDecode",
    PREVIEW_COUNT: "dialog.mediaPreviewCount",
    PREVIEW_DIMENSIONS: "dialog.mediaPreviewDimensions",
    PREVIEW_REQUIRED: "dialog.mediaPreviewRequired",
    STORE_IMAGE_SIZE: "dialog.mediaImageSize",
    STORE_IMAGE_TYPE: "dialog.mediaImageType",
  };
  const key = keys[code] ?? "dialog.mediaDecode";
  let message = code === "PREVIEW_COUNT"
    ? t(key, { count: details?.count ?? 0 })
    : t(key);
  if (details) {
    if (code === "ICON_DIMENSIONS" || code === "COVER_DIMENSIONS" || code === "PREVIEW_DIMENSIONS") {
      message += t("dialog.mediaActualDimensions", {
        width: details.width ?? 0,
        height: details.height ?? 0,
      });
    } else if (code === "ICON_SIZE" || code === "STORE_IMAGE_SIZE") {
      message += t("dialog.mediaActualSize", { size: formatBytes(details.actualBytes ?? 0) });
    } else if (code === "ICON_TYPE" || code === "STORE_IMAGE_TYPE") {
      message += t("dialog.mediaActualType", { type: String(details.actualType ?? "") });
    }
  }
  if (previewIndex !== undefined) {
    message = `${t("dialog.mediaPreviewOrdinal", { index: previewIndex + 1 })}${message}`;
  }
  return message;
}

function applicationSubmissionReady(submission: ApplicationSubmissionInput, mediaErrors: ApplicationMediaFieldErrors): boolean {
  return !mediaErrors.icon
    && !mediaErrors.cover
    && !mediaErrors.previews
    && (submission.iconMode !== "upload" || Boolean(submission.iconFile))
    && (submission.coverMode !== "upload" || Boolean(submission.coverFile))
    && (submission.previewsMode !== "replace" || submission.previewFiles.length > 0);
}

function trapDialogFocus(event: ReactKeyboardEvent<HTMLElement>, dialog: HTMLElement): void {
  if (event.key !== "Tab") return;
  const focusable = Array.from(dialog.querySelectorAll<HTMLElement>(
    "button:not([disabled]), input:not([disabled]):not([tabindex='-1']), select:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex='-1'])",
  )).filter((element) => element.getAttribute("aria-hidden") !== "true");
  if (focusable.length === 0) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  } else if (!dialog.contains(document.activeElement)) {
    event.preventDefault();
    first.focus();
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
  scopeId,
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
  scopeId?: string;
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
  const contextRef = useRef({ actionBody, scopeId, selectedItem });
  contextRef.current = { actionBody, scopeId, selectedItem };
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
      scopeId: context.scopeId,
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
  }, [loadPage, name, page, requestVersion, scopeId, selectedItem]);

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

interface ScopeOption {
  id: string;
  label: string;
}

function scopeOption(
  item: Record<string, unknown>,
  scopeKind: "application",
): ScopeOption | undefined {
  const rawId = item.id
    ?? item.applicationId;
  if (typeof rawId !== "string" && typeof rawId !== "number") return undefined;
  const id = String(rawId);
  const rawLabel = item.name ?? item.slug ?? item.hostname;
  const label = typeof rawLabel === "string" && rawLabel.trim()
    ? `${rawLabel.trim()} (${id})`
    : id;
  return { id, label };
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

function actionText(
  t: (key: WebserverMessageKey) => string,
  resource: WebserverResourceKey,
  action: WebserverResourceAction,
): string {
  const key = `action.${resource}.${action.id}` as WebserverMessageKey;
  try {
    return t(key);
  } catch {
    return action.label;
  }
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

function recordLabel(item: Record<string, unknown>, index: number): string {
  const value = item.name ?? item.slug ?? item.id ?? item.applicationId;
  return typeof value === "string" || typeof value === "number"
    ? String(value)
    : String(index + 1);
}

function displayValue(value: unknown, column: string, resource: WebserverResourceKey, locale: WebserverLocale): ReactNode {
  if (value === null || value === undefined) return "-";
  if (resource === "applications" && column === "status") {
    return <span className={`status-badge application-status-${String(value).toLowerCase()}`}>{applicationStatus(value, locale)}</span>;
  }
  if (resource === "servers" && column === "status") {
    return <span className={`status-badge server-status-${String(value).toLowerCase()}`}>{serverStatus(value, locale)}</span>;
  }
  if ((resource === "deployments" || resource === "application-deployments") && column === "status") {
    const label = deploymentStatus(value, locale);
    return <span className={`status-badge deployment-status-${String(value).toLowerCase()}`}>{label}</span>;
  }
  if ((resource === "source-versions" || resource === "application-source-versions") && column === "status") {
    return <span className={`status-badge source-version-status-${String(value).toLowerCase()}`}>{sourceVersionStatus(value, locale)}</span>;
  }
  if (column === "artifactSize") return formatBytes(value);
  if (column === "durationMs") return formatDuration(value);
  if (column === "configSnapshot" && isRecord(value)) {
    const appConfigDetected = value.appConfigDetected === true;
    const deploymentConfigDetected = value.deploymentConfigDetected === true;
    const detected = Number(appConfigDetected) + Number(deploymentConfigDetected);
    const label = locale === "zh-CN" ? `已发现 ${detected}/2` : `${detected}/2 detected`;
    return <span title={JSON.stringify(value)}>{label}</span>;
  }
  if (column === "artifactHash" && typeof value === "string") {
    return <span title={value}>{value.length > 16 ? `${value.slice(0, 12)}...${value.slice(-4)}` : value}</span>;
  }
  if (column === "rollbackFromDeploymentId" && typeof value === "string") {
    return <span title={value}>{value.length > 18 ? `${value.slice(0, 8)}...${value.slice(-6)}` : value}</span>;
  }
  if (column === "sourceVersionId" && typeof value === "string") {
    return <span title={value}>{value.length > 18 ? `${value.slice(0, 8)}...${value.slice(-6)}` : value}</span>;
  }
  if (column === "artifactDriveUri" && typeof value === "string") {
    const nodeId = value.split("/nodes/")[1];
    return <span title={value}>{nodeId ? `Drive / ${nodeId}` : value}</span>;
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
  scopeId: string,
): boolean {
  return action.availableWhen?.({ body: action.bodyTemplate, scopeId: scopeId || undefined, selectedItem })
    ?? true;
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
      "applicationType:API": "API service",
      "applicationType:WEB": "Web application",
      "certType:1": "Let's Encrypt",
      "certType:2": "Custom certificate",
      "certType:3": "Self-signed certificate",
      "deployType:1": "Manual package",
      "deployType:2": "Git",
      "deployType:3": "CI/CD",
      "deployType:4": "API",
      "siteType:1": "Static site",
      "siteType:2": "Single-page application (SPA)",
      "siteType:3": "Node.js",
      "siteType:4": "PHP",
      "siteType:5": "Python",
      "siteType:6": "Other",
      "environment:development": "Development",
      "environment:production": "Production",
      "environment:staging": "Staging",
      "environment:test": "Test",
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
      "applicationType:API": "API 服务",
      "applicationType:WEB": "Web 应用",
      "certType:1": "Let's Encrypt",
      "certType:2": "自定义证书",
      "certType:3": "自签名证书",
      "deployType:1": "手动上传",
      "deployType:2": "Git",
      "deployType:3": "CI/CD",
      "deployType:4": "API",
      "siteType:1": "静态站点",
      "siteType:2": "单页应用（SPA）",
      "siteType:3": "Node.js",
      "siteType:4": "PHP",
      "siteType:5": "Python",
      "siteType:6": "其他",
      "environment:development": "开发环境",
      "environment:production": "生产环境",
      "environment:staging": "预发布环境",
      "environment:test": "测试环境",
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
  const listing = action.applicationSubmission ? storeListingBody(selected?.storeListing) : {};
  return Object.fromEntries(
    Object.entries(action.bodyTemplate).map(([field, fallback]) => [
      field,
      selected?.[field] !== undefined ? selected[field] : listing[field] ?? fallback,
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
    applications: ["id", "name", "applicationType", "siteType", "status", "updatedAt", "createdAt"],
    deployments: ["versionTag", "environment", "status", "rollbackFromDeploymentId", "artifactHash", "createdAt", "startedAt", "completedAt", "durationMs"],
    "source-versions": ["versionTag", "sourceType", "retained", "configSnapshot", "artifactSize", "artifactHash", "status", "createdAt"],
    "application-source-versions": ["versionTag", "sourceType", "retained", "configSnapshot", "artifactSize", "artifactHash", "status", "createdAt"],
    "application-deployments": ["versionTag", "environment", "status", "rollbackFromDeploymentId", "artifactHash", "createdAt", "startedAt", "completedAt", "durationMs"],
    nginx: ["id", "configName", "configType", "isActive", "status", "versionNo", "deployedAt", "updatedAt"],
    servers: ["id", "name", "host", "sshPort", "status", "lastHeartbeatAt", "createdAt"],
    audit: ["operatorId", "operatorType", "action", "targetType", "targetUuid", "ipAddress", "createdAt"],
  };
  const ordered = [
    ...(preferred[resource] ?? []).filter((column) => available.includes(column)),
    ...available.filter((column) => !(preferred[resource] ?? []).includes(column)),
  ];
  return ordered.slice(0, resource === "deployments" || resource === "application-deployments" ? 9 : 8);
}

function applicationStatus(value: unknown, locale: WebserverLocale): string {
  const statuses: Record<WebserverLocale, Record<string, string>> = {
    "en-US": { "0": "Draft", "1": "Active", "2": "Disabled" },
    "zh-CN": { "0": "草稿", "1": "运行中", "2": "已停用" },
  };
  return statuses[locale][String(value)] ?? String(value);
}

function serverStatus(value: unknown, locale: WebserverLocale): string {
  const statuses: Record<WebserverLocale, Record<string, string>> = {
    "en-US": { "0": "Offline", "1": "Online" },
    "zh-CN": { "0": "离线", "1": "在线" },
  };
  return statuses[locale][String(value)] ?? String(value);
}

function deploymentStatus(value: unknown, locale: WebserverLocale): string {
  const statuses: Record<WebserverLocale, Record<string, string>> = {
    "en-US": {
      "0": "Pending",
      "1": "Deploying",
      "2": "Succeeded",
      "3": "Failed",
      "4": "Rolled back",
      "5": "Rollback source",
      "6": "Cancelled",
    },
    "zh-CN": {
      "0": "待处理",
      "1": "发布中",
      "2": "已成功",
      "3": "发布失败",
      "4": "已回滚",
      "5": "回滚源版本",
      "6": "已取消",
    },
  };
  return statuses[locale][String(value)] ?? String(value);
}

function sourceVersionStatus(value: unknown, locale: WebserverLocale): string {
  const statuses: Record<WebserverLocale, Record<string, string>> = {
    "en-US": { "0": "Processing", "1": "Ready", "2": "Failed", "3": "Pruned" },
    "zh-CN": { "0": "处理中", "1": "可发布", "2": "存储失败", "3": "已清理" },
  };
  return statuses[locale][String(value)] ?? String(value);
}

function formatBytes(value: unknown): string {
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes < 0) return String(value);
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let amount = bytes;
  let unit = -1;
  do {
    amount /= 1024;
    unit += 1;
  } while (amount >= 1024 && unit < units.length - 1);
  return `${amount >= 10 ? amount.toFixed(1) : amount.toFixed(2)} ${units[unit]}`;
}

function formatDuration(value: unknown): string {
  const milliseconds = Number(value);
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return String(value);
  return milliseconds < 1000 ? `${milliseconds} ms` : `${(milliseconds / 1000).toFixed(1)} s`;
}
