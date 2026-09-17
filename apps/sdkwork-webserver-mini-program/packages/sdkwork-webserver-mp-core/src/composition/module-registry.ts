/**
 * Module registry for the mini program core. Capability packages register their
 * route contributions through the shell package; the core only owns the
 * (currently empty) module-slot registry that `APP_COMPOSITION_SPEC.md` §3
 * requires every core package to expose.
 */
export function createSdkworkWebserverMpModuleRegistry() {
  return {} as const;
}
