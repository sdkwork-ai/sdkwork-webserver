import type { WebserverMessageKey } from "./i18n/index.ts";
import type { WebserverResourceKey } from "./types.ts";

/**
 * Sidebar sections: a purely **presentational grouping** of the workspace menu.
 *
 * The flat resource routes never change — a section only decides where a menu
 * item is drawn inside the sidebar. Entries owned by no declared section are
 * rendered in the leading, unlabelled group, so an ungrouped resource keeps its
 * exact former position and no deep link moves.
 *
 * Membership is declared by resource key rather than by path prefix on purpose:
 * the same key (`skills`) is reachable both as `/console/skills` (self-service)
 * and `/admin/skills` (operations) depending on the surface, and both should
 * land in the same section.
 */
export interface WebserverMenuSectionDefinition {
  id: WebserverMenuSectionId;
  labelKey: WebserverMessageKey;
  /** Resources drawn under this section, in declaration order. */
  resources: readonly WebserverResourceKey[];
}

export type WebserverMenuSectionId = "aiEcosystem";

/** Sidebar section order is the declaration order of this array. */
export const MENU_SECTIONS: readonly WebserverMenuSectionDefinition[] = [
  {
    id: "aiEcosystem",
    labelKey: "menuSection.aiEcosystem",
    // The user-owned AI assets: Skills, MCP servers, plugins, and the platform
    // plugin-category catalog those plugins file under. Grouped at the bottom
    // of the sidebar, after the delivery and server resources.
    resources: ["plugins", "plugin-categories", "skills", "mcp"],
  },
];

/**
 * Splits menu entries into the ungrouped leading group plus one group per
 * declared section. The ungrouped group preserves the supplied entry order (a
 * caller's `order`-sorted menu); a declared section follows **its own**
 * `resources` order so the section reads the same regardless of how the caller
 * sorted the flat menu.
 *
 * Empty sections are dropped: an operator without the AI-ecosystem resources
 * must not see a dangling section heading.
 */
export interface WebserverMenuGroup<TEntry> {
  id: WebserverMenuSectionId | null;
  labelKey: WebserverMessageKey | null;
  entries: readonly TEntry[];
}

export function groupMenuEntries<TEntry extends { resource: string }>(
  entries: readonly TEntry[],
): readonly WebserverMenuGroup<TEntry>[] {
  const claimed = new Set<string>(MENU_SECTIONS.flatMap((section) => [...section.resources]));
  const groups: WebserverMenuGroup<TEntry>[] = [{
    id: null,
    labelKey: null,
    entries: entries.filter((entry) => !claimed.has(entry.resource)),
  }];
  for (const section of MENU_SECTIONS) {
    const byResource = new Map(entries.map((entry) => [entry.resource, entry]));
    const sectionEntries = section.resources
      .map((resource) => byResource.get(resource))
      .filter((entry): entry is TEntry => entry !== undefined);
    if (sectionEntries.length === 0) continue;
    groups.push({ id: section.id, labelKey: section.labelKey, entries: sectionEntries });
  }
  return groups;
}

/**
 * Top-level admin modules, rendered as tabs in the workspace header.
 *
 * The backend-admin surface keeps its canonical flat routes
 * (`/admin/<resource>`): a module is a **navigation grouping** over those
 * paths, never an extra URL segment. Membership is therefore derived from the
 * path prefixes a module owns, so every existing deep link stays valid and a
 * new module only has to claim its own subtree.
 *
 * Resources are partitioned by *longest matching prefix*, which lets a narrow
 * module (`/admin/storage`) be carved out of the catch-all `home` module
 * (`/admin`) without enumerating what stays behind.
 */
export type WebserverAdminModuleId = "home" | "storageCenter" | "clusterCenter";

export interface WebserverAdminModuleDefinition {
  id: WebserverAdminModuleId;
  labelKey: WebserverMessageKey;
  descriptionKey: WebserverMessageKey;
  /**
   * Path prefixes owned by the module. The **longest** match wins, so `home`
   * can remain the catch-all owner of `/admin` while narrower modules take
   * their own subtree away from it.
   */
  pathPrefixes: readonly string[];
}

export const DEFAULT_ADMIN_MODULE_ID: WebserverAdminModuleId = "home";

/** Header tab order is the declaration order of this array. */
export const ADMIN_MODULES: readonly WebserverAdminModuleDefinition[] = [
  {
    id: "home",
    labelKey: "module.home",
    descriptionKey: "module.home.description",
    // Catch-all owner: every backend-admin path not claimed by a narrower module.
    pathPrefixes: ["/admin"],
  },
  {
    id: "storageCenter",
    labelKey: "module.storageCenter",
    descriptionKey: "module.storageCenter.description",
    pathPrefixes: ["/admin/storage"],
  },
  {
    id: "clusterCenter",
    labelKey: "module.clusterCenter",
    descriptionKey: "module.clusterCenter.description",
    pathPrefixes: ["/admin/cluster"],
  },
];

