import {
  PLUGIN_CATEGORY_STORAGE_KEY,
  parsePluginCategories,
  seedPluginCategories,
  serializePluginCategories,
  sortPluginCategories,
  type PluginCategoryRecord,
} from "./plugin-category.ts";

type ReadableStorage = Pick<Storage, "getItem">;
type WritableStorage = Pick<Storage, "setItem">;

/**
 * Categories are a **platform catalog**: one global list, curated by an
 * operator in the backend-admin console and read by every user's create form.
 * It is therefore not owner-scoped, unlike the plugin catalog itself.
 *
 * An empty catalog is seeded with `DEFAULT_PLUGIN_CATEGORIES` so the create
 * form is never blocked by an operator who has not curated anything yet.
 */
export function loadPluginCategories(
  storage: ReadableStorage & Partial<WritableStorage> = localStorage,
): PluginCategoryRecord[] {
  const raw = storage.getItem(PLUGIN_CATEGORY_STORAGE_KEY);
  const parsed = parsePluginCategories(raw);
  if (parsed.length > 0) return sortPluginCategories(parsed);
  const seeded = seedPluginCategories();
  if (storage.setItem) {
    storage.setItem(PLUGIN_CATEGORY_STORAGE_KEY, serializePluginCategories(seeded));
  }
  return seeded;
}

export function savePluginCategories(
  items: readonly PluginCategoryRecord[],
  storage: WritableStorage = localStorage,
): void {
  storage.setItem(PLUGIN_CATEGORY_STORAGE_KEY, serializePluginCategories(items));
}
