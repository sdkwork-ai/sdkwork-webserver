import {
  createDriveNodesImagePreviewReader,
  createDriveUploadImageService,
  type DriveUploadImageService,
} from "@sdkwork/drive-upload-image-core";
import {
  WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD,
  type SdkworkDriveAppClient,
} from "@sdkwork/webserver-pc-console-core";

/**
 * Host-side avatar capability for the IAM admin user directory.
 *
 * A thin facade over the shared `@sdkwork/drive-upload-image-core` factory:
 * the service binds this application's declared avatar intent constant
 * (`DRIVE_SPEC.md` §18 — the service layer, not the UI, supplies declared
 * values) to the composed `drive.uploader.uploadAvatar()` surface, and previews
 * of drive-backed avatars go through the shared bounded same-origin preview
 * reader over `drive.nodes.content.retrieve` (§8). The shared
 * `DriveUploadImage` component owns picking, parking, and uploading; the
 * workspace flushes a create-mode pick against the fresh user id through the
 * component's ref handle.
 */

/**
 * Builds the shared `DriveUploadImageService` for this application's declared
 * user-avatar upload intent. Handed to the IAM user workspace as
 * `driveUploadImageService` so the shared `DriveUploadImage` component can
 * render the avatar field end to end.
 */
export function createSdkworkIamUserDriveUploadImageService(
  drive: SdkworkDriveAppClient,
): DriveUploadImageService {
  return createDriveUploadImageService({
    uploader: drive.uploader,
    declaration: WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD,
    previewReader: createDriveNodesImagePreviewReader(drive.drive.nodes),
  });
}
