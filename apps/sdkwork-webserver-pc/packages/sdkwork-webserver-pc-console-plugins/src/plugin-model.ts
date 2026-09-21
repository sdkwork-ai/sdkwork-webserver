import { uuid } from "@sdkwork/utils/id";
import {
  normalizePluginContributions,
  normalizePluginHostTools,
  type PluginContributionKind,
  type PluginHostToolId,
} from "./plugin-tool-catalog.ts";

/**
 * Legacy single-tenant key. Read once to migrate an existing catalog onto the
 * per-owner key so no plugin disappears when ownership isolation lands.
 */
export const PLUGIN_CATALOG_LEGACY_STORAGE_KEY = "sdkwork.webserver.plugins.catalog.v1";
export const PLUGIN_CATALOG_STORAGE_PREFIX = "sdkwork.webserver.plugins.catalog.v2";
/**
 * Sentinel owner used when no IAM subject is known (server-side render, unit
 * tests, or a not-yet-authenticated surface). Never a real user id.
 */
export const PLUGIN_CATALOG_ANONYMOUS_OWNER = "anonymous";

export const PLUGIN_KEY_PATTERN = /^plugin\.[a-z][a-z0-9-]*(\.[a-z0-9-]+)+$/;
export const PLUGIN_GIT_REF_MAX_LENGTH = 200;

export type PluginSourceKind = "git" | "archive";
export type PluginStatus = "active" | "draft";

export interface PluginRecord {
  id: string;
  /**
   * Owning IAM subject. Every user has their own plugin CRUD scope, so this is
   * the isolation key the catalog reads and writes are filtered by.
   */
  ownerKey: string;
  pluginKey: string;
  displayName: string;
  summary: string;
  version: string;
  /**
   * Platform-curated category this plugin is filed under. Required on create
   * (PLUGIN_CATEGORY_* in plugin-category.ts owns the catalog).
   */
  categoryId: string;
  /** Agent hosts that can load this plugin bundle (Codex, Claude Code, Cursor, …). */
  supportedHostTools: PluginHostToolId[];
  /** Bundle contributions declared by the plugin manifest. */
  contributedCapabilities: PluginContributionKind[];
  sourceKind: PluginSourceKind;
  gitRepository: string | null;
  gitRef: string | null;
  artifactRef: string | null;
  checksumSha256: string | null;
  archiveFileName: string | null;
  status: PluginStatus;
  createdAt: string;
  updatedAt: string;
}

/**
 * Per-owner storage key. `localStorage` is shared by every account that signs
 * into the same browser, so keying by the IAM subject is what keeps one user's
 * plugins invisible to the next.
 */
export function resolvePluginCatalogStorageKey(ownerKey: string): string {
  const owner = normalizePluginOwnerKey(ownerKey);
  return `${PLUGIN_CATALOG_STORAGE_PREFIX}.${owner}`;
}

export function normalizePluginOwnerKey(ownerKey: string | null | undefined): string {
  const trimmed = ownerKey?.trim();
  return trimmed ? trimmed : PLUGIN_CATALOG_ANONYMOUS_OWNER;
}

export interface PluginCatalogSnapshot {
  version: 1;
  items: PluginRecord[];
}

export function isValidPluginKey(value: string): boolean {
  return PLUGIN_KEY_PATTERN.test(value.trim());
}

export function normalizePluginGitRef(value: string | undefined): string | null {
  const ref = value?.trim() ?? "";
  if (!ref) return "main";
  if (ref.length > PLUGIN_GIT_REF_MAX_LENGTH) {
    throw new Error(`Git ref must not exceed ${PLUGIN_GIT_REF_MAX_LENGTH} characters`);
  }
  if (!/^[A-Za-z0-9._\-/\u4e00-\u9fff]+$/.test(ref)) {
    throw new Error("Git ref contains unsupported characters");
  }
  return ref;
}

export function createPluginId(): string {
  return uuid();
}

export function normalizePluginRecord(
  value: unknown,
  ownerKey: string = PLUGIN_CATALOG_ANONYMOUS_OWNER,
): PluginRecord | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<PluginRecord>;
  if (typeof record.id !== "string"
    || typeof record.pluginKey !== "string"
    || typeof record.displayName !== "string"
    || (record.sourceKind !== "git" && record.sourceKind !== "archive")) {
    return null;
  }
  return {
    id: record.id,
    ownerKey: normalizePluginOwnerKey(
      typeof record.ownerKey === "string" ? record.ownerKey : ownerKey,
    ),
    pluginKey: record.pluginKey,
    displayName: record.displayName,
    summary: typeof record.summary === "string" ? record.summary : "",
    version: typeof record.version === "string" ? record.version : "1.0.0",
    categoryId: typeof record.categoryId === "string" ? record.categoryId : "",
    supportedHostTools: normalizePluginHostTools(record.supportedHostTools),
    contributedCapabilities: normalizePluginContributions(record.contributedCapabilities),
    sourceKind: record.sourceKind,
    gitRepository: typeof record.gitRepository === "string" ? record.gitRepository : null,
    gitRef: typeof record.gitRef === "string" ? record.gitRef : null,
    artifactRef: typeof record.artifactRef === "string" ? record.artifactRef : null,
    checksumSha256: typeof record.checksumSha256 === "string" ? record.checksumSha256 : null,
    archiveFileName: typeof record.archiveFileName === "string" ? record.archiveFileName : null,
    status: record.status === "draft" ? "draft" : "active",
    createdAt: typeof record.createdAt === "string" ? record.createdAt : new Date().toISOString(),
    updatedAt: typeof record.updatedAt === "string" ? record.updatedAt : new Date().toISOString(),
  };
}

export function parsePluginCatalog(
  raw: string | null,
  ownerKey: string = PLUGIN_CATALOG_ANONYMOUS_OWNER,
): PluginRecord[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as PluginCatalogSnapshot | PluginRecord[];
    const items = Array.isArray(parsed) ? parsed : parsed.items;
    if (!Array.isArray(items)) return [];
    return items
      .map((item) => normalizePluginRecord(item, ownerKey))
      .filter((item): item is PluginRecord => item != null);
  } catch {
    return [];
  }
}

export function serializePluginCatalog(items: readonly PluginRecord[]): string {
  const snapshot: PluginCatalogSnapshot = { version: 1, items: [...items] };
  return JSON.stringify(snapshot);
}

export function isPluginRecord(value: unknown): value is PluginRecord {
  return normalizePluginRecord(value) != null;
}
