import { useSdkworkAuthControllerState } from "@sdkwork/auth-pc-react";
import { SdkworkThemeProvider } from "@sdkwork/ui-pc-react/theme";
import { portalAgentCatalog } from "@sdkwork/webserver-pc-portal";
import type { SdkworkThemeSelection } from "@sdkwork/ui-pc-react/theme";
import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import type { BootstrappedWebserverPcRuntime } from "./bootstrap/runtime.ts";
import { browserPortalClipboard, createBrowserPortalStatistics } from "./bootstrap/portalHost.ts";
import {
  commitWebserverTheme,
  resolveInitialWebserverTheme,
  WEBSERVER_THEME_COLOR,
  WEBSERVER_THEME_OVERRIDES,
} from "./bootstrap/theme.ts";

const LazyAuthenticatedSurface = lazy(() => import("./surfaces/WebserverAuthenticatedSurface.tsx").then((module) => ({ default: module.WebserverAuthenticatedSurface })));
const LazyWebserverDocumentation = lazy(() => import("@sdkwork/webserver-pc-documentation").then((module) => ({ default: module.WebserverDocumentation })));
const LazyWebserverPortal = lazy(() => import("@sdkwork/webserver-pc-portal").then((module) => ({ default: module.WebserverPortal })));
const supportedAgents = portalAgentCatalog.map(({ label }) => label);

export function App({ runtime }: { runtime: BootstrappedWebserverPcRuntime }) {
  const [themeSelection, setThemeSelection] = useState(resolveInitialWebserverTheme);

  const handleThemeSelectionChange = (nextTheme: SdkworkThemeSelection) => {
    setThemeSelection(commitWebserverTheme(nextTheme));
  };

  return (
    <SdkworkThemeProvider
      className="webserver-pc-theme"
      locale={runtime.locale}
      onThemeSelectionChange={handleThemeSelectionChange}
      overrides={WEBSERVER_THEME_OVERRIDES}
      themeColor={WEBSERVER_THEME_COLOR}
      themeSelection={themeSelection}
    >
      <BrowserRouter>
        <Routes>
          <Route
            path="/"
            element={<PublicPortalApplication runtime={runtime} />}
          />
          <Route
            path="/docs/*"
            element={<PublicDocumentationApplication runtime={runtime} />}
          />
          <Route
            path="/*"
            element={(
              <Suspense fallback={<div className="bootstrap-state">SDKWork Web Server</div>}>
                <LazyAuthenticatedSurface runtime={runtime} />
              </Suspense>
            )}
          />
        </Routes>
      </BrowserRouter>
    </SdkworkThemeProvider>
  );
}

function PublicPortalApplication({ runtime }: { runtime: BootstrappedWebserverPcRuntime }) {
  const authState = usePublicAuthState(runtime);
  const statistics = useMemo(
    () => createBrowserPortalStatistics(async () => (await runtime.loadConsoleClients()).web),
    [runtime.loadConsoleClients],
  );

  const viewer = authState.isAuthenticated
    ? { label: authState.user?.displayName || authState.user?.email }
    : undefined;

  return (
    <Suspense fallback={<div className="bootstrap-state">SDKWork Web Server</div>}>
      <LazyWebserverPortal
        clipboard={browserPortalClipboard}
        locale={runtime.locale}
        navigation={{
          consoleHref: "/console",
          createApplicationHref: "/console/applications",
          deploymentsHref: "/console/deployments",
          documentationHref: "/docs",
          notificationsHref: runtime.config.messagingPcUrl,
        }}
        statistics={authState.isAuthenticated ? statistics : undefined}
        viewer={viewer}
      />
    </Suspense>
  );
}

function PublicDocumentationApplication({ runtime }: { runtime: BootstrappedWebserverPcRuntime }) {
  const authState = usePublicAuthState(runtime);
  const viewer = authState.isAuthenticated
    ? { label: authState.user?.displayName || authState.user?.email }
    : undefined;

  return (
    <Suspense fallback={<div className="bootstrap-state">SDKWork Web Server</div>}>
      <LazyWebserverDocumentation
        locale={runtime.locale}
        navigation={{
          consoleHref: "/console",
          notificationsHref: runtime.config.messagingPcUrl,
          portalHref: "/",
        }}
        supportedAgents={supportedAgents}
        viewer={viewer}
      />
    </Suspense>
  );
}

function usePublicAuthState(runtime: BootstrappedWebserverPcRuntime) {
  const authState = useSdkworkAuthControllerState(runtime.authController);

  useEffect(() => {
    if (authState.isBootstrapped) return;
    void runtime.authController.bootstrap().catch(() => undefined);
  }, [authState.isBootstrapped, runtime.authController]);

  return authState;
}
