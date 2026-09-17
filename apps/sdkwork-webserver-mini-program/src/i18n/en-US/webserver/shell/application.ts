/**
 * Shell-level bootstrap copy owned by the mini program root.
 *
 * Scope: only the strings the root itself renders before any capability package
 * exists — launch and failure. Capability copy belongs to the owning capability
 * package fragment, never here.
 */
export const webserverShellApplicationEnUs = {
  "shell.bootstrap.loading": "Starting SDKWork Web Server…",
  "shell.bootstrap.failed": "Launch failed. Try again shortly.",
} as const;

export type WebserverShellApplicationMessages = typeof webserverShellApplicationEnUs;
