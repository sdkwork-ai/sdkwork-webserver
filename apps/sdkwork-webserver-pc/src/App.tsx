import { useSdkworkAuthControllerState } from "@sdkwork/auth-pc-react";
import { SdkworkI18nProvider, useSdkworkI18n, useSdkworkModuleMessages } from "@sdkwork/i18n-pc-react";
import { SdkworkThemeProvider } from "@sdkwork/ui-pc-react/theme";
import { portalAgentCatalog } from "@sdkwork/webserver-pc-portal";
import type { SdkworkThemeSelection } from "@sdkwork/ui-pc-react/theme";
import type { WebserverLocale } from "@sdkwork/webserver-pc-core";
import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import type { BootstrappedWebserverPcRuntime } from "./bootstrap/runtime.ts";
import {
  commitBrowserLocalePreference,
  narrowWebserverLocale,
  resolveBrowserInitialLocale,
} from "./bootstrap/locale.ts";
import { browserPortalClipboard, createBrowserPortalStatistics } from "./bootstrap/portalHost.ts";
import {
  commitWebserverTheme,
  resolveInitialWebserverTheme,
  WEBSERVER_THEME_COLOR,
  WEBSERVER_THEME_OVERRIDES,
} from "./bootstrap/theme.ts";
import { createWebserverI18nRuntimeConfig, webserverApplicationCatalog } from "./i18n/index.ts";

const LazyAuthenticatedSurface = lazy(() => import("./surfaces/WebserverAuthenticatedSurface.tsx").then((module) => ({ default: module.WebserverAuthenticatedSurface })));
const LazyWebserverDocumentation = lazy(() => import("@sdkwork/webserver-pc-documentation").then((module) => ({ default: module.WebserverDocumentation })));
const LazyWebserverPortal = lazy(() => import("@sdkwork/webserver-pc-portal").then((module) => ({ default: module.WebserverPortal })));
const supportedAgents = portalAgentCatalog.map(({ label }) => label);

export function App({ runtime }: { runtime: BootstrappedWebserverPcRuntime }) {
  const i18nRuntimeConfig = useMemo(
    () => createWebserverI18nRuntimeConfig(runtime.config),
    [runtime.config],
  );
  // The provider owns the live locale; the shell only seeds it once from the
  // stored user preference and the browser's preferred languages.
  const [initialLocale] = useState(() => resolveBrowserInitialLocale(runtime.config));

  return (
    <SdkworkI18nProvider
      catalogs={[webserverApplicationCatalog]}
      config={i18nRuntimeConfig}
      locale={initialLocale}
      syncDocumentLanguage
    >
      <WebserverApplication runtime={runtime} />
    </SdkworkI18nProvider>
  );
}

function WebserverApplication({ runtime }: { runtime: BootstrappedWebserverPcRuntime }) {
  const i18n = useSdkworkI18n();
  const messages = useSdkworkModuleMessages(webserverApplicationCatalog);
  const [themeSelection, setThemeSelection] = useState(resolveInitialWebserverTheme);
  const locale = narrowWebserverLocale(i18n?.localeTag, runtime.config);

  const handleLocaleChange = (next: WebserverLocale) => {
    // Keep SDK negotiation, the persisted preference, and the rendered locale
    // in step: the runtime slot feeds Accept-Language on the next request.
    runtime.setLocale(next);
    commitBrowserLocalePreference(next);
    void i18n?.changeLocale(next);
  };

  const handleThemeSelectionChange = (nextTheme: SdkworkThemeSelection) => {
    setThemeSelection(commitWebserverTheme(nextTheme));
  };

  return (
    <SdkworkThemeProvider
      className="webserver-pc-theme"
      locale={locale}
      onThemeSelectionChange={handleThemeSelectionChange}
      overrides={WEBSERVER_THEME_OVERRIDES}
      themeColor={WEBSERVER_THEME_COLOR}
      themeSelection={themeSelection}
    >
      <BrowserRouter>
        <Routes>
          <Route
            path="/"
            element={(
              <PublicPortalApplication
                availableLocales={runtime.config.activeLocales}
                locale={locale}
                onLocaleChange={handleLocaleChange}
                runtime={runtime}
              />
            )}
          />
          <Route
            path="/docs/*"
            element={<PublicDocumentationApplication locale={locale} runtime={runtime} />}
          />
          <Route
            path="/*"
            element={(
              <Suspense fallback={<SurfaceLoadingState message={messages["shell.status.loadingWorkspace"]} />}>
                <LazyAuthenticatedSurface locale={locale} runtime={runtime} />
              </Suspense>
            )}
          />
        </Routes>
      </BrowserRouter>
    </SdkworkThemeProvider>
  );
}

/**
 * Loading placeholder for a lazy route surface. It reads shell copy through the
 * injected provider so a language switch is reflected even while a surface is
 * still resolving.
 */
function SurfaceLoadingState({ message }: { message: string }) {
  return <div className="bootstrap-state" role="status">{message}</div>;
}

function PublicPortalApplication({
  availableLocales,
  locale,
  onLocaleChange,
  runtime,
}: {
  availableLocales: readonly WebserverLocale[];
  locale: WebserverLocale;
  onLocaleChange: (locale: WebserverLocale) => void;
  runtime: BootstrappedWebserverPcRuntime;
}) {
  const authState = usePublicAuthState(runtime);
  const messages = useSdkworkModuleMessages(webserverApplicationCatalog);
  const statistics = useMemo(
    () => createBrowserPortalStatistics(async () => (await runtime.loadConsoleClients()).deploy),
    [runtime.loadConsoleClients],
  );

  const viewer = authState.isAuthenticated
    ? { label: authState.user?.displayName || authState.user?.email }
    : undefined;

  return (
    <Suspense fallback={<SurfaceLoadingState message={messages["shell.status.loadingWorkspace"]} />}>
      <LazyWebserverPortal
        availableLocales={availableLocales}
        clipboard={browserPortalClipboard}
        locale={locale}
        navigation={{
          consoleHref: "/console",
          createApplicationHref: "/console/applications",
          deploymentsHref: "/console/deployments",
          documentationHref: "/docs",
          notificationsHref: runtime.config.messagingPcUrl,
        }}
        onLocaleChange={onLocaleChange}
        statistics={authState.isAuthenticated ? statistics : undefined}
        viewer={viewer}
      />
    </Suspense>
  );
}

function PublicDocumentationApplication({
  locale,
  runtime,
}: {
  locale: WebserverLocale;
  runtime: BootstrappedWebserverPcRuntime;
}) {
  const authState = usePublicAuthState(runtime);
  const messages = useSdkworkModuleMessages(webserverApplicationCatalog);
  const viewer = authState.isAuthenticated
    ? { label: authState.user?.displayName || authState.user?.email }
    : undefined;

  return (
    <Suspense fallback={<SurfaceLoadingState message={messages["shell.status.loadingWorkspace"]} />}>
      <LazyWebserverDocumentation
        locale={locale}
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
