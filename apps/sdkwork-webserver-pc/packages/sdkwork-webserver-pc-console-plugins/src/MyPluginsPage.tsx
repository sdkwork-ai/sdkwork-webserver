import { useEffect, useMemo, useState } from "react";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import { CreatePluginForm } from "./CreatePluginForm.tsx";
import { EditPluginForm } from "./EditPluginForm.tsx";
import { usePluginsT } from "./locale.tsx";
import {
  EMPTY_PLUGIN_LIST_FILTERS,
  filterPluginRecords,
  hasActivePluginFilters,
} from "./plugin-filter.ts";
import {
  loadPluginCatalog,
  removePluginRecord,
  savePluginCatalog,
  upsertPluginRecord,
} from "./plugin-catalog.ts";
import type { PluginRecord } from "./plugin-model.ts";
import { PluginListFilterBar, PluginToolBadges } from "./PluginToolMultiSelect.tsx";
import { ConfirmModal, SurfaceDrawer } from "./SurfaceOverlay.tsx";

type DrawerState = { kind: "create" } | { kind: "edit"; plugin: PluginRecord } | null;

function formatSourceDetail(item: PluginRecord, t: (key: "mine.source.git" | "mine.source.archive") => string): string {
  if (item.sourceKind === "git") {
    const ref = item.gitRef ? ` @ ${item.gitRef}` : "";
    return `${item.gitRepository ?? t("mine.source.git")}${ref}`;
  }
  return item.archiveFileName || item.artifactRef || t("mine.source.archive");
}

export function MyPluginsPage({
  drive,
  variant = "console",
}: {
  drive: SdkworkDriveAppClient;
  variant?: "admin" | "console";
}) {
  const t = usePluginsT();
  const [items, setItems] = useState<PluginRecord[]>([]);
  const [filters, setFilters] = useState(EMPTY_PLUGIN_LIST_FILTERS);
  const [drawer, setDrawer] = useState<DrawerState>(null);
  const [deleteTarget, setDeleteTarget] = useState<PluginRecord | null>(null);
  const [error, setError] = useState<string | null>(null);

  const visibleItems = useMemo(() => filterPluginRecords(items, filters), [filters, items]);
  const filterActive = hasActivePluginFilters(filters);

  useEffect(() => {
    try {
      setItems(loadPluginCatalog());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  function persist(next: PluginRecord[]) {
    savePluginCatalog(next);
    setItems(next);
  }

  return (
    <section className="skills-console-page">
      <header className="skills-console-header">
        <div>
          <h2>{variant === "admin" ? t("admin.title") : t("mine.title")}</h2>
          <p>{variant === "admin" ? t("admin.description") : t("mine.description")}</p>
        </div>
        <button type="button" className="skills-console-primary" onClick={() => setDrawer({ kind: "create" })}>
          {t("mine.create")}
        </button>
      </header>
      {error ? (
        <p className="skills-console-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="data-surface">
        {items.length > 0 ? (
          <PluginListFilterBar filters={filters} onChange={setFilters} />
        ) : null}
        <div className="table-frame">
          {items.length === 0 ? (
            <div className="empty-state">
              <h3>{t("mine.empty.title")}</h3>
              <p>{t("mine.empty.description")}</p>
              <button type="button" className="skills-console-primary" onClick={() => setDrawer({ kind: "create" })}>
                {t("mine.empty.action")}
              </button>
            </div>
          ) : visibleItems.length === 0 ? (
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
          ) : (
            <table>
              <thead>
                <tr>
                  <th>{t("mine.column.name")}</th>
                  <th>{t("mine.column.key")}</th>
                  <th>{t("mine.column.tools")}</th>
                  <th>{t("mine.column.source")}</th>
                  <th>{t("mine.column.version")}</th>
                  <th>{t("mine.column.status")}</th>
                  <th>{t("mine.column.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {visibleItems.map((item) => (
                  <tr key={item.id}>
                    <td>{item.displayName}</td>
                    <td>{item.pluginKey}</td>
                    <td>
                      <PluginToolBadges
                        hostTools={item.supportedHostTools}
                        capabilities={item.contributedCapabilities}
                      />
                    </td>
                    <td>
                      <strong>{item.sourceKind === "git" ? t("mine.source.git") : t("mine.source.archive")}</strong>
                      <span className="plugin-source-detail" title={formatSourceDetail(item, t)}>
                        {formatSourceDetail(item, t)}
                      </span>
                    </td>
                    <td>{item.version}</td>
                    <td>{item.status === "draft" ? t("mine.status.draft") : t("mine.status.active")}</td>
                    <td>
                      <div className="skills-console-actions">
                        <button type="button" onClick={() => setDrawer({ kind: "edit", plugin: item })}>
                          {t("mine.edit")}
                        </button>
                        <button type="button" onClick={() => setDeleteTarget(item)}>
                          {t("mine.delete")}
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
        {filterActive && visibleItems.length > 0 ? (
          <p className="plugin-filter-summary">
            {t("mine.filter.summary", { shown: visibleItems.length, total: items.length })}
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
          drive={drive}
          existingKeys={items.map((item) => item.pluginKey)}
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
        onClose={() => setDrawer(null)}
      >
        {drawer?.kind === "edit" ? (
          <EditPluginForm
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
