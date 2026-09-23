import { useSdkworkAuthControllerState } from "@sdkwork/auth-pc-react";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { DeployAppsAdminSurface, webserverModule as appsAdminModule } from "@sdkwork/webserver-pc-admin-apps";
import { webserverModule as auditModule } from "@sdkwork/webserver-pc-admin-audit";
import { CloudAccountAdminSurface, webserverModule as cloudAccountAdminModule } from "@sdkwork/webserver-pc-admin-cloud-account";
import { DashboardAdminSurface, TrafficStatisticsAdminSurface, webserverModule as dataStatisticsAdminModule } from "@sdkwork/webserver-pc-admin-data-statistics";
import { ClusterOverviewSurface, webserverModule as clusterModule } from "@sdkwork/webserver-pc-admin-cluster";
import { ServedCertificateAdminSurface, ServedDomainAdminSurface, webserverModule as deliveryAdminModule } from "@sdkwork/webserver-pc-admin-delivery";
import { webserverModule as diagnosticsModule } from "@sdkwork/webserver-pc-admin-diagnostics";
import { webserverModule as mcpAdminModule, McpAdminSurface, type McpAdminSurfaceProps } from "@sdkwork/webserver-pc-admin-mcp";
import { webserverModule as pluginsAdminModule, PluginsAdminSurface, type PluginsAdminSurfaceProps } from "@sdkwork/webserver-pc-admin-plugins";
import { webserverModule as skillsAdminModule, SkillsAdminSurface, type SkillsAdminSurfaceProps } from "@sdkwork/webserver-pc-admin-skills";
import { StorageCenterSurface, webserverModule as storageModule, type StorageCenterResource } from "@sdkwork/webserver-pc-admin-storage";
import { hasWebserverAdminAccess, type WebserverPcModuleDefinition, type WebserverPcSurface } from "@sdkwork/webserver-pc-commons";
import type { WebserverLocale } from "@sdkwork/webserver-pc-core";
import { CloudAccountManagementSurface, webserverModule as cloudAccountModule } from "@sdkwork/webserver-pc-console-cloud-account";
import { DashboardSurface, TrafficStatisticsSurface, webserverModule as dataStatisticsModule } from "@sdkwork/webserver-pc-console-data-statistics";
import { WebserverConsoleSdkProvider } from "@sdkwork/webserver-pc-console-core";
import { DeployAppsManagementSurface, DeployDomainManagementSurface, webserverModule as deliveryModule } from "@sdkwork/webserver-pc-console-delivery";
import { webserverModule as mcpModule, McpConsoleSurface, type McpConsoleSurfaceProps } from "@sdkwork/webserver-pc-console-mcp";
import { webserverModule as pluginsModule, PluginsConsoleSurface, type PluginsConsoleSurfaceProps } from "@sdkwork/webserver-pc-console-plugins";
import { WebserverConsoleShell } from "@sdkwork/webserver-pc-console-shell";
import { webserverModule as skillsModule, SkillsConsoleSurface, type SkillsConsoleSurfaceProps } from "@sdkwork/webserver-pc-console-skills";
import { lazy, Suspense, use, useMemo } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import type { BootstrappedWebserverPcRuntime } from "../bootstrap/runtime.ts";
import { webserverApplicationCatalog } from "../i18n/index.ts";
import { useSdkworkModuleMessages } from "@sdkwork/i18n-pc-react";

