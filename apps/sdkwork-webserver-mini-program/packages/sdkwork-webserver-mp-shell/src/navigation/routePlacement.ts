/**
 * Mini program route placement metadata and projection inputs.
 *
 * Authority: `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §5 and §9. SDKWork packages
 * are source/dependency boundaries; platform `subPackages` are runtime loading
 * and package-size boundaries. The build projects route contributions into root
 * pages and subpackages, so a capability package never hand-maintains `app.json`.
 *
 * Route metadata must not declare API URLs or SDK methods.
 */

export interface MiniProgramRoutePlacement {
  readonly rootPackage?: boolean;
  readonly subpackage?: string;
  readonly pagePath: string;
  readonly preload?: boolean;
}

export type WebserverMpRouteAuth = "public" | "required";

export interface WebserverMpRouteContribution {
  /** `<surface>.<domain>.<capability>.<screen>` — aligned with the other client roots. */
  readonly id: string;
  readonly surface: "app";
  readonly domain: string;
  readonly capability: string;
  readonly screen: string;
  readonly titleKey: string;
  readonly auth: WebserverMpRouteAuth;
  readonly permissionHint?: string;
  /** Navigation slot this route occupies, and its idle position in the tab bar. */
  readonly navigation?: WebserverMpNavigationContribution;
  readonly miniProgram: MiniProgramRoutePlacement;
}

export interface WebserverMpNavigationContribution {
  readonly labelKey: string;
  readonly permission: string;
  readonly order: number;
}

export interface WebserverMpPageProjectionEntry {
  readonly pagePath: string;
  readonly rootPackage: boolean;
  readonly subpackage?: string;
  readonly preload?: boolean;
}

export interface WebserverMpSubPackageProjection {
  readonly root: string;
  readonly pages: readonly string[];
}

/**
 * Projects route contributions into the ordered mini program `pages` list.
 * Root-package routes come first and keep their declared order; subpackage pages
 * are assembled by `projectWebserverMpSubPackages`.
 */
export function projectWebserverMpPages(
  routes: readonly WebserverMpRouteContribution[],
): WebserverMpPageProjectionEntry[] {
  return routes.map((route) => ({
    pagePath: route.miniProgram.pagePath,
    rootPackage: route.miniProgram.rootPackage === true,
    ...(route.miniProgram.subpackage ? { subpackage: route.miniProgram.subpackage } : {}),
    ...(route.miniProgram.preload === true ? { preload: true } : {}),
  }));
}

export function listWebserverMpRootPages(
  routes: readonly WebserverMpRouteContribution[],
): string[] {
  return projectWebserverMpPages(routes)
    .filter((entry) => entry.rootPackage)
    .map((entry) => entry.pagePath);
}

export function projectWebserverMpSubPackages(
  routes: readonly WebserverMpRouteContribution[],
): WebserverMpSubPackageProjection[] {
  const grouped = new Map<string, string[]>();
  for (const entry of projectWebserverMpPages(routes)) {
    if (!entry.subpackage || entry.rootPackage) continue;
    const pages = grouped.get(entry.subpackage) ?? [];
    pages.push(entry.pagePath);
    grouped.set(entry.subpackage, pages);
  }
  return [...grouped.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([root, pages]) => ({ root, pages }));
}

/** Validate the canonical route id shape and the placement/identity agreement. */
export function validateWebserverMpRouteContributions(
  routes: readonly WebserverMpRouteContribution[],
): string[] {
  const issues: string[] = [];
  const seenIds = new Set<string>();
  const seenPaths = new Set<string>();
  for (const route of routes) {
    const expectedId = `${route.surface}.${route.domain}.${route.capability}.${route.screen}`;
    if (route.id !== expectedId) {
      issues.push(`route id ${route.id} must equal ${expectedId}`);
    }
    if (seenIds.has(route.id)) {
      issues.push(`duplicate mini program route id ${route.id}`);
    }
    seenIds.add(route.id);
    if (seenPaths.has(route.miniProgram.pagePath)) {
      issues.push(`duplicate mini program page path ${route.miniProgram.pagePath}`);
    }
    seenPaths.add(route.miniProgram.pagePath);
    if (route.miniProgram.rootPackage !== true && !route.miniProgram.subpackage) {
      issues.push(`route ${route.id} must declare rootPackage or subpackage placement`);
    }
    if (route.navigation && !route.permissionHint) {
      issues.push(`route ${route.id} contributes navigation but declares no permissionHint`);
    }
  }
  return issues;
}
