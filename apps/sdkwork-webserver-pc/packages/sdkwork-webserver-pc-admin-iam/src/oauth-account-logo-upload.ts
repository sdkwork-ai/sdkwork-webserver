import {
  createDriveNodesImagePreviewReader,
  createDriveUploadImageService,
  type DriveUploadImageService,
} from "@sdkwork/drive-upload-image-core";
import {
  WEBSERVER_PC_ADMIN_OAUTH_ACCOUNT_LOGO_UPLOAD,
  type SdkworkDriveAppClient,
} from "@sdkwork/webserver-pc-console-core";

/**
 * Host-side OAuth account-logo capability for the IAM admin console.
 *
 * Mirrors `organization-logo-upload.ts`: the service binds this application's
 * declared OAuth account-logo intent constant (`DRIVE_SPEC.md` §18) to the
 * composed `drive.uploader` surface, and `drive://` logo references resolve
 * their display through the shared bounded same-origin preview reader over
 * `drive.nodes.content.retrieve` (§8). The create-flow attach (upload against
 * the fresh account id, then a config update) is orchestrated by the account
 * setup section through this same service.
 */
export function createSdkworkIamOauthAccountDriveUploadImageService(
  drive: SdkworkDriveAppClient,
): DriveUploadImageService {
  return createDriveUploadImageService({
    uploader: drive.uploader,
    declaration: WEBSERVER_PC_ADMIN_OAUTH_ACCOUNT_LOGO_UPLOAD,
    previewReader: createDriveNodesImagePreviewReader(drive.drive.nodes),
  });
}
