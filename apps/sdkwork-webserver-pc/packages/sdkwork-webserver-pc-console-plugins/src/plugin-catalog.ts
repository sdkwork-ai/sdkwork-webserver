import {
  PLUGIN_CATALOG_LEGACY_STORAGE_KEY,
  normalizePluginOwnerKey,
  parsePluginCatalog,
  resolvePluginCatalogStorageKey,
  serializePluginCatalog,
  type PluginRecord,
} from "./plugin-model.ts";

type ReadableStorage = Pick<Storage, "getItem">;
type WritableStorage = Pick<Storage, "setItem">;

/**
 * Reads the current user's plugin catalog.
 *
 * On the first read for an owner we migrate the legacy single-tenant key onto
 * that owner, so the account that had plugins before ownership isolation keeps
 * them instead of seeing an empty console.
 */
export function loadPluginCatalog(
  ownerKey: string,
  storage: ReadableStorage & Partial<WritableStorage> = localStorage,
): PluginRecord[] {
  const owner = normalizePluginOwnerKey(ownerKey);
  const scopedKey = resolvePluginCatalogStorageKey(owner);
  const scopedRaw = storage.getItem(scopedKey);
  if (scopedRaw !== null) return parsePluginCatalog(scopedRaw, owner);

  const legacyRaw = storage.getItem(PLUGIN_CATALOG_LEGACY_STORAGE_KEY);
  if (legacyRaw === null) return [];
  // Legacy rows pre-date ownership isolation, so whatever owner they carry is
  // meaningless (the old model had no owner). Re-own them to the account
  // consuming the legacy key — otherwise a stale `ownerKey` would keep the
  // migrated plugin invisible to the very user we are migrating it for.
  const migrated = parsePluginCatalog(legacyRaw, owner).map((item) => ({ ...item, ownerKey: owner }));
  if (storage.setItem) {
    storage.setItem(scopedKey, serializePluginCatalog(migrated));
    // The legacy key is consumed once; keeping it would re-migrate the same
    // rows into whichever account signs in next.
    storage.setItem(PLUGIN_CATALOG_LEGACY_STORAGE_KEY, "");
  }
  return migrated;
}

export function savePluginCatalog(
  ownerKey: string,
  items: readonly PluginRecord[],
  storage: WritableStorage = localStorage,
): void {
  const owner = normalizePluginOwnerKey(ownerKey);
  storage.setItem(resolvePluginCatalogStorageKey(owner), serializePluginCatalog(items));
}

/**
 * Upserts a record **within an owner's catalog**. The owner is part of the
 * match key, so the same `pluginKey` registered by two users stays two rows
 * and one user can never overwrite another's plugin.
 */
export function upsertPluginRecord(
  items: readonly PluginRecord[],
  next: PluginRecord,
): PluginRecord[] {
  const owner = normalizePluginOwnerKey(next.ownerKey);
  const scoped = next.ownerKey === owner ? next : { ...next, ownerKey: owner };
  const remaining = items.filter(
    (item) =>
      item.ownerKey !== owner
      || (item.id !== scoped.id && item.pluginKey !== scoped.pluginKey),
  );
  return [scoped, ...remaining].sort((left, right) =>
    right.updatedAt.localeCompare(left.updatedAt),
  );
}

export function removePluginRecord(
  items: readonly PluginRecord[],
  pluginId: string,
): PluginRecord[] {
  return items.filter((item) => item.id !== pluginId);
}

export function filterPluginRecordsByOwner(
  items: readonly PluginRecord[],
  ownerKey: string,
): PluginRecord[] {
  const owner = normalizePluginOwnerKey(ownerKey);
  return items.filter((item) => normalizePluginOwnerKey(item.ownerKey) === owner);
}
