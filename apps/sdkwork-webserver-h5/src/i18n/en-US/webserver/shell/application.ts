/**
 * Application-shell copy owned by the Web Server H5 browser host.
 *
 * Scope: bootstrap status and runtime-configuration failure. Shell chrome copy
 * lives in `@sdkwork/webserver-h5-commons`; business copy belongs to the owning
 * feature package fragment, never here.
 */
export const webserverShellApplicationEnUs = {
  "shell.bootstrap.loading": "Loading SDKWork Web Server…",
  "shell.bootstrap.failed": "Runtime configuration could not be loaded.",
} as const;
