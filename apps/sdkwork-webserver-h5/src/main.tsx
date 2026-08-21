import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.tsx";
import "./index.css";

const element = document.getElementById("root");
if (!element) {
  throw new Error("Application root element is missing");
}

createRoot(element).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
