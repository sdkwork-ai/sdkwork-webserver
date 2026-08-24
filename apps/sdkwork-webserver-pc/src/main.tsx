import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.tsx";
import { ensureCryptoRandomUuid } from "./bootstrap/ensure-crypto-random-uuid.ts";
import { bootstrapWebserverPcRuntime } from "./bootstrap/runtime.ts";
import "./deploy-domains.css";
import "./index.css";

ensureCryptoRandomUuid();

const element = document.getElementById("root"); if (!element) throw new Error("Application root element is missing"); const root = createRoot(element); root.render(<div className="bootstrap-state" role="status">SDKWork Web Server</div>); void bootstrapWebserverPcRuntime().then((runtime) => root.render(<StrictMode><App runtime={runtime} /></StrictMode>)).catch((cause) => { console.error(cause); root.render(<div className="fatal-state" role="alert">Runtime configuration could not be loaded.</div>); });
