import {
  PLUGIN_CATALOG_STORAGE_KEY,
  parsePluginCatalog,
  serializePluginCatalog,
  type PluginRecord,
} from "./plugin-model.ts";

export function loadPluginCatalog(storage: Pick<Storage, "getItem"> = localStorage): PluginRecord[] {
  return parsePluginCatalog(storage.getItem(PLUGIN_CATALOG_STORAGE_KEY));
}

export function savePluginCatalog(
  items: readonly PluginRecord[],
  storage: Pick<Storage, "setItem"> = localStorage,
): void {
  storage.setItem(PLUGIN_CATALOG_STORAGE_KEY, serializePluginCatalog(items));
}

export function upsertPluginRecord(
  items: readonly PluginRecord[],
  next: PluginRecord,
): PluginRecord[] {
  const remaining = items.filter((item) => item.id !== next.id && item.pluginKey !== next.pluginKey);
  return [next, ...remaining].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
}

export function removePluginRecord(items: readonly PluginRecord[], pluginId: string): PluginRecord[] {
  return items.filter((item) => item.id !== pluginId);
}
