/**
 * Web Server H5 navigation model.
 *
 * Route ownership lives in the shell so the application root only assembles
 * entries into a router and a feature package never has to know the mobile URL
 * it happens to live under. Physical H5 paths stay short (`/applications`) while
 * the route **id** keeps the cross-client capability segment stable
 * (`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §2), which is what lets the same
 * screen stay addressable on PC, H5, mini-program, Flutter, and Harmony.
 */

/**
 * Cross-client capability segment of the Web Server application surface.
 *
 * It matches the PC console resource key and the `deploy.apps.read` permission
 * resource, so a navigation entry and its permission hint never drift apart.
 */
export const APPLICATIONS_ROUTE = "applications" as const;

/** Mobile path of the applications surface. */
export const APPLICATIONS_PATH = `/${APPLICATIONS_ROUTE}` as const;

export interface WebserverH5NavigationEntry {
  /** Cross-client capability segment; identical on every client surface. */
  readonly id: string;
  /** Absolute H5 path the application root registers in its router. */
  readonly path: string;
  /** Catalog key resolved by the application root, never inline copy. */
  readonly labelKey: string;
  /**
   * Permission required to see the entry, using the same `[resource].[action]`
   * shape the gateway derives from `operationId`. The shell only carries the
   * hint; the root decides whether the viewer holds it.
   */
  readonly permission: string;
  /** Ascending display order in the tab bar. */
  readonly order: number;
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
    id: APPLICATIONS_ROUTE,
    path: APPLICATIONS_PATH,
    labelKey: "navigation.applications",
    permission: "deploy.apps.read",
    order: 10,
  },
];

/** Path the router falls back to when nothing matched. */
export const WEBSERVER_H5_HOME_PATH = APPLICATIONS_PATH;

export function createWebserverH5Navigation(
  contributions: readonly (readonly WebserverH5NavigationEntry[])[],
): readonly WebserverH5NavigationEntry[] {
  const merged = contributions.flat();
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
  return entries.filter((entry) => hasPermission(entry.permission));
}
