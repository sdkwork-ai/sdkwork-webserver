import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.tsx";
import { ensureCryptoRandomUuid } from "./bootstrap/ensure-crypto-random-uuid.ts";
import { resolveBootstrapShellLocale } from "./bootstrap/locale.ts";
import { bootstrapWebserverPcRuntime } from "./bootstrap/runtime.ts";
import { webserverApplicationCatalog } from "./i18n/index.ts";
import "./deploy-surface.css";
import "./index.css";

ensureCryptoRandomUuid();

// These two states render before the i18n provider exists, so they resolve the
// shell catalog directly. The catalog applies its own fallback for an unknown
// locale, so no deployment default is duplicated here.
const shellMessages = webserverApplicationCatalog.resolveMessages(resolveBootstrapShellLocale());

const element = document.getElementById("root"); if (!element) throw new Error("Application root element is missing"); const root = createRoot(element); root.render(<div className="bootstrap-state" role="status">{shellMessages["shell.status.loadingRuntime"]}</div>); void bootstrapWebserverPcRuntime().then((runtime) => root.render(<StrictMode><App runtime={runtime} /></StrictMode>)).catch((cause) => { console.error(cause); root.render(<div className="fatal-state" role="alert">{shellMessages["shell.error.runtimeUnavailable"]}</div>); });
