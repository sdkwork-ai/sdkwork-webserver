import { useSdkworkModuleMessages } from "@sdkwork/i18n-pc-react";
import type { WebserverLocale } from "@sdkwork/webserver-pc-core";
import { lazy, Suspense } from "react";
import { WebserverAuthGate } from "../auth/WebserverAuthGate.tsx";
import type { BootstrappedWebserverPcRuntime } from "../bootstrap/runtime.ts";
import { webserverApplicationCatalog } from "../i18n/index.ts";

const LazyAuthRoutes = lazy(() => import("../auth/WebserverAuthRoutes.tsx").then((module) => ({ default: module.WebserverAuthRoutes })));
const LazyAuthorizedWorkspace = lazy(() => import("./WebserverAuthorizedWorkspace.tsx").then((module) => ({ default: module.WebserverAuthorizedWorkspace })));

/**
 * The surface receives the shell's live locale instead of reading the frozen
 * bootstrap value, so a language switch re-renders the authenticated surface
 * and its auth routes without reconstructing auth or session state.
 */
export function WebserverAuthenticatedSurface({
  locale,
  runtime,
}: {
  locale: WebserverLocale;
  runtime: BootstrappedWebserverPcRuntime;
}) {
  const messages = useSdkworkModuleMessages(webserverApplicationCatalog);
  const loadingState = <div className="bootstrap-state" role="status">{messages["shell.status.loadingWorkspace"]}</div>;

  return (
    <WebserverAuthGate
      authRoutes={(
        <Suspense fallback={loadingState}>
          <LazyAuthRoutes
            controller={runtime.authController}
            loadRuntimeConfig={runtime.loadAuthRuntimeConfig}
            locale={locale}
          />
        </Suspense>
      )}
      controller={runtime.authController}
      locale={locale}
    >
      <Suspense fallback={loadingState}>
        <LazyAuthorizedWorkspace locale={locale} runtime={runtime} />
      </Suspense>
    </WebserverAuthGate>
  );
}