// Applications, domains, and certificates all render the canonical
// sdkwork-deployments pages over `deploy_app`, `deploy_domain_zone`, and the
// certificate entities. `delivery` therefore owns those three menu entries and
// nothing else: the Web Server console keeps no local application lifecycle.
// These are the *per-user* halves of each story — an authenticated operator
// manages the applications, domains, and certificates they own.
//
// Exported, together with `adminModules` below, so the real-browser acceptance
// harness mounts the menu this file actually declares rather than a copy of it:
// the failure this guards against is exactly a module that was implemented and
// tested but never added here, which a harness carrying its own list of modules
// reproduces instead of catching.
export const consoleModules = [deliveryModule, pluginsModule, skillsModule, mcpModule, cloudAccountModule, dataStatisticsModule] satisfies readonly WebserverPcModuleDefinition[];
// Domains and Certificates reappear here at the *tenant* level: the served root
// domains / subdomains this edge answers for (reconciled from its configuration
// at startup) and the TLS certificates over them. That is a different plane from
// the console pair above — the root table has no `user_id` at all and the
// reconciled subdomains carry `user_id IS NULL` — so it is its own module on the
// operations surface rather than folded into `appsAdminModule`, and only this
// surface offers the whole-tenant edge inventory.
// Nginx, Servers, Server Files, and Server Config are deliberately absent. The
// edge is deployed as a cluster, so the per-node shapes those four menus
// described are no longer the operational model: a single nginx runtime's
// config/validate/reload cycle, a hand-maintained inventory of machines, and
// SSH-scoped browsing or online editing over one node's deployment tree. The
// cluster plane owns that ground now — hosts, instances, and their liveness are
// read off `clusterModule` (its own `clusterCenter` tab), which is what an
// operator actually remediates. Re-adding any of the four here would reintroduce
// a single-node control surface next to the cluster one, which is the duplicate
// the removal is meant to end.
export const adminModules = [appsAdminModule, deliveryAdminModule, cloudAccountAdminModule, clusterModule, diagnosticsModule, auditModule, pluginsAdminModule, skillsAdminModule, mcpAdminModule, storageModule, dataStatisticsAdminModule] satisfies readonly WebserverPcModuleDefinition[];
const LazyAdminSurface = lazy(() => import("./WebserverAdminSurface.tsx").then((module) => ({ default: module.WebserverAdminSurface })));

export interface TrafficPageRendererInput {
  backendApiBaseUrl: string;
  locale: WebserverLocale;
  permissionScope: readonly string[];
  surface: WebserverPcSurface;
  tokenManager: AuthTokenManager;
}

/**
 * The renderer entries for the two traffic pages, for the surface asking.
 *
 * The pages are one implementation with two reaches, and which reach a surface
 * gets is the product's own rule rather than a per-mount choice: the console
 * reads the caller's own tenant, the operations surface reads every tenant this
 * edge serves, because the platform operation is restricted to the operator
 * tenant (a tenant-bound caller gets `40301` rather than a wider reading). So
 * the surface decides which *pair of symbols* is mounted, and each symbol binds
 * its reach — nothing here is a prop a caller could point at the other pair.
 *
 * Extracted and exported so the acceptance harness mounts this same map. A
 * harness that rebuilt it would still pass while the host's copy was missing an
 * entry, and "implemented, tested, never mounted" is the failure mode this
 * module has already been through once.
 */
export function trafficPageRenderers({ backendApiBaseUrl, locale, permissionScope, surface, tokenManager }: TrafficPageRendererInput) {
  const admin = surface === "backend-admin";
  const Dashboard = admin ? DashboardAdminSurface : DashboardSurface;
  const TrafficStatistics = admin ? TrafficStatisticsAdminSurface : TrafficStatisticsSurface;
  return {
    dashboard: <Dashboard backendApiBaseUrl={backendApiBaseUrl} locale={locale} permissionScope={permissionScope} resource="dashboard" tokenManager={tokenManager} />,
    "traffic-usage": <TrafficStatistics backendApiBaseUrl={backendApiBaseUrl} locale={locale} permissionScope={permissionScope} resource="traffic-usage" tokenManager={tokenManager} />,
  };
}

