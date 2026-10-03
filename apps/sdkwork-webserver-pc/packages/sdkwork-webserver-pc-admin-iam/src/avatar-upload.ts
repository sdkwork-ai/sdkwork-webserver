import {
  WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD,
  type SdkworkDriveAppClient,
} from "@sdkwork/webserver-pc-console-core";
import type { SdkworkIamAdminUserAvatarResource } from "@sdkwork/iam-pc-admin-user";

/**
 * Host-side avatar capability for the IAM admin user directory.
 *
 * Uploads go through the composed `drive.uploader.uploadAvatar()` surface with
 * this application's declared intent constants (`DRIVE_SPEC.md` §18) — no
 * ambient identity fields, which Drive derives from the authenticated runtime.
 * Previews of drive-backed avatars are read back through the generated
 * `drive.nodes.content.retrieve` bounded same-origin read (§8) and surfaced as
 * a transient data URL; plain external avatars resolve to their delivery URL.
 */

/** Preview read ceiling; avatars are small, larger content degrades to the placeholder. */
const AVATAR_PREVIEW_MAX_BYTES = 2 * 1024 * 1024;

const DRIVE_URI_NODE_PATTERN = /^drive:\/\/spaces\/([^/]+)\/nodes\/([^/?#]+)/;

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
  const uploaded = await drive.uploader.uploadAvatar({
    appResourceId: userId,
    appResourceType: WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD.appResourceType,
    file,
    scene: WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD.scene,
    source: WEBSERVER_PC_ADMIN_USER_AVATAR_UPLOAD.source,
  });
  const spaceId = uploaded.uploadItem.spaceId;
  const nodeId = uploaded.uploadItem.nodeId;
  if (!spaceId || !nodeId) {
    throw new Error("Drive did not return the uploaded avatar identity");
  }
  return {
    fileName: uploaded.uploadItem.originalFileName || file.name,
    id: nodeId,
    kind: "image",
    metadata: { drive: { nodeId, spaceId } },
    mimeType: uploaded.uploadItem.contentType || file.type || undefined,
    sizeBytes: uploaded.uploadItem.contentLength || String(file.size),
    source: "drive",
    uri: `drive://spaces/${spaceId}/nodes/${nodeId}`,
  };
}

/**
 * Transient display URL for a stored avatar resource. Drive-backed resources
 * read through the SDK (bounded, same-origin); other sources use their own
 * delivery URL. The result is presentation-only state and never persisted.
 */
export async function resolveUserAvatarUrl(
  drive: SdkworkDriveAppClient,
  avatar: SdkworkIamAdminUserAvatarResource,
): Promise<string | undefined> {
  const directUrl = avatar.publicUrl || avatar.url;
  if (avatar.source !== "drive") {
    return directUrl || avatar.uri;
  }
  const nodeId = avatar.metadata?.drive?.nodeId
    ?? DRIVE_URI_NODE_PATTERN.exec(avatar.uri ?? "")?.[2];
  if (!nodeId) {
    return directUrl;
  }
  const content = await drive.drive.nodes.content.retrieve(nodeId, {
    encoding: "base64",
    maxBytes: AVATAR_PREVIEW_MAX_BYTES,
  });
  if (content.hasMore) {
    return undefined;
  }
  return `data:${content.contentType || avatar.mimeType || "image/png"};base64,${content.content}`;
}
