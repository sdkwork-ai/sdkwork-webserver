/**
 * Cross-architecture composition entry.
 *
 * Authority: `APP_COMPOSITION_SPEC.md`. Feature packages resolve runtime
 * composition metadata through this core package's public exports only.
 */
export * from '../main/ets/composition/DependencyManifest';
export * from '../main/ets/composition/HostRegistry';
export * from '../main/ets/composition/ModuleRegistry';
export * from '../main/ets/composition/SdkInventory';
