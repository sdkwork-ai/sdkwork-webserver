/**
 * Host-adapter registry for the mini program core. The WeChat platform adapter
 * lives in `@sdkwork/webserver-mp-host`; this registry only declares which host
 * adapter a bootstrapped runtime composed, so capability packages never reach
 * for a platform global themselves (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8).
 */
export function createSdkworkWebserverMpHostRegistry() {
  return {} as const;
}
