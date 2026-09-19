/**
 * Application upload declaration constants.
 *
 * Authority: `DRIVE_SPEC.md` section 18 (Application Upload Declaration Contract).
 * Declared values live in `apps/sdkwork-webserver-pc/specs/upload.declaration.json`; this module
 * carries them into code so upload call sites reference a constant instead of repeating literals.
 *
 * The `scene`, `source`, and `appResourceType` values were previously bare literals at the call
 * site. Section 18.3 forbids that: a duplicate silently diverges from the declaration that the
 * gate validates.
 */

export interface WebserverPcUploadDeclarationEntry {
  readonly appResourceIdKind: 'application' | 'entity' | 'draft';
  readonly appResourceType: string;
  readonly purpose: string;
  readonly retention: 'long_term' | 'temporary';
  readonly scene: string;
  readonly source: string;
  readonly uploadProfileCode: string;
}

/** This application's canonical appId, from `sdkwork.app.config.json` `backend.appId`. */
export const WEBSERVER_PC_APP_ID = 'sdkwork-webserver-pc' as const;

/** The single call-origin label for every upload from this application. */
export const WEBSERVER_PC_UPLOAD_SOURCE = 'sdkwork-webserver-pc' as const;

/** A published plugin package archive uploaded from the web console. */
export const WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD = {
  appResourceIdKind: 'entity',
  appResourceType: 'web.plugin.package',
  purpose: 'Plugin package archive uploaded from the web console.',
  retention: 'long_term',
  scene: 'plugin-package',
  source: WEBSERVER_PC_UPLOAD_SOURCE,
  uploadProfileCode: 'archive',
} as const satisfies WebserverPcUploadDeclarationEntry;

/** Every declared upload purpose for this application. */
export const WEBSERVER_PC_UPLOAD_DECLARATIONS: readonly WebserverPcUploadDeclarationEntry[] = [
  WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD,
];
