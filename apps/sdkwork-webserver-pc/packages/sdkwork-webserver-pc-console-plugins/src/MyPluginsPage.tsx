import { useEffect, useMemo, useState } from "react";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import type { SdkworkDriveAppClient } from "@sdkwork/webserver-pc-console-core";
import { CreatePluginForm } from "./CreatePluginForm.tsx";
import { EditPluginForm } from "./EditPluginForm.tsx";
import { usePluginsT } from "./locale.tsx";
import {
  EMPTY_PLUGIN_LIST_FILTERS,
  filterPluginRecords,
  hasActivePluginFilters,
} from "./plugin-filter.ts";
import {
  filterPluginRecordsByOwner,
  loadPluginCatalog,
  removePluginRecord,
  savePluginCatalog,
  upsertPluginRecord,
} from "./plugin-catalog.ts";
import { loadPluginCategories } from "./plugin-category-catalog.ts";
import {
  findPluginCategory,
  type PluginCategoryRecord,
} from "./plugin-category.ts";
import { normalizePluginOwnerKey, type PluginRecord } from "./plugin-model.ts";
import { PluginListFilterBar, PluginToolBadges } from "./PluginToolMultiSelect.tsx";
import { ConfirmModal, SurfaceDrawer } from "./SurfaceOverlay.tsx";

type DrawerState = { kind: "create" } | { kind: "edit"; plugin: PluginRecord } | null;

/**
 * Admin scope selector. `User` is the same isolation boundary the console
 * surface enforces; `All` is the operator-only cross-user review view.
 */
export type PluginsAdminScope = "All" | "User";

/** Plugin catalog is fully read into memory ⇒ client-side paging. */
const PLUGIN_PAGE_SIZES = [20, 50, 100] as const;

function formatSourceDetail(item: PluginRecord, t: (key: "mine.source.git" | "mine.source.archive") => string): string {
  if (item.sourceKind === "git") {
    const ref = item.gitRef ? ` @ ${item.gitRef}` : "";
    return `${item.gitRepository ?? t("mine.source.git")}${ref}`;
  }
  return item.archiveFileName || item.artifactRef || t("mine.source.archive");
}

