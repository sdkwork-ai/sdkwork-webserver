import { uuid } from "@sdkwork/utils/id";

/**
 * Plugin categories are **platform-curated** navigation groups, maintained by
 * an operator in the backend-admin console and consumed read-only by every
 * user's plugin create/edit form. Categories are therefore global (not
 * per-user): a plugin record only carries the `categoryId` it was filed under,
 * so renaming or retiring a category never has to rewrite user records.
 */
export const PLUGIN_CATEGORY_STORAGE_KEY = "sdkwork.webserver.plugins.categories.v1";

export const PLUGIN_CATEGORY_CODE_PATTERN = /^[a-z][a-z0-9-]*(\.[a-z0-9-]+)*$/;
export const PLUGIN_CATEGORY_NAME_MAX_LENGTH = 60;

export interface PluginCategoryRecord {
  id: string;
  /** Stable machine code, e.g. `workspace.tooling`. Never shown as the label. */
  code: string;
  name: string;
  description: string;
  /** Ascending display order; ties fall back to `name` for a stable table. */
  sortOrder: number;
  /**
   * Retired categories stay in storage so existing plugin records keep
   * resolving their label, but they are **not offered** to the create form.
   */
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface PluginCategorySnapshot {
  version: 1;
  items: PluginCategoryRecord[];
}

/**
 * Fallback catalog seeded on first run. These are the platform categories an
 * operator starts from; they can be renamed, reordered, retired, or deleted.
 */
export const DEFAULT_PLUGIN_CATEGORIES: readonly Omit<
  PluginCategoryRecord,
  "id" | "createdAt" | "updatedAt"
>[] = [
  {
    code: "workspace.tooling",
    name: "Workspace tooling",
    description: "Build, package, and workspace automation bundles.",
    sortOrder: 10,
    enabled: true,
  },
  {
    code: "code.intelligence",
    name: "Code intelligence",
    description: "Review, navigation, and refactoring assistants.",
    sortOrder: 20,
    enabled: true,
  },
  {
    code: "data.integration",
    name: "Data & integration",
    description: "Connectors to databases, APIs, and third-party services.",
    sortOrder: 30,
    enabled: true,
  },
  {
    code: "docs.knowledge",
    name: "Docs & knowledge",
    description: "Documentation, knowledge bases, and content workflows.",
    sortOrder: 40,
    enabled: true,
  },
  {
    code: "ops.delivery",
    name: "Ops & delivery",
    description: "Deployment, observability, and release automation.",
    sortOrder: 50,
    enabled: true,
  },
];

export function createPluginCategoryId(): string {
  return uuid();
}

export function isValidPluginCategoryCode(value: string): boolean {
  return PLUGIN_CATEGORY_CODE_PATTERN.test(value.trim());
}

export function normalizePluginCategoryCode(value: string): string {
  return value.trim().toLowerCase();
}

export function normalizePluginCategoryRecord(value: unknown): PluginCategoryRecord | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<PluginCategoryRecord>;
  if (typeof record.id !== "string" || typeof record.code !== "string") return null;
  const code = normalizePluginCategoryCode(record.code);
  if (!code) return null;
  const name = typeof record.name === "string" ? record.name.trim() : "";
  const now = new Date().toISOString();
  return {
    id: record.id,
    code,
    name: name || code,
    description: typeof record.description === "string" ? record.description : "",
    sortOrder: typeof record.sortOrder === "number" && Number.isFinite(record.sortOrder)
      ? record.sortOrder
      : 0,
    // Absent `enabled` on a legacy record means "still offered".
    enabled: record.enabled !== false,
    createdAt: typeof record.createdAt === "string" ? record.createdAt : now,
    updatedAt: typeof record.updatedAt === "string" ? record.updatedAt : now,
  };
}

export function parsePluginCategories(raw: string | null): PluginCategoryRecord[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as PluginCategorySnapshot | PluginCategoryRecord[];
    const items = Array.isArray(parsed) ? parsed : parsed.items;
    if (!Array.isArray(items)) return [];
    return items
      .map(normalizePluginCategoryRecord)
      .filter((item): item is PluginCategoryRecord => item != null);
  } catch {
    return [];
  }
}

export function serializePluginCategories(items: readonly PluginCategoryRecord[]): string {
  const snapshot: PluginCategorySnapshot = { version: 1, items: [...items] };
  return JSON.stringify(snapshot);
}

/** Stable display order: `sortOrder` ascending, then name, then code. */
export function sortPluginCategories(
  items: readonly PluginCategoryRecord[],
): PluginCategoryRecord[] {
  return [...items].sort((left, right) => {
    if (left.sortOrder !== right.sortOrder) return left.sortOrder - right.sortOrder;
    const byName = left.name.localeCompare(right.name);
    return byName !== 0 ? byName : left.code.localeCompare(right.code);
  });
}

export function seedPluginCategories(now = new Date().toISOString()): PluginCategoryRecord[] {
  return DEFAULT_PLUGIN_CATEGORIES.map((category) => ({
    ...category,
    id: createPluginCategoryId(),
    createdAt: now,
    updatedAt: now,
  }));
}

/** Categories offered in the plugin create/edit picker. */
export function selectablePluginCategories(
  items: readonly PluginCategoryRecord[],
): PluginCategoryRecord[] {
  return sortPluginCategories(items.filter((item) => item.enabled));
}

export function findPluginCategory(
  items: readonly PluginCategoryRecord[],
  categoryId: string | null | undefined,
): PluginCategoryRecord | null {
  if (!categoryId) return null;
  return items.find((item) => item.id === categoryId) ?? null;
}

/** Operator-facing label for a plugin's category; falls back when unset. */
export function resolvePluginCategoryName(
  items: readonly PluginCategoryRecord[],
  categoryId: string | null | undefined,
): string | null {
  return findPluginCategory(items, categoryId)?.name ?? null;
}

export function upsertPluginCategoryRecord(
  items: readonly PluginCategoryRecord[],
  next: PluginCategoryRecord,
): PluginCategoryRecord[] {
  // Code is the operator-visible uniqueness contract (it is the stable handle
  // other tooling keys off), so a same-code different-id row is collapsed too.
  const remaining = items.filter(
    (item) => item.id !== next.id && item.code !== next.code,
  );
  return sortPluginCategories([...remaining, next]);
}

export function removePluginCategoryRecord(
  items: readonly PluginCategoryRecord[],
  categoryId: string,
): PluginCategoryRecord[] {
  return items.filter((item) => item.id !== categoryId);
}