export function WebserverAuthorizedWorkspace({ locale, runtime }: { locale: WebserverLocale; runtime: BootstrappedWebserverPcRuntime }) {
  const messages = useSdkworkModuleMessages(webserverApplicationCatalog);
  const authState = useSdkworkAuthControllerState(runtime.authController);
  const consoleClients = use(runtime.loadConsoleClients());
  const permissionScope = authState.session?.context?.permissionScope ?? [];
  // Storage Center is a tenant-scoped drive plane: the shared pages take the
  // tenant they administer and the operator their mutations are attributed to,
  // so the host projects those two facts out of the IAM session instead of
  // handing over the whole session object.
  const tenantId = authState.session?.context?.tenantId ?? "";
  const operatorId = authState.session?.context?.userId ?? authState.user?.id ?? "";
  const adminAccess = hasWebserverAdminAccess(permissionScope);
  const landingPath = adminAccess ? "/admin" : "/console";
  const userLabel = authState.user?.displayName || authState.user?.email;
  const signOut = () => { void runtime.authController.signOut(); };
  // Applications, domains, and certificates are the canonical sdkwork-deployments
  // surfaces; the menu entries stay in this host and the pages take the shared
  // IAM session plus the two generated clients built from the injected token
  // manager. There is no local re-implementation on either side of the console.
  const deployBaseUrl = runtime.config.deployAppApiBaseUrl;
  const driveBaseUrl = runtime.config.driveAppApiBaseUrl;
  const resourceRenderers = {
    apps: <DeployAppsManagementSurface deployBaseUrl={deployBaseUrl} driveBaseUrl={driveBaseUrl} locale={locale} tokenManager={runtime.tokenManager} />,
    domains: <DeployDomainManagementSurface deployBaseUrl={deployBaseUrl} driveBaseUrl={driveBaseUrl} locale={locale} resource="domains" tokenManager={runtime.tokenManager} />,
    certificates: <DeployDomainManagementSurface deployBaseUrl={deployBaseUrl} driveBaseUrl={driveBaseUrl} locale={locale} resource="certificates" tokenManager={runtime.tokenManager} />,
    // Plugins / Skills / MCP are module self-service surfaces; menu entries stay
    // in the host while pages share the IAM dual-token session via tokenManager.
    // Plugins additionally receive the IAM subject so each user reads/writes
    // their own plugin catalog.
    plugins: <PluginsConsoleSurface attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as PluginsConsoleSurfaceProps["attachSdkClientBoundaries"]} driveAppApiBaseUrl={driveBaseUrl} locale={locale} ownerKey={operatorId} resource="plugins" tokenManager={runtime.tokenManager} />,
    skills: <SkillsConsoleSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as SkillsConsoleSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} locale={locale} resource="skills" tokenManager={runtime.tokenManager} />,
    mcp: <McpConsoleSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as McpConsoleSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} locale={locale} resource="mcp" tokenManager={runtime.tokenManager} />,
    // The cloud account center is an IAM-owned resource served by the IAM backend
    // API, so the page comes from the IAM capability package and this host injects
    // only the session scope. The IAM service facade driving it is composed by
    // console-core and published through the SDK provider wrapping these routes,
    // which is also what keeps the generated IAM clients out of a capability
    // package (`verify-repo` forbids that import).
    "cloud-accounts": <CloudAccountManagementSurface permissionScope={permissionScope} tenantId={tenantId} />,
    ...trafficPageRenderers({
      backendApiBaseUrl: runtime.config.backendApiBaseUrl,
      locale,
      permissionScope,
      surface: "app-console",
      tokenManager: runtime.tokenManager,
    }),
  };
  // Storage Center mounts the drive-owned admin storage pages. The surface
  // picks the page from `resource`, so the host owns the menu entry and the
  // route while the pages stay the shared implementation.
  const storageCenterSurface = (resource: StorageCenterResource) => (
    <StorageCenterSurface
      adminStorageApiBaseUrl={driveBaseUrl}
      locale={locale}
      operatorId={operatorId}
      resource={resource}
      tenantId={tenantId}
      tokenManager={runtime.tokenManager}
    />
  );
  const adminResourceRenderers = {
    apps: <DeployAppsAdminSurface deployBaseUrl={deployBaseUrl} driveBaseUrl={driveBaseUrl} locale={locale} tokenManager={runtime.tokenManager} />,
    // Domains and Certificates read the Web Server's own tenant-level planes, so
    // their pages are authored in the delivery capability package. They take no
    // client prop: both render inside `WebserverAdminSdkProvider` (mounted by
    // `WebserverAdminSurface`), which is where the admin SDK client comes from.
    domains: <ServedDomainAdminSurface locale={locale} resource="domains" />,
    certificates: <ServedCertificateAdminSurface locale={locale} resource="certificates" />,
    // Cloud accounts are one IAM-owned resource with one route set, so the admin
    // tab renders the same page the console does and only marks itself `admin`.
    // The ownership levels are not passed in: the shared adapter derives them from
    // the session, which is what lets the platform tenant see `platform` here
    // without the tenant console ever being able to.
    "cloud-accounts": <CloudAccountAdminSurface permissionScope={permissionScope} tenantId={tenantId} />,
    plugins: <PluginsAdminSurface attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as PluginsAdminSurfaceProps["attachSdkClientBoundaries"]} driveAppApiBaseUrl={driveBaseUrl} locale={locale} ownerKey={operatorId} resource="plugins" tokenManager={runtime.tokenManager} />,
    "plugin-categories": <PluginsAdminSurface attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as PluginsAdminSurfaceProps["attachSdkClientBoundaries"]} driveAppApiBaseUrl={driveBaseUrl} locale={locale} ownerKey={operatorId} resource="plugin-categories" tokenManager={runtime.tokenManager} />,
    skills: <SkillsAdminSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as SkillsAdminSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} resource="skills" tokenManager={runtime.tokenManager} permissionScope={permissionScope} />,
    mcp: <McpAdminSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as McpAdminSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} resource="mcp" tokenManager={runtime.tokenManager} />,
    "storage-providers": storageCenterSurface("storage-providers"),
    "storage-kinds": storageCenterSurface("storage-kinds"),
    "storage-buckets": storageCenterSurface("storage-buckets"),
    "storage-bindings": storageCenterSurface("storage-bindings"),
    // The cluster pages read the admin SDK from the shell's provider (the
    // overview polls it directly), so no per-page client is constructed here.
    // `cluster-clusters` / `cluster-hosts` / `cluster-instances` /
    // `cluster-events` are registry-driven and need no renderer entry.
    "cluster-overview": <ClusterOverviewSurface locale={locale} resource="cluster-overview" />,
    ...trafficPageRenderers({
      backendApiBaseUrl: runtime.config.backendApiBaseUrl,
      locale,
      permissionScope,
      surface: "backend-admin",
      tokenManager: runtime.tokenManager,
    }),
  };

  return (
    <WebserverConsoleSdkProvider clients={consoleClients}>
      <Routes>
        <Route
          path="/console/*"
          element={(
            <WebserverConsoleShell
              locale={locale}
              modules={consoleModules}
              notificationsHref={runtime.config.messagingPcUrl}
              onSignOut={signOut}
              permissionScope={permissionScope}
              portalHref="/"
              resourceRenderers={resourceRenderers}
              userLabel={userLabel}
            />
          )}
        />
        <Route
          path="/admin/*"
          element={adminAccess ? (
            <Suspense fallback={<div className="bootstrap-state" role="status">{messages["shell.status.loadingWorkspace"]}</div>}>
              <LazyAdminSurface
                backendApiBaseUrl={runtime.config.backendApiBaseUrl}
                locale={locale}
                modules={adminModules}
                onSignOut={signOut}
                permissionScope={permissionScope}
                resourceRenderers={adminResourceRenderers}
                tokenManager={runtime.tokenManager}
                userLabel={userLabel}
              />
            </Suspense>
          ) : <Navigate to="/console" replace />}
        />
        <Route path="*" element={<Navigate to={landingPath} replace />} />
      </Routes>
    </WebserverConsoleSdkProvider>
  );
}
