/**
 * Web Server H5 navigation model.
 *
 * Route ownership lives in the shell so the application root only assembles
 * entries into a router and a feature package never has to know the mobile URL
 * it happens to live under. Physical H5 paths stay short (`/applications`)
 * while `id` carries the canonical cross-client identity
 * (`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` section 7):
 *
 * ```text
 * <surface>.<domain>.<capability>.<screen>
 * ```
 *
 * Section 7: "SDKWork client routes are aligned by route identity, not by
 * identical physical paths", and the `id`, `surface`, `domain`, `capability`,
 * `screen`, `titleKey`, and `permissionHint` tuple must stay consistent across
 * PC, H5, Flutter, mini program, Android, iOS, and Harmony implementations of
 * one workflow.
 *
 * History: this module previously used the bare capability segment
 * (`applications`) as the route `id`, citing the cross-client capability
 * segment. Section 7 owns route identity and mandates four segments, so the id
 * was migrated and `createWebserverH5Navigation` now refuses a non-canonical id
 * rather than letting the drift reappear.
 */

/** Cross-client surface segment for every application-surface H5 route. */
export const WEBSERVER_H5_ROUTE_SURFACE = "app" as const;

/** Cross-client domain segment for Web Server routes. */
export const WEBSERVER_H5_ROUTE_DOMAIN = "webserver" as const;

/**
 * Cross-client capability segment of the applications workflow.
 *
 * It matches the PC console resource key and the `deploy.apps.read` permission
 * resource, so a navigation entry and its permission hint never drift apart.
 * The physical H5 path is derived from it; the route **id** is not.
 */
export const APPLICATIONS_ROUTE = "applications" as const;

/** Cross-client screen segment of the applications list workflow. */
export const APPLICATIONS_SCREEN = "list" as const;

/** Canonical cross-client route id of the applications list workflow. */
export const APPLICATIONS_ROUTE_ID =
  `${WEBSERVER_H5_ROUTE_SURFACE}.${WEBSERVER_H5_ROUTE_DOMAIN}.${APPLICATIONS_ROUTE}.${APPLICATIONS_SCREEN}` as const;

/** Mobile path of the applications surface. */
export const APPLICATIONS_PATH = `/${APPLICATIONS_ROUTE}` as const;

export interface WebserverH5NavigationEntry {
  /** Canonical route id: `<surface>.<domain>.<capability>.<screen>`. */
  readonly id: string;
  /** Cross-client surface segment; identical on every client surface. */
  readonly surface: string;
  /** Cross-client domain segment. */
  readonly domain: string;
  /** Cross-client capability segment. */
  readonly capability: string;
  /** Cross-client screen segment. */
  readonly screen: string;
  /** Absolute H5 path the application root registers in its router. */
  readonly path: string;
  /** Catalog key of the tab-bar label, never inline copy. */
  readonly labelKey: string;
  /** Catalog key of the screen title, shared with the other client roots. */
  readonly titleKey: string;
  /**
   * Permission required to see the entry, using the same `[resource].[action]`
   * shape the gateway derives from `operationId`. The shell only carries the
   * hint; the root decides whether the viewer holds it.
   */
  readonly permissionHint: string;
  /** Ascending display order in the tab bar. */
  readonly order: number;
}

export function validateWebserverH5NavigationEntries(
  entries: readonly WebserverH5NavigationEntry[],
): readonly string[] {
  const issues: string[] = [];
  for (const entry of entries) {
    const expectedId = `${entry.surface}.${entry.domain}.${entry.capability}.${entry.screen}`;
    if (entry.id !== expectedId) {
      issues.push(`route id ${entry.id} must equal ${expectedId}`);
    }
    if (entry.path.trim().length === 0) {
      issues.push(`route ${entry.id} must declare a path`);
    }
    if (entry.titleKey.trim().length === 0) {
      issues.push(`route ${entry.id} must declare a titleKey`);
    }
    if (entry.permissionHint.trim().length === 0) {
      issues.push(`route ${entry.id} must declare a permissionHint`);
    }
  }
  return issues;
}

/**
 * Entries the H5 shell itself owns: chrome-level destinations that are not the
 * product of any single feature package.
 */
export const WEBSERVER_H5_SHELL_NAVIGATION: readonly WebserverH5NavigationEntry[] = [];

/**
 * The applications entry contributed by `@sdkwork/webserver-h5-applications`.
 *
 * It is declared here rather than inside the feature package so the shell stays
 * the single place that knows the ordered navigation model, and the feature
 * package stays the single place that knows how the screen is rendered.
 */
export const WEBSERVER_H5_APPLICATIONS_NAVIGATION: readonly WebserverH5NavigationEntry[] = [
  {
    id: APPLICATIONS_ROUTE_ID,
    surface: WEBSERVER_H5_ROUTE_SURFACE,
    domain: WEBSERVER_H5_ROUTE_DOMAIN,
    capability: APPLICATIONS_ROUTE,
    screen: APPLICATIONS_SCREEN,
    path: APPLICATIONS_PATH,
    labelKey: "navigation.applications",
    titleKey: "applications.list.title",
    permissionHint: "deploy.apps.read",
    order: 10,
  },
];

/** Path the router falls back to when nothing matched. */
export const WEBSERVER_H5_HOME_PATH = APPLICATIONS_PATH;

export function createWebserverH5Navigation(
  contributions: readonly (readonly WebserverH5NavigationEntry[])[],
): readonly WebserverH5NavigationEntry[] {
  const merged = contributions.flat();

  const issues = validateWebserverH5NavigationEntries(merged);
  if (issues.length > 0) {
    throw new Error(issues.join("; "));
  }

  const seenIds = new Set<string>();
  const seenPaths = new Set<string>();
  for (const entry of merged) {
    if (seenIds.has(entry.id)) {
      throw new Error(`duplicate H5 navigation id: ${entry.id}`);
    }
    if (seenPaths.has(entry.path)) {
      throw new Error(`duplicate H5 navigation path: ${entry.path}`);
    }
    seenIds.add(entry.id);
    seenPaths.add(entry.path);
  }
  return [...merged].sort((left, right) => left.order - right.order);
}

/** Entries the viewer may open, in tab-bar order. */
export function filterNavigationForPermission(
  entries: readonly WebserverH5NavigationEntry[],
  hasPermission: (permission: string) => boolean,
): readonly WebserverH5NavigationEntry[] {
  return entries.filter((entry) => hasPermission(entry.permissionHint));
}
