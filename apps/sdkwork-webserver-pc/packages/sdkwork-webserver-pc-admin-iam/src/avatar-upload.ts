import {
  createDriveNodesImagePreviewReader,
  createDriveUploadImageService,
  type DriveUploadImageService,
} from "@sdkwork/drive-upload-image-core";
import type { SdkworkIamAdminUserAvatarResource } from "@sdkwork/iam-pc-admin-user";
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
 * reader over `drive.nodes.content.retrieve` (§8). Plain external avatars keep
 * resolving to their own delivery URL.
 */

/**
 * Builds the shared `DriveUploadImageService` for this application's declared
 * user-avatar upload intent. Also handed to the IAM user workspace as
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

export interface SdkworkIamUserAvatarService {
  resolveAvatarUrl(avatar: SdkworkIamAdminUserAvatarResource): Promise<string | undefined>;
  uploadAvatar(userId: string, file: File): Promise<SdkworkIamAdminUserAvatarResource>;
}

export function createSdkworkIamUserAvatarService(drive: SdkworkDriveAppClient): SdkworkIamUserAvatarService {
  return {
    resolveAvatarUrl: (avatar) => resolveUserAvatarUrl(drive, avatar),
    uploadAvatar: (userId, file) => uploadUserAvatar(drive, userId, file),
  };
}

/**
 * Uploads the picked image for an existing user and returns the drive-backed
 * media resource stored as the user's avatar snapshot (`DRIVE_SPEC.md` §10
 * mapping: `source: "drive"`, `uri: drive://spaces/{spaceId}/nodes/{nodeId}`,
 * and the `metadata.drive` block).
 */
export async function uploadUserAvatar(
  drive: SdkworkDriveAppClient,
  userId: string,
  file: File,
): Promise<SdkworkIamAdminUserAvatarResource> {
  const uploaded = await createSdkworkIamUserDriveUploadImageService(drive).upload({
    appResourceId: userId,
    file,
  });
  const driveMetadata = uploaded.metadata?.drive;
  const spaceId = driveMetadata?.spaceId;
  const nodeId = driveMetadata?.nodeId;
  if (!spaceId || !nodeId) {
    throw new Error("Drive did not return the uploaded avatar identity");
  }
  return {
    fileName: driveMetadata.originalFileName || file.name,
    id: nodeId,
    kind: "image",
    metadata: { drive: { nodeId, spaceId } },
    mimeType: driveMetadata.contentType || file.type || undefined,
    sizeBytes: driveMetadata.contentLength || String(file.size),
    source: "drive",
    uri: uploaded.uri,
  };
}

/**
 * Transient display URL for a stored avatar resource. Drive-backed resources
 * resolve through the shared bounded preview reader; other sources keep their
 * own delivery URL. The result is presentation-only state and never persisted.
 */
export async function resolveUserAvatarUrl(
  drive: SdkworkDriveAppClient,
  avatar: SdkworkIamAdminUserAvatarResource,
): Promise<string | undefined> {
  const directUrl = avatar.publicUrl || avatar.url;
  if (avatar.source !== "drive") {
    return directUrl || avatar.uri || undefined;
  }
  if (!avatar.uri) {
    return directUrl || undefined;
  }
  const previewUrl = await createSdkworkIamUserDriveUploadImageService(drive).resolvePreview({
    uri: avatar.uri,
  });
  return previewUrl ?? undefined;
}
