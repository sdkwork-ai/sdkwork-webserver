import { useSdkworkAuthControllerState } from "@sdkwork/auth-pc-react";
import { DeployAppsAdminSurface, webserverModule as appsAdminModule } from "@sdkwork/webserver-pc-admin-apps";
import { webserverModule as auditModule } from "@sdkwork/webserver-pc-admin-audit";
import { ClusterOverviewSurface, webserverModule as clusterModule } from "@sdkwork/webserver-pc-admin-cluster";
import { webserverModule as diagnosticsModule } from "@sdkwork/webserver-pc-admin-diagnostics";
import { webserverModule as mcpAdminModule, McpAdminSurface, type McpAdminSurfaceProps } from "@sdkwork/webserver-pc-admin-mcp";
import { webserverModule as nginxModule } from "@sdkwork/webserver-pc-admin-nginx";
import { webserverModule as pluginsAdminModule, PluginsAdminSurface, type PluginsAdminSurfaceProps } from "@sdkwork/webserver-pc-admin-plugins";
import { webserverModule as serversModule } from "@sdkwork/webserver-pc-admin-servers";
import { webserverModule as serversExplorerModule, ServerFilesExplorerSurface } from "@sdkwork/webserver-pc-admin-servers-explorer";
import { webserverModule as webserverConfigModule, WebserverConfigSurface } from "@sdkwork/webserver-pc-admin-webserver-config";
import { webserverModule as skillsAdminModule, SkillsAdminSurface, type SkillsAdminSurfaceProps } from "@sdkwork/webserver-pc-admin-skills";
import { StorageCenterSurface, webserverModule as storageModule, type StorageCenterResource } from "@sdkwork/webserver-pc-admin-storage";
import { hasWebserverAdminAccess, type WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";
import type { WebserverLocale } from "@sdkwork/webserver-pc-core";
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

// Applications / Domains / Certificates all render the canonical
// sdkwork-deployments pages over `deploy_app`, `deploy_domain_zone`, and the
// certificate entities. `delivery` therefore owns those three menu entries and
// nothing else: the Web Server console keeps no local application lifecycle.
const consoleModules = [deliveryModule, pluginsModule, skillsModule, mcpModule] satisfies readonly WebserverPcModuleDefinition[];
const adminModules = [appsAdminModule, nginxModule, serversModule, serversExplorerModule, webserverConfigModule, clusterModule, diagnosticsModule, auditModule, pluginsAdminModule, skillsAdminModule, mcpAdminModule, storageModule] satisfies readonly WebserverPcModuleDefinition[];
const LazyAdminSurface = lazy(() => import("./WebserverAdminSurface.tsx").then((module) => ({ default: module.WebserverAdminSurface })));

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
    plugins: <PluginsConsoleSurface attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as PluginsConsoleSurfaceProps["attachSdkClientBoundaries"]} driveAppApiBaseUrl={driveBaseUrl} locale={locale} resource="plugins" tokenManager={runtime.tokenManager} />,
    skills: <SkillsConsoleSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as SkillsConsoleSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} locale={locale} resource="skills" tokenManager={runtime.tokenManager} />,
    mcp: <McpConsoleSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as McpConsoleSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} locale={locale} resource="mcp" tokenManager={runtime.tokenManager} />,
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
    plugins: <PluginsAdminSurface attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as PluginsAdminSurfaceProps["attachSdkClientBoundaries"]} driveAppApiBaseUrl={driveBaseUrl} locale={locale} resource="plugins" tokenManager={runtime.tokenManager} />,
    skills: <SkillsAdminSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as SkillsAdminSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} resource="skills" tokenManager={runtime.tokenManager} permissionScope={permissionScope} />,
    mcp: <McpAdminSurface appApiBaseUrl={runtime.config.appApiBaseUrl} attachSdkClientBoundaries={runtime.attachSdkClientBoundaries as McpAdminSurfaceProps["attachSdkClientBoundaries"]} backendApiBaseUrl={runtime.config.backendApiBaseUrl} driveAppApiBaseUrl={driveBaseUrl} resource="mcp" tokenManager={runtime.tokenManager} />,
    "servers-explorer": <ServerFilesExplorerSurface backendApiBaseUrl={runtime.config.backendApiBaseUrl} permissionScope={permissionScope} resource="servers-explorer" tokenManager={runtime.tokenManager} />,
    "webserver-config": <WebserverConfigSurface backendApiBaseUrl={runtime.config.backendApiBaseUrl} permissionScope={permissionScope} resource="webserver-config" tokenManager={runtime.tokenManager} />,
    "storage-providers": storageCenterSurface("storage-providers"),
    "storage-kinds": storageCenterSurface("storage-kinds"),
    "storage-buckets": storageCenterSurface("storage-buckets"),
    "storage-bindings": storageCenterSurface("storage-bindings"),
    "cluster-overview": <ClusterOverviewSurface locale={locale} resource="cluster-overview" />,
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