export function MyPluginsPage({
  drive,
  ownerKey,
  variant = "console",
}: {
  drive: SdkworkDriveAppClient;
  /**
   * The IAM subject owning this session's plugin catalog. Every read/write is
   * scoped to it so each user gets their own plugin CRUD space.
   */
  ownerKey: string;
  variant?: "admin" | "console";
}) {
  const t = usePluginsT();
  const owner = normalizePluginOwnerKey(ownerKey);
  const isAdmin = variant === "admin";
  const [items, setItems] = useState<PluginRecord[]>([]);
  const [categories, setCategories] = useState<PluginCategoryRecord[]>([]);
  const [scope, setScope] = useState<PluginsAdminScope>("All");
  const [filters, setFilters] = useState(EMPTY_PLUGIN_LIST_FILTERS);
  const [drawer, setDrawer] = useState<DrawerState>(null);
  const [deleteTarget, setDeleteTarget] = useState<PluginRecord | null>(null);
  const [error, setError] = useState<string | null>(null);

  function reload() {
    setItems(loadPluginCatalog(owner));
    setCategories(loadPluginCategories());
  }

  useEffect(() => {
    try {
      reload();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
    // `owner` is the isolation key: switching accounts must re-read the catalog.
  }, [owner]);

  /**
   * Console always shows the caller's own plugins. The admin surface defaults
   * to the whole workspace but can be narrowed to the operator's own rows.
   */
  const scopedItems = useMemo(
    () => (!isAdmin || scope === "All" ? items : filterPluginRecordsByOwner(items, owner)),
    [isAdmin, items, owner, scope],
  );
  const visibleItems = useMemo(() => filterPluginRecords(scopedItems, filters), [filters, scopedItems]);
  const filterActive = hasActivePluginFilters(filters);

  /**
   * Column definitions map 1:1 to the table contract. Category renders the
   * curated label (retired categories stay readable); tools/source carry
   * multi-part rich text; the row action slot holds edit/delete.
   */
  const columns = useMemo<DataTableColumn<PluginRecord>[]>(() => {
    const base: DataTableColumn<PluginRecord>[] = [
      { id: "displayName", header: t("mine.column.name"), cell: (item) => item.displayName },
      {
        id: "category",
        header: t("mine.column.category"),
        cell: (item) => {
          const category = findPluginCategory(categories, item.categoryId);
          if (!category) return <span className="plugin-category-cell-empty">{t("mine.category.none")}</span>;
          if (!category.enabled) {
            return (
              <span className="plugin-category-cell plugin-category-cell--retired">
                {t("mine.category.retired", { name: category.name })}
              </span>
            );
          }
          return <span className="plugin-category-cell">{category.name}</span>;
        },
      },
      { id: "pluginKey", header: t("mine.column.key"), cell: (item) => item.pluginKey },
    ];
    if (isAdmin) {
      base.push({
        id: "ownerKey",
        header: t("admin.columns.owner"),
        cell: (item) => (
          <span className="plugin-owner-cell">
            {item.ownerKey || t("admin.owner.unknown")}
          </span>
        ),
      });
    }
    base.push(
      {
        id: "tools",
        header: t("mine.column.tools"),
        cell: (item) => <PluginToolBadges hostTools={item.supportedHostTools} capabilities={item.contributedCapabilities} />,
      },
      {
        id: "source",
        header: t("mine.column.source"),
        cell: (item) => (
          <>
            <strong>{item.sourceKind === "git" ? t("mine.source.git") : t("mine.source.archive")}</strong>
            <span className="plugin-source-detail" title={formatSourceDetail(item, t)}>
              {formatSourceDetail(item, t)}
            </span>
          </>
        ),
      },
      { id: "version", header: t("mine.column.version"), cell: (item) => item.version },
      {
        id: "status",
        header: t("mine.column.status"),
        cell: (item) => item.status === "draft" ? t("mine.status.draft") : t("mine.status.active"),
      },
    );
    return base;
  }, [categories, isAdmin, t]);

  function persist(next: PluginRecord[]) {
    savePluginCatalog(owner, next);
    setItems(next);
  }

  return (
    <section className="skills-console-page">
      <header className="skills-console-header">
        <div>
          <h2>{isAdmin ? t("admin.title") : t("mine.title")}</h2>
          <p>{isAdmin ? t("admin.description") : t("mine.description")}</p>
        </div>
        <div className="skills-console-header-actions">
          <button type="button" className="plugin-secondary" onClick={reload}>
            {t("mine.refresh")}
          </button>
          <button type="button" className="skills-console-primary" onClick={() => setDrawer({ kind: "create" })}>
            {t("mine.create")}
          </button>
        </div>
      </header>
      {!isAdmin ? <p className="plugin-owner-note">{t("mine.owner.scope")}</p> : null}
      {isAdmin ? (
        <div className="plugin-scope-bar" role="group" aria-label={t("admin.scope.scope")}>
          {(["All", "User"] as const).map((candidate) => (
            <button
              key={candidate}
              type="button"
              className={`plugin-scope-option${scope === candidate ? " plugin-scope-option--active" : ""}`}
              aria-pressed={scope === candidate}
              onClick={() => setScope(candidate)}
            >
              <span className="plugin-scope-option-label">{t(`admin.scope.${candidate}` as never)}</span>
              <span className="plugin-scope-option-description">
                {t(`admin.scope.${candidate}.description` as never)}
              </span>
            </button>
          ))}
        </div>
      ) : null}
      {error ? (
        <p className="skills-console-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="data-surface">
        {scopedItems.length > 0 ? (
          <PluginListFilterBar filters={filters} categories={categories} onChange={setFilters} />
        ) : null}
        <DataTable<PluginRecord>
          columns={columns}
          density="compact"
          emptyState={scopedItems.length === 0 ? (
            <div className="empty-state">
              <h3>{t("mine.empty.title")}</h3>
              <p>{t("mine.empty.description")}</p>
              <button type="button" className="skills-console-primary" onClick={() => setDrawer({ kind: "create" })}>
                {t("mine.empty.action")}
              </button>
            </div>
          ) : (
            <div className="empty-state">
              <h3>{t("mine.filter.empty.title")}</h3>
              <p>{t("mine.filter.empty.description")}</p>
              <button
                type="button"
                className="skills-console-primary"
                onClick={() => setFilters(EMPTY_PLUGIN_LIST_FILTERS)}
              >
                {t("mine.filter.clear")}
              </button>
            </div>
          )}
          getRowId={(item) => item.id}
          pagination={{ defaultPageSize: 20, mode: "client", pageSizeOptions: PLUGIN_PAGE_SIZES }}
          // A read-only cross-user row must not offer edit/delete: the console
          // can only mutate rows this operator owns.
          rowActions={(item) => {
            const editable = !isAdmin || item.ownerKey === owner;
            if (!editable) return <span className="plugin-row-readonly">—</span>;
            return (
              <div className="skills-console-actions">
                <button type="button" onClick={() => setDrawer({ kind: "edit", plugin: item })}>
                  {t("mine.edit")}
                </button>
                <button type="button" onClick={() => setDeleteTarget(item)}>
                  {t("mine.delete")}
                </button>
              </div>
            );
          }}
          rowActionsLabel={t("mine.column.actions")}
          rows={visibleItems}
          stickyHeader
        />
        {filterActive && visibleItems.length > 0 ? (
          <p className="plugin-filter-summary">
            {t("mine.filter.summary", { shown: visibleItems.length, total: scopedItems.length })}
          </p>
        ) : null}
      </div>

      <SurfaceDrawer
        open={drawer?.kind === "create"}
        title={t("create.title")}
        description={t("create.description")}
        onClose={() => setDrawer(null)}
      >
        <CreatePluginForm
          categories={categories}
          drive={drive}
          existingKeys={filterPluginRecordsByOwner(items, owner).map((item) => item.pluginKey)}
          ownerKey={owner}
          onCancel={() => setDrawer(null)}
          onSuccess={async (record) => {
            persist(upsertPluginRecord(items, record));
            setDrawer(null);
          }}
        />
      </SurfaceDrawer>

      <SurfaceDrawer
        open={drawer?.kind === "edit"}
        title={t("edit.title")}
        description={drawer?.kind === "edit"
          ? t("edit.description", { key: drawer.plugin.pluginKey })
          : undefined}
        onClose={() => setDrawer(null)}
      >
        {drawer?.kind === "edit" ? (
          <EditPluginForm
            categories={categories}
            drive={drive}
            plugin={drawer.plugin}
            onCancel={() => setDrawer(null)}
            onSuccess={async (record) => {
              persist(upsertPluginRecord(items, record));
              setDrawer(null);
            }}
          />
        ) : null}
      </SurfaceDrawer>

      <ConfirmModal
        open={deleteTarget != null}
        title={t("mine.delete.confirmTitle")}
        description={t("mine.delete.confirmDescription", { name: deleteTarget?.displayName ?? "" })}
        confirmLabel={t("mine.delete")}
        cancelLabel={t("dialog.cancel")}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => {
          if (!deleteTarget) return;
          persist(removePluginRecord(items, deleteTarget.id));
          setDeleteTarget(null);
        }}
      />
    </section>
  );
}
