/**
 * Configuration file presentation helpers for the Server Config surface.
 *
 * The backend catalog carries a Monaco language id per entry (see
 * `sdkwork-webserver-config-service::language`); this module mirrors that
 * mapping for client-side pre-save validation and adds display metadata so
 * the surface stays declarative.
 */

export type WebserverConfigKindId = "default-config" | "import-config" | "module-config";

/** Monaco language id for a configuration file name (mirror of the service mapping). */
export function monacoLanguageFor(fileName: string): string {
  const extension = fileName.toLowerCase().split(".").pop() ?? "";
  switch (extension) {
    case "json":
      return "json";
    case "yaml":
    case "yml":
      return "yaml";
    case "toml":
    case "ini":
    case "conf":
    case "cfg":
      return "ini";
    case "sh":
    case "env":
      return "shell";
    case "md":
      return "markdown";
    default:
      return "plaintext";
  }
}

/** Human label per configuration group. */
export const CONFIG_KIND_LABEL: Record<WebserverConfigKindId, string> = {
  "default-config": "Default Config",
  "import-config": "Import Config",
  "module-config": "Module Config",
};

/** Short description per configuration group, shown in the catalog sidebar. */
export const CONFIG_KIND_DESCRIPTION: Record<WebserverConfigKindId, string> = {
  "default-config": "Top-level Web Server runtime configuration.",
  "import-config": "imports.d module import plane (active import set).",
  "module-config": "Sibling-module sidecar configs (read-only, module-owned).",
};

/** Group order used by the catalog sidebar. */
export const CONFIG_KIND_ORDER: readonly WebserverConfigKindId[] = [
  "default-config",
  "import-config",
  "module-config",
];

/** Client-side syntax gate before a save request is issued. */
export function validateConfigContent(
  fileName: string,
  content: string,
): string | null {
  if (content.includes("\0")) {
    return "Content must not contain NUL bytes.";
  }
  const extension = fileName.toLowerCase().split(".").pop() ?? "";
  if (extension === "json") {
    try {
      JSON.parse(content);
    } catch (reason) {
      return `Invalid JSON: ${reason instanceof Error ? reason.message : String(reason)}`;
    }
  }
  return null;
}
