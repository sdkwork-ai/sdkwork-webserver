/**
 * Monaco editor bootstrap for the Server Config surface.
 *
 * Monaco is bundled from the local npm package (self-hosted deployments have
 * no CDN access) and wired to Vite's `?worker` chunks so JSON validation and
 * syntax tokenization run off the main thread. This module is imported
 * dynamically by the surface, so the editor payload stays out of the
 * initial admin bundle.
 */
import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import CssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import TsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

const globalScope = self as unknown as {
  MonacoEnvironment?: {
    getWorker?(workerId: string, label: string): Worker;
  };
};

globalScope.MonacoEnvironment = {
  getWorker(_workerId: string, label: string): Worker {
    switch (label) {
      case "json":
        return new JsonWorker();
      case "css":
      case "scss":
      case "less":
        return new CssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new HtmlWorker();
      case "typescript":
      case "javascript":
        return new TsWorker();
      default:
        return new EditorWorker();
    }
  },
};

loader.config({ monaco });

export { monaco };
