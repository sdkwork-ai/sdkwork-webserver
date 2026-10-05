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
  appResourceIdKind: 'application',
  appResourceType: 'web.plugin.package',
  purpose: 'Web console plugin package uploaded so the plugin can be installed into the running webserver.',
  retention: 'long_term',
  scene: 'plugin-package',
  source: WEBSERVER_PC_UPLOAD_SOURCE,
  uploadProfileCode: 'archive',
} as const satisfies WebserverPcUploadDeclarationEntry;

/**
 * A user directory avatar uploaded from the IAM admin console.
 *
 * `appResourceIdKind` is `entity`: the avatar belongs to the user directory
 * record, so `appResourceId` at call time is that user's id. The create flow
 * therefore persists the user first, uploads against the new id, and attaches
 * the returned media resource with a follow-up update (`DRIVE_SPEC.md`
 * section 18.3: persist first, upload second).
 */
export const WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD = {
  appResourceIdKind: 'entity',
  appResourceType: 'profile.avatar',
  purpose: 'IAM admin console user avatar uploaded to Drive and attached to the user directory record.',
  retention: 'long_term',
  scene: 'avatar',
  source: WEBSERVER_PC_UPLOAD_SOURCE,
  uploadProfileCode: 'avatar',
} as const satisfies WebserverPcUploadDeclarationEntry;

/**
 * An organization logo uploaded from the IAM admin console.
 *
 * Same shape and flow rules as the user avatar: `entity`-kind id (the
 * organization directory record), persisted first and attached second on
 * create (`DRIVE_SPEC.md` section 18.3), and bound to the same `avatar`
 * upload profile the IAM snapshot columns were sized around.
 */
export const WEBSERVER_PC_ADMIN_ORGANIZATION_LOGO_UPLOAD = {
  appResourceIdKind: 'entity',
  appResourceType: 'organization.logo',
  purpose: 'IAM admin console organization logo uploaded to Drive and attached to the organization directory record.',
  retention: 'long_term',
  scene: 'organization-logo',
  source: WEBSERVER_PC_UPLOAD_SOURCE,
  uploadProfileCode: 'avatar',
} as const satisfies WebserverPcUploadDeclarationEntry;

/**
 * An OAuth resource-account logo (mini program / official account) uploaded
 * from the IAM admin console.
 *
 * Same shape and flow rules as the organization logo: `entity`-kind id (the
 * resource account), persisted first and attached second on create
 * (`DRIVE_SPEC.md` section 18.3), bound to the `avatar` upload profile.
 */
export const WEBSERVER_PC_ADMIN_OAUTH_ACCOUNT_LOGO_UPLOAD = {
  appResourceIdKind: 'entity',
  appResourceType: 'oauth.account_logo',
  purpose: 'IAM admin console OAuth account logo uploaded to Drive and referenced by the account configuration.',
  retention: 'long_term',
  scene: 'oauth-account-logo',
  source: WEBSERVER_PC_UPLOAD_SOURCE,
  uploadProfileCode: 'avatar',
} as const satisfies WebserverPcUploadDeclarationEntry;

/**
 * Stable application-scope resource id sent as `appResourceId` for plugin archives.
 *
 * The create flow uploads the archive before any plugin entity exists, so the id must not be
 * derived from the uploaded file's name (`DRIVE_SPEC.md` section 18.3): a runtime file name is
 * neither an entity identifier nor a stable statistic dimension. The original file name already
 * travels as `originalFileName`.
 */
export const WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD_RESOURCE_ID = 'plugin-package-archive' as const;

/** Every declared upload purpose for this application. */
export const WEBSERVER_PC_UPLOAD_DECLARATIONS: readonly WebserverPcUploadDeclarationEntry[] = [
  WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD,
  WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD,
  WEBSERVER_PC_ADMIN_ORGANIZATION_LOGO_UPLOAD,
  WEBSERVER_PC_ADMIN_OAUTH_ACCOUNT_LOGO_UPLOAD,
];
