import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App, WebserverH5BootstrapFailure, WebserverH5BootstrapStatus } from "./App.tsx";
import { bootstrapWebserverH5 } from "./bootstrap/runtime.ts";
import { createWebserverH5MessageResolver } from "./i18n/index.ts";
import "./index.css";

const element = document.getElementById("root");
if (!element) {
  throw new Error("Application root element is missing");
}

const root = createRoot(element);
/**
 * Bootstrap failures happen before the runtime configuration decides which
 * locales are supported, so the pre-boot status line and the failure block use
 * the browser's own language rather than a locale the deployment may not ship.
 */
const preBootstrapResolver = createWebserverH5MessageResolver(
  globalThis.navigator?.language ?? "en-US",
);

root.render(
  <StrictMode>
    <WebserverH5BootstrapStatus resolveMessage={preBootstrapResolver} />
  </StrictMode>,
);

void bootstrapWebserverH5()
  .then((bootstrap) => {
    globalThis.document?.documentElement?.setAttribute("lang", bootstrap.locale);
    root.render(
      <StrictMode>
        <App
          clients={bootstrap.clients}
          resolveMessage={createWebserverH5MessageResolver(bootstrap.locale)}
        />
      </StrictMode>,
    );
  })
  .catch((error: unknown) => {
    root.render(
      <StrictMode>
        <WebserverH5BootstrapFailure
          detail={error instanceof Error ? error.message : String(error)}
          resolveMessage={preBootstrapResolver}
        />
      </StrictMode>,
    );
  });
