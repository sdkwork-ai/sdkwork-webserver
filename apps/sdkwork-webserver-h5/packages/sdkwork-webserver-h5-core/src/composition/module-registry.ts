/**
 * Module registry for the H5 core. Feature packages register their route and
 * navigation contributions through the shell package; the core only owns the
 * (currently empty) module-slot registry that `APP_COMPOSITION_SPEC.md` §3
 * requires every core package to expose.
 */
export function createSdkworkWebserverH5ModuleRegistry() {
  return {} as const;
}
