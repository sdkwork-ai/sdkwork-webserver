import { useEffect, useMemo, useState, type FormEvent } from "react";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { usePluginsT } from "./locale.tsx";
import {
  createPluginCategoryId,
  isValidPluginCategoryCode,
  normalizePluginCategoryCode,
  removePluginCategoryRecord,
  seedPluginCategories,
  sortPluginCategories,
  upsertPluginCategoryRecord,
  type PluginCategoryRecord,
} from "./plugin-category.ts";
import { loadPluginCategories, savePluginCategories } from "./plugin-category-catalog.ts";
import { loadPluginCatalog } from "./plugin-catalog.ts";
import { normalizePluginOwnerKey, type PluginRecord } from "./plugin-model.ts";
import { ConsoleField, ConsoleFormSection } from "./plugin-form-kit.tsx";
import { ConfirmModal, SurfaceDrawer } from "./SurfaceOverlay.tsx";

type DrawerState = { kind: "create" } | { kind: "edit"; category: PluginCategoryRecord } | null;

interface CategoryDraft {
  id: string | null;
  code: string;
  name: string;
  description: string;
  sortOrder: string;
}

const EMPTY_DRAFT: CategoryDraft = {
  id: null,
  code: "",
  name: "",
  description: "",
  sortOrder: "10",
};

/**
 * Platform category curation for the admin console.
 *
 * Categories are the one list every user's plugin create form reads, so this
 * page is deliberately the single place they are written. Two safety rules are
 * enforced here rather than left to discipline:
 *  - a category **in use** by any plugin cannot be hard-deleted (only retired),
 *    so no plugin is silently orphaned;
 *  - **retiring** keeps it resolvable for existing plugins while removing it
 *    from the picker.
 */
