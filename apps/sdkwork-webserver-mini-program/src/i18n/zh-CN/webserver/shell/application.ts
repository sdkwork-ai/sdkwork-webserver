/**
 * Shell-level bootstrap copy owned by the mini program root.
 *
 * Scope: only the strings the root itself renders before any capability package
 * exists — launch and failure. Capability copy belongs to the owning capability
 * package fragment, never here.
 */
export const webserverShellApplicationZhCn = {
  "shell.bootstrap.loading": "正在启动 SDKWork Web Server…",
  "shell.bootstrap.failed": "启动失败，请稍后重试。",
} as const;

export type WebserverShellApplicationMessages = typeof webserverShellApplicationZhCn;
