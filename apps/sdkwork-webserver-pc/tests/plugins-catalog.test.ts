import { describe, expect, it } from "vitest";

import {
  PLUGIN_CATALOG_ANONYMOUS_OWNER,
  PLUGIN_CATALOG_LEGACY_STORAGE_KEY,
  parsePluginCatalog,
  serializePluginCatalog,
  isValidPluginKey,
  createPluginId,
  normalizePluginOwnerKey,
  normalizePluginRecord,
  resolvePluginCatalogStorageKey,
  type PluginRecord,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-model.ts";
import {
  filterPluginRecordsByOwner,
  loadPluginCatalog,
  removePluginRecord,
  savePluginCatalog,
  upsertPluginRecord,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-catalog.ts";
import {
  EMPTY_PLUGIN_LIST_FILTERS,
  filterPluginRecords,
  hasActivePluginFilters,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-filter.ts";
import {
  DEFAULT_PLUGIN_CATEGORIES,
  createPluginCategoryId,
  findPluginCategory,
  isValidPluginCategoryCode,
  normalizePluginCategoryCode,
  normalizePluginCategoryRecord,
  parsePluginCategories,
  removePluginCategoryRecord,
  selectablePluginCategories,
  seedPluginCategories,
  serializePluginCategories,
  sortPluginCategories,
  upsertPluginCategoryRecord,
  type PluginCategoryRecord,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-category.ts";
import {
  loadPluginCategories,
  savePluginCategories,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-category-catalog.ts";
import {
  PLUGIN_HOST_TOOL_IDS,
  PLUGIN_HOST_TOOL_GROUPS,
  PLUGIN_HOST_TOOL_MONOGRAMS,
  normalizePluginHostTools,
  normalizePluginContributions,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-tool-catalog.ts";

/** Minimal in-memory Storage double so per-owner isolation is testable. */
function memoryStorage(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => (map.has(key) ? (map.get(key) as string) : null),
    setItem: (key: string, value: string) => void map.set(key, value),
    removeItem: (key: string) => void map.delete(key),
    snapshot: () => Object.fromEntries(map),
  };
}

function sample(overrides: Partial<PluginRecord> = {}): PluginRecord {
  const now = "2026-08-22T00:00:00.000Z";
  return {
    id: createPluginId(),
    ownerKey: "user-a",
    pluginKey: "plugin.workspace.sample",
    displayName: "Sample",
    summary: "",
    version: "1.0.0",
    categoryId: "cat-tooling",
    supportedHostTools: ["cursor", "codex"],
    contributedCapabilities: ["skills", "hooks"],
    sourceKind: "git",
    gitRepository: "https://github.com/org/plugin.git",
    gitRef: "main",
    artifactRef: null,
    checksumSha256: null,
    archiveFileName: null,
    status: "active",
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

function category(overrides: Partial<PluginCategoryRecord> = {}): PluginCategoryRecord {
  const now = "2026-08-22T00:00:00.000Z";
  return {
    id: createPluginCategoryId(),
    code: "workspace.tooling",
    name: "Workspace tooling",
    description: "",
    sortOrder: 10,
    enabled: true,
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

describe("plugin catalog model", () => {
  it("validates plugin keys", () => {
    expect(isValidPluginKey("plugin.workspace.sample")).toBe(true);
    expect(isValidPluginKey("plugin.a")).toBe(false);
    expect(isValidPluginKey("skill.workspace.sample")).toBe(false);
  });

  it("round-trips catalog snapshots and upserts by plugin key", () => {
    const first = sample({ id: "1", pluginKey: "plugin.workspace.one", updatedAt: "2026-08-22T01:00:00.000Z" });
    const second = sample({ id: "2", pluginKey: "plugin.workspace.two", updatedAt: "2026-08-22T02:00:00.000Z" });
    const replaced = sample({
      id: "3",
      pluginKey: "plugin.workspace.one",
      displayName: "Replaced",
      supportedHostTools: ["claude_code"],
      updatedAt: "2026-08-22T03:00:00.000Z",
    });
    const stored = serializePluginCatalog([first, second]);
    const loaded = parsePluginCatalog(stored, "user-a");
    expect(loaded).toHaveLength(2);
    expect(loaded[0]?.supportedHostTools).toEqual(["cursor", "codex"]);

    const next = upsertPluginRecord(loaded, replaced);
    expect(next).toHaveLength(2);
    expect(next[0]?.displayName).toBe("Replaced");
    expect(next[0]?.supportedHostTools).toEqual(["claude_code"]);
    expect(next.find((item) => item.pluginKey === "plugin.workspace.one")?.id).toBe("3");
    expect(removePluginRecord(next, "3")).toHaveLength(1);
  });

  it("migrates legacy records without tool, owner, or category fields", () => {
    const legacy = {
      id: "legacy-1",
      pluginKey: "plugin.workspace.legacy",
      displayName: "Legacy",
      summary: "",
      version: "1.0.0",
      sourceKind: "git",
      gitRepository: "https://github.com/org/legacy.git",
      gitRef: "main",
      status: "active",
      createdAt: "2026-08-22T00:00:00.000Z",
      updatedAt: "2026-08-22T00:00:00.000Z",
    };
    const normalized = normalizePluginRecord(legacy, "user-a");
    expect(normalized?.supportedHostTools).toEqual([]);
    expect(normalized?.contributedCapabilities).toEqual([]);
    // A legacy row is adopted by the owner reading it, and starts uncategorized
    // so the edit form forces the operator to file it.
    expect(normalized?.ownerKey).toBe("user-a");
    expect(normalized?.categoryId).toBe("");

    const loaded = parsePluginCatalog(JSON.stringify({ version: 1, items: [legacy] }), "user-a");
    expect(loaded[0]?.supportedHostTools).toEqual([]);
    expect(loaded[0]?.ownerKey).toBe("user-a");
  });

  it("drops unknown tool identifiers during normalization", () => {
    expect(normalizePluginHostTools(["cursor", "unknown_host", "codex"])).toEqual(["cursor", "codex"]);
    expect(normalizePluginContributions(["skills", "not_real", "tools"])).toEqual(["skills", "tools"]);
    expect(normalizePluginHostTools(["workbuddy", "zcode"])).toEqual(["workbuddy", "zcode"]);
  });

  it("offers BirdCoder as a selectable agent host", () => {
    expect(PLUGIN_HOST_TOOL_IDS).toContain("birdcoder");
    expect(PLUGIN_HOST_TOOL_GROUPS.find((group) => group.ids.includes("birdcoder"))?.id).toBe(
      "agent",
    );
    // Unknown variants are dropped, the canonical id survives normalization.
    expect(normalizePluginHostTools(["birdcoder", "birdcoder2"])).toEqual(["birdcoder"]);
  });

  it("gives every host tool a chip monogram", () => {
    for (const id of PLUGIN_HOST_TOOL_IDS) {
      expect(PLUGIN_HOST_TOOL_MONOGRAMS[id].length).toBeGreaterThan(0);
    }
  });

  it("groups host tools into a complete, duplicate-free partition", () => {
    const grouped = PLUGIN_HOST_TOOL_GROUPS.flatMap((group) => [...group.ids]).sort();
    expect(new Set(grouped).size).toBe(grouped.length);
    expect(grouped.sort()).toEqual([...PLUGIN_HOST_TOOL_IDS].sort());
    expect(grouped).toContain("workbuddy");
    expect(grouped).toContain("zcode");
  });
});

describe("per-user plugin isolation", () => {
  it("derives a distinct storage key per owner and normalizes blanks", () => {
    expect(resolvePluginCatalogStorageKey("user-a")).toBe(
      "sdkwork.webserver.plugins.catalog.v2.user-a",
    );
    expect(resolvePluginCatalogStorageKey("user-b")).not.toBe(
      resolvePluginCatalogStorageKey("user-a"),
    );
    expect(normalizePluginOwnerKey("  ")).toBe(PLUGIN_CATALOG_ANONYMOUS_OWNER);
    expect(normalizePluginOwnerKey(null)).toBe(PLUGIN_CATALOG_ANONYMOUS_OWNER);
    expect(normalizePluginOwnerKey(" user-a ")).toBe("user-a");
  });

  it("keeps two users' catalogs fully separate in the same browser", () => {
    const storage = memoryStorage();
    savePluginCatalog("user-a", [sample({ id: "a1", ownerKey: "user-a" })], storage);
    savePluginCatalog("user-b", [sample({ id: "b1", ownerKey: "user-b" })], storage);

    const a = loadPluginCatalog("user-a", storage);
    const b = loadPluginCatalog("user-b", storage);
    expect(a.map((item) => item.id)).toEqual(["a1"]);
    expect(b.map((item) => item.id)).toEqual(["b1"]);
    expect(filterPluginRecordsByOwner(a, "user-b")).toHaveLength(0);
  });

  it("does not let one user overwrite another user's identical plugin key", () => {
    const shared = "plugin.workspace.shared";
    const base = [
      sample({ id: "a1", ownerKey: "user-a", pluginKey: shared }),
    ];
    const next = upsertPluginRecord(
      base,
      sample({ id: "b1", ownerKey: "user-b", pluginKey: shared, displayName: "B copy" }),
    );
    expect(next).toHaveLength(2);
    expect(next.map((item) => item.id).sort()).toEqual(["a1", "b1"]);

    // Same owner + same key is still a replace, not an append.
    const replaced = upsertPluginRecord(
      next,
      sample({ id: "a2", ownerKey: "user-a", pluginKey: shared, displayName: "A v2" }),
    );
    expect(replaced).toHaveLength(2);
    expect(replaced.find((item) => item.pluginKey === shared && item.ownerKey === "user-a")?.id).toBe("a2");
  });

  it("adopts the legacy single-tenant catalog exactly once", () => {
    const legacyRow = sample({ id: "legacy", ownerKey: PLUGIN_CATALOG_ANONYMOUS_OWNER });
    const storage = memoryStorage({
      [PLUGIN_CATALOG_LEGACY_STORAGE_KEY]: serializePluginCatalog([legacyRow]),
    });

    const first = loadPluginCatalog("user-a", storage);
    expect(first.map((item) => item.id)).toEqual(["legacy"]);
    // The row is re-owned by the account that consumed the legacy key.
    expect(first[0]?.ownerKey).toBe("user-a");

    // A second account must not inherit the same rows.
    const second = loadPluginCatalog("user-b", storage);
    expect(second).toEqual([]);
  });

  it("stamps a record with the owner even when the caller forgets to", () => {
    const orphan = sample({ ownerKey: "" });
    const next = upsertPluginRecord([], { ...orphan, ownerKey: "user-a" });
    expect(next[0]?.ownerKey).toBe("user-a");
  });
});

describe("plugin list filters", () => {
  const cursorOnly = sample({
    id: "a",
    pluginKey: "plugin.workspace.a",
    categoryId: "cat-tooling",
    supportedHostTools: ["cursor"],
    contributedCapabilities: ["skills"],
  });
  const codexHooks = sample({
    id: "b",
    pluginKey: "plugin.workspace.b",
    categoryId: "cat-docs",
    supportedHostTools: ["codex"],
    contributedCapabilities: ["hooks"],
  });
  const both = sample({
    id: "c",
    pluginKey: "plugin.workspace.c",
    categoryId: "cat-docs",
    supportedHostTools: ["cursor", "deepseek_harness"],
    contributedCapabilities: ["skills", "tools"],
  });
  const items = [cursorOnly, codexHooks, both];

  it("filters by host tools and capabilities with OR semantics within each group", () => {
    expect(filterPluginRecords(items, { categoryIds: [], hostTools: ["deepseek_harness"], capabilities: [] })).toHaveLength(1);
    expect(filterPluginRecords(items, { categoryIds: [], hostTools: ["cursor"], capabilities: [] })).toHaveLength(2);
    expect(filterPluginRecords(items, { categoryIds: [], hostTools: [], capabilities: ["tools"] })).toHaveLength(1);
    expect(filterPluginRecords(items, { categoryIds: [], hostTools: ["codex"], capabilities: ["hooks"] })).toHaveLength(1);
    expect(hasActivePluginFilters(EMPTY_PLUGIN_LIST_FILTERS)).toBe(false);
    expect(hasActivePluginFilters({ categoryIds: [], hostTools: ["cursor"], capabilities: [] })).toBe(true);
  });

  it("filters by category and ANDs it with the other groups", () => {
    expect(filterPluginRecords(items, { categoryIds: ["cat-docs"], hostTools: [], capabilities: [] }))
      .toHaveLength(2);
    expect(filterPluginRecords(items, { categoryIds: ["cat-tooling"], hostTools: [], capabilities: [] }))
      .toHaveLength(1);
    expect(filterPluginRecords(items, {
      categoryIds: ["cat-docs"],
      hostTools: ["cursor"],
      capabilities: [],
    })).toHaveLength(1);
    expect(hasActivePluginFilters({ categoryIds: ["cat-docs"], hostTools: [], capabilities: [] })).toBe(true);
  });
});

describe("plugin categories", () => {
  it("validates and normalizes category codes", () => {
    expect(isValidPluginCategoryCode("workspace.tooling")).toBe(true);
    expect(isValidPluginCategoryCode("ops")).toBe(true);
    expect(isValidPluginCategoryCode("Workspace.Tooling")).toBe(false);
    expect(isValidPluginCategoryCode(".leading")).toBe(false);
    expect(isValidPluginCategoryCode("has space")).toBe(false);
    expect(normalizePluginCategoryCode("  Workspace.Tooling ")).toBe("workspace.tooling");
  });

  it("round-trips a category catalog and drops malformed rows", () => {
    const stored = serializePluginCategories([category({ id: "c1" })]);
    const loaded = parsePluginCategories(stored);
    expect(loaded).toHaveLength(1);
    expect(loaded[0]?.code).toBe("workspace.tooling");
    expect(parsePluginCategories(JSON.stringify({ version: 1, items: [{ id: "x" }] }))).toEqual([]);
    expect(parsePluginCategories("not json")).toEqual([]);
    expect(parsePluginCategories(null)).toEqual([]);
  });

  it("treats a missing enabled flag on legacy rows as still offered", () => {
    const restored = normalizePluginCategoryRecord({
      id: "c1",
      code: "legacy.group",
      name: "Legacy",
    });
    expect(restored?.enabled).toBe(true);
    expect(normalizePluginCategoryRecord({ id: "c2", code: "explicit.off", enabled: false })?.enabled)
      .toBe(false);
  });

  it("sorts by sortOrder then name, and only offers enabled categories", () => {
    const a = category({ id: "a", name: "Beta", sortOrder: 20 });
    const b = category({ id: "b", name: "Alpha", sortOrder: 20 });
    const c = category({ id: "c", name: "First", sortOrder: 5 });
    const retired = category({ id: "d", name: "Zed", sortOrder: 1, enabled: false });
    expect(sortPluginCategories([a, b, c]).map((item) => item.id)).toEqual(["c", "b", "a"]);
    expect(selectablePluginCategories([a, b, c, retired]).map((item) => item.id))
      .toEqual(["c", "b", "a"]);
  });

  it("upserts by id or code so a re-code does not duplicate the row", () => {
    const existing = category({ id: "c1", code: "workspace.tooling" });
    const byId = upsertPluginCategoryRecord([existing], { ...existing, name: "Renamed" });
    expect(byId).toHaveLength(1);
    expect(byId[0]?.name).toBe("Renamed");

    const byCode = upsertPluginCategoryRecord([existing], category({ id: "c2", code: "workspace.tooling" }));
    expect(byCode).toHaveLength(1);
    expect(byCode[0]?.id).toBe("c2");

    expect(removePluginCategoryRecord([existing], "c1")).toEqual([]);
  });

  it("resolves a category by id, including retired ones", () => {
    const retired = category({ id: "c9", name: "Retired", enabled: false });
    expect(findPluginCategory([retired], "c9")?.name).toBe("Retired");
    expect(findPluginCategory([retired], "")).toBeNull();
    expect(findPluginCategory([retired], "missing")).toBeNull();
  });

  it("seeds a usable default catalog", () => {
    const seeded = seedPluginCategories("2026-08-22T00:00:00.000Z");
    expect(seeded).toHaveLength(DEFAULT_PLUGIN_CATEGORIES.length);
    expect(new Set(seeded.map((item) => item.id)).size).toBe(seeded.length);
    expect(new Set(seeded.map((item) => item.code)).size).toBe(seeded.length);
    expect(seeded.every((item) => item.enabled)).toBe(true);
    expect(seeded.every((item) => item.createdAt === "2026-08-22T00:00:00.000Z")).toBe(true);
  });

  it("seeds the platform catalog once and then persists edits", () => {
    const storage = memoryStorage();
    const seeded = loadPluginCategories(storage);
    expect(seeded.length).toBeGreaterThan(0);

    const edited = sortPluginCategories([...seeded, category({ id: "extra", code: "extra.one", sortOrder: 999 })]);
    savePluginCategories(edited, storage);
    const reloaded = loadPluginCategories(storage);
    expect(reloaded.map((item) => item.id)).toContain("extra");
    // A second load must not re-seed or duplicate anything.
    expect(reloaded).toHaveLength(edited.length);
  });
});