const ADMIN_MODULE_PREFIXES: readonly { moduleId: WebserverAdminModuleId; prefix: string }[] =
  ADMIN_MODULES.flatMap((module) =>
    module.pathPrefixes.map((prefix) => ({ moduleId: module.id, prefix })),
  );

/**
 * Minimal shape the grouping needs from a surface entry. Kept structural so the
 * registry stays independent of `WebserverModuleEntry`'s other fields.
 */
export interface AdminModuleEntryLike {
  /**
   * Route segment under the surface base path. Defaults to `resource`; a module
   * whose resources are grouped under a sub-path (`/admin/storage/providers`)
   * sets it without having to encode the slash in the resource key.
   */
  path?: string;
  resource: string;
}

/** Route segment an entry occupies. */
export function adminEntryPathSegment(entry: AdminModuleEntryLike): string {
  return entry.path?.trim() || entry.resource;
}

/** Absolute route path for an entry within a surface (`<basePath>/<segment>`). */
export function resolveAdminEntryPath(basePath: string, entry: AdminModuleEntryLike): string {
  return `${normalizeBasePath(basePath)}/${adminEntryPathSegment(entry)}`;
}

export function resolveAdminModuleDefinition(
  moduleId: WebserverAdminModuleId,
): WebserverAdminModuleDefinition {
  const module = ADMIN_MODULES.find((candidate) => candidate.id === moduleId);
  if (!module) {
    throw new Error(`unknown admin module: ${moduleId}`);
  }
  return module;
}

/**
 * Resolves the module that owns `pathname`.
 *
 * Matching is segment-aware (`/admin/storage` never matches `/admin/storage-x`)
 * and resolution is longest-prefix-wins, which keeps `home` as the catch-all
 * owner of `/admin`.
 */
export function resolveAdminModuleFromPath(pathname: string): WebserverAdminModuleId {
  let owner: WebserverAdminModuleId = DEFAULT_ADMIN_MODULE_ID;
  let ownerPrefixLength = -1;
  for (const candidate of ADMIN_MODULE_PREFIXES) {
    if (!matchesAdminPathPrefix(pathname, candidate.prefix)) continue;
    if (candidate.prefix.length <= ownerPrefixLength) continue;
    owner = candidate.moduleId;
    ownerPrefixLength = candidate.prefix.length;
  }
  return owner;
}

/** Resolves the module that owns an entry's route within a surface. */
export function resolveAdminModuleForEntry(
  basePath: string,
  entry: AdminModuleEntryLike,
): WebserverAdminModuleId {
  return resolveAdminModuleFromPath(resolveAdminEntryPath(basePath, entry));
}

export interface AdminModuleEntryGroup<TEntry> {
  moduleId: WebserverAdminModuleId;
  /** Visible entries owned by the module, in the order they were supplied. */
  entries: readonly TEntry[];
}

/**
 * Partitions entries into one group **per declared module**, preserving both
 * the module declaration order (header tab order) and the supplied entry order
 * (a caller's `order`-sorted menu) inside each group.
 *
 * Empty groups are returned rather than dropped: the caller decides whether an
 * empty module still deserves a tab, which keeps this function a pure
 * partition instead of a policy.
 */
export function groupAdminModuleEntries<TEntry extends AdminModuleEntryLike>(
  basePath: string,
  entries: readonly TEntry[],
): readonly AdminModuleEntryGroup<TEntry>[] {
  const buckets = new Map<WebserverAdminModuleId, TEntry[]>(
    ADMIN_MODULES.map((module) => [module.id, []]),
  );
  for (const entry of entries) {
    const moduleId = resolveAdminModuleForEntry(basePath, entry);
    buckets.get(moduleId)?.push(entry);
  }
  return ADMIN_MODULES.map((module) => ({
    moduleId: module.id,
    entries: buckets.get(module.id) ?? [],
  }));
}

/**
 * Landing path for a module tab: the first entry the operator can actually
 * open, so a permission-filtered menu never sends them to a page they cannot
 * see.
 */
export function resolveAdminModuleLandingPath(
  basePath: string,
  entries: readonly AdminModuleEntryLike[],
): string | undefined {
  const first = entries[0];
  return first ? resolveAdminEntryPath(basePath, first) : undefined;
}

function normalizeBasePath(basePath: string): string {
  const trimmed = basePath.replace(/\/+$/, "");
  return trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
}

function matchesAdminPathPrefix(pathname: string, prefix: string): boolean {
  if (pathname === prefix) return true;
  return pathname.startsWith(`${prefix}/`);
}