export function PluginCategoriesAdminPage({ ownerKey }: { ownerKey: string }) {
  const t = usePluginsT();
  const owner = normalizePluginOwnerKey(ownerKey);
  const [categories, setCategories] = useState<PluginCategoryRecord[]>([]);
  const [allPlugins, setAllPlugins] = useState<PluginRecord[]>([]);
  const [draft, setDraft] = useState<CategoryDraft>(EMPTY_DRAFT);
  const [drawer, setDrawer] = useState<DrawerState>(null);
  const [deleteTarget, setDeleteTarget] = useState<PluginCategoryRecord | null>(null);
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [codeError, setCodeError] = useState<string | null>(null);
  const [nameError, setNameError] = useState<string | null>(null);

  useEffect(() => {
    try {
      setCategories(loadPluginCategories());
      // Usage counts read the caller's own catalog; the admin surface is the
      // only place plugins are curated, so this is the authoritative view.
      setAllPlugins(loadPluginCatalog(owner));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [owner]);

  const usageByCategory = useMemo(() => {
    const counts = new Map<string, number>();
    for (const plugin of allPlugins) {
      if (!plugin.categoryId) continue;
      counts.set(plugin.categoryId, (counts.get(plugin.categoryId) ?? 0) + 1);
    }
    return counts;
  }, [allPlugins]);

  const enabledCount = categories.filter((category) => category.enabled).length;

  function persist(next: PluginCategoryRecord[]) {
    const sorted = sortPluginCategories(next);
    savePluginCategories(sorted);
    setCategories(sorted);
  }

  function openCreate() {
    setDraft({ ...EMPTY_DRAFT, sortOrder: String((categories.length + 1) * 10) });
    setCodeError(null);
    setNameError(null);
    setError(null);
    setDrawer({ kind: "create" });
  }

  function openEdit(category: PluginCategoryRecord) {
    setDraft({
      id: category.id,
      code: category.code,
      name: category.name,
      description: category.description,
      sortOrder: String(category.sortOrder),
    });
    setCodeError(null);
    setNameError(null);
    setError(null);
    setDrawer({ kind: "edit", category });
  }

  function onSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setCodeError(null);
    setNameError(null);
    const code = normalizePluginCategoryCode(draft.code);
    const name = draft.name.trim();
    if (!isValidPluginCategoryCode(code)) {
      setCodeError(t("categories.admin.error.code"));
      return;
    }
    if (!name) {
      setNameError(t("categories.admin.error.nameRequired"));
      return;
    }
    const clash = categories.find(
      (category) => category.code === code && category.id !== draft.id,
    );
    if (clash) {
      setCodeError(t("categories.admin.error.duplicateCode", { code }));
      return;
    }
    const now = new Date().toISOString();
    const existing = draft.id
      ? categories.find((category) => category.id === draft.id) ?? null
      : null;
    const next: PluginCategoryRecord = {
      id: existing?.id ?? createPluginCategoryId(),
      code,
      name,
      description: draft.description.trim(),
      sortOrder: Number.parseInt(draft.sortOrder, 10) || 0,
      enabled: existing?.enabled ?? true,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    };
    persist(upsertPluginCategoryRecord(categories, next));
    setDrawer(null);
  }

  function toggleEnabled(category: PluginCategoryRecord) {
    persist(
      upsertPluginCategoryRecord(categories, {
        ...category,
        enabled: !category.enabled,
        updatedAt: new Date().toISOString(),
      }),
    );
  }

  function confirmDelete() {
    if (!deleteTarget) return;
    persist(removePluginCategoryRecord(categories, deleteTarget.id));
    setDeleteTarget(null);
  }

  /** Re-add any default category whose code is missing; custom ones survive. */
  function restoreDefaults() {
    const now = new Date().toISOString();
    const existingCodes = new Set(categories.map((category) => category.code));
    const restored = seedPluginCategories(now).filter(
      (category) => !existingCodes.has(category.code),
    );
    persist([...categories, ...restored]);
    setRestoreOpen(false);
  }

  const columns = useMemo<DataTableColumn<PluginCategoryRecord>[]>(() => [
    { id: "name", header: t("categories.admin.column.name"), cell: (category) => category.name },
    {
      id: "code",
      header: t("categories.admin.column.code"),
      cell: (category) => <code className="plugin-category-code">{category.code}</code>,
    },
    {
      id: "description",
      header: t("categories.admin.column.description"),
      cell: (category) => (
        <span className="plugin-category-description">{category.description || "—"}</span>
      ),
    },
    { id: "sortOrder", header: t("categories.admin.column.sort"), cell: (category) => category.sortOrder },
    {
      id: "usage",
      header: t("mine.column.tools"),
      cell: (category) => (
        <span className="plugin-category-usage">
          {t("categories.admin.usage", { count: usageByCategory.get(category.id) ?? 0 })}
        </span>
      ),
    },
    {
      id: "enabled",
      header: t("categories.admin.column.state"),
      cell: (category) => (
        <span
          className={
            category.enabled
              ? "plugin-category-state plugin-category-state--enabled"
              : "plugin-category-state plugin-category-state--disabled"
          }
        >
          {category.enabled
            ? t("categories.admin.state.enabled")
            : t("categories.admin.state.disabled")}
        </span>
      ),
    },
  ], [t, usageByCategory]);

  const deleteUsage = deleteTarget ? usageByCategory.get(deleteTarget.id) ?? 0 : 0;

  return (
    <section className="skills-console-page">
      <header className="skills-console-header">
        <div>
          <h2>{t("categories.admin.title")}</h2>
          <p>{t("categories.admin.description")}</p>
        </div>
        <div className="skills-console-header-actions">
          <button type="button" className="plugin-secondary" onClick={() => setRestoreOpen(true)}>
            {t("categories.admin.seedRestore")}
          </button>
          <button type="button" className="skills-console-primary" onClick={openCreate}>
            {t("categories.admin.create")}
          </button>
        </div>
      </header>
      <p className="plugin-owner-note">
        {t("categories.admin.summary", { enabled: enabledCount, total: categories.length })}
      </p>
      {error ? (
        <p className="skills-console-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="data-surface">
        <DataTable<PluginCategoryRecord>
          columns={columns}
          density="compact"
          emptyState={(
            <div className="empty-state">
              <h3>{t("categories.admin.empty.title")}</h3>
              <p>{t("categories.admin.empty.description")}</p>
              <button type="button" className="skills-console-primary" onClick={openCreate}>
                {t("categories.admin.create")}
              </button>
            </div>
          )}
          getRowId={(category) => category.id}
          pagination={{ defaultPageSize: 20, mode: "client", pageSizeOptions: [20, 50] }}
          rowActions={(category) => {
            const usage = usageByCategory.get(category.id) ?? 0;
            return (
              <div className="skills-console-actions">
                <button type="button" onClick={() => openEdit(category)}>
                  {t("mine.edit")}
                </button>
                <button type="button" onClick={() => toggleEnabled(category)}>
                  {category.enabled ? t("categories.admin.disable") : t("categories.admin.enable")}
                </button>
                {/* Hard-deleting an in-use category would orphan plugins, so the
                    row only offers retire until the last plugin moves off it. */}
                <button
                  type="button"
                  disabled={usage > 0}
                  title={usage > 0 ? t("categories.admin.error.inUse", { name: category.name, count: usage }) : undefined}
                  onClick={() => setDeleteTarget(category)}
                >
                  {t("mine.delete")}
                </button>
              </div>
            );
          }}
          rowActionsLabel={t("categories.admin.column.actions")}
          rows={categories}
          stickyHeader
        />
      </div>

      <SurfaceDrawer
        open={drawer !== null}
        title={drawer?.kind === "edit"
          ? t("categories.admin.drawer.editTitle")
          : t("categories.admin.drawer.createTitle")}
        description={t("categories.admin.drawer.description")}
        onClose={() => setDrawer(null)}
        size="md"
      >
        <form className="skills-console-form plugin-console-form" onSubmit={onSubmit}>
          <ConsoleFormSection title={t("categories.admin.title")}>
            <ConsoleField
              htmlFor="plugin-category-code"
              label={t("categories.admin.field.code")}
              required
              hint={
                codeError
                  ? <span className="plugin-field-warning">{codeError}</span>
                  : t("categories.admin.hint.code")
              }
            >
              <input
                id="plugin-category-code"
                value={draft.code}
                onChange={(event) => setDraft({ ...draft, code: event.target.value })}
                placeholder="workspace.tooling"
                required
              />
            </ConsoleField>
            <ConsoleField
              htmlFor="plugin-category-name"
              label={t("categories.admin.field.name")}
              required
              hint={nameError ? <span className="plugin-field-warning">{nameError}</span> : undefined}
            >
              <input
                id="plugin-category-name"
                value={draft.name}
                onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                required
              />
            </ConsoleField>
            <ConsoleField
              htmlFor="plugin-category-description"
              label={t("categories.admin.field.description")}
              optionalLabel={t("create.field.optional")}
            >
              <textarea
                id="plugin-category-description"
                value={draft.description}
                onChange={(event) => setDraft({ ...draft, description: event.target.value })}
                rows={3}
              />
            </ConsoleField>
            <ConsoleField
              htmlFor="plugin-category-sort"
              label={t("categories.admin.field.sort")}
              hint={t("categories.admin.hint.sort")}
            >
              <input
                id="plugin-category-sort"
                type="number"
                value={draft.sortOrder}
                onChange={(event) => setDraft({ ...draft, sortOrder: event.target.value })}
              />
            </ConsoleField>
          </ConsoleFormSection>
          <div className="sdkwork-surface-drawer-form-actions">
            <button type="button" onClick={() => setDrawer(null)}>
              {t("dialog.cancel")}
            </button>
            <button type="submit" className="skills-console-primary">
              {drawer?.kind === "edit" ? t("edit.save") : t("categories.admin.create")}
            </button>
          </div>
        </form>
      </SurfaceDrawer>

      <ConfirmModal
        open={deleteTarget != null}
        title={t("categories.admin.delete.confirmTitle")}
        description={t("categories.admin.delete.confirmDescription", { name: deleteTarget?.name ?? "" })}
        confirmLabel={t("mine.delete")}
        cancelLabel={t("dialog.cancel")}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={confirmDelete}
        busy={deleteUsage > 0}
      />

      <ConfirmModal
        open={restoreOpen}
        title={t("categories.admin.seedRestore.confirmTitle")}
        description={t("categories.admin.seedRestore.confirmDescription")}
        confirmLabel={t("categories.admin.seedRestore")}
        cancelLabel={t("dialog.cancel")}
        tone="default"
        onCancel={() => setRestoreOpen(false)}
        onConfirm={restoreDefaults}
      />
    </section>
  );
}
