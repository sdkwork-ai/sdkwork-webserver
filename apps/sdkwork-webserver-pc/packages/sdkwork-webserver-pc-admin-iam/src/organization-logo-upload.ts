import {
  createDriveNodesImagePreviewReader,
  createDriveUploadImageService,
  type DriveUploadImageService,
} from "@sdkwork/drive-upload-image-core";
import type {
  SdkworkIamOrganizationLogoMediaResource,
  SdkworkIamOrganizationLogoService,
} from "@sdkwork/iam-pc-admin-organization";
import {
  WEBSERVER_PC_ADMIN_ORGANIZATION_LOGO_UPLOAD,
  type SdkworkDriveAppClient,
} from "@sdkwork/webserver-pc-console-core";

/**
 * Host-side organization-logo capability for the IAM admin organization
 * directory.
 *
 * A thin facade over the shared `@sdkwork/drive-upload-image-core` factory,
 * mirroring `avatar-upload.ts`: the service binds this application's declared
 * organization-logo intent constant (`DRIVE_SPEC.md` §18 — the service layer,
 * not the UI, supplies declared values) to the composed `drive.uploader`
 * surface, and previews of drive-backed logos go through the shared bounded
 * same-origin preview reader over `drive.nodes.content.retrieve` (§8). Plain
 * external logos keep resolving to their own delivery URL.
 */

/**
 * Builds the shared `DriveUploadImageService` for this application's declared
 * organization-logo upload intent. Also handed to the organization workspace
 * as `driveUploadImageService` so the shared `DriveUploadImage` component can
 * render the logo field end to end in the edit drawer.
 */
export function createSdkworkIamOrganizationDriveUploadImageService(
  drive: SdkworkDriveAppClient,
): DriveUploadImageService {
  return createDriveUploadImageService({
    uploader: drive.uploader,
    declaration: WEBSERVER_PC_ADMIN_ORGANIZATION_LOGO_UPLOAD,
    previewReader: createDriveNodesImagePreviewReader(drive.drive.nodes),
  });
}

export function createSdkworkIamOrganizationLogoService(
  drive: SdkworkDriveAppClient,
): SdkworkIamOrganizationLogoService {
  return {
    attachLogo: (organizationId, file) => attachOrganizationLogo(drive, organizationId, file),
    resolveLogoUrl: (logo) => resolveOrganizationLogoUrl(drive, logo),
  };
}

/**
 * Uploads the picked image for an existing organization and returns the
 * drive-backed media resource stored as the organization's logo snapshot
 * (`DRIVE_SPEC.md` §10 mapping: `source: "drive"`,
 * `uri: drive://spaces/{spaceId}/nodes/{nodeId}`, and the `metadata.drive`
 * block).
 */
async function attachOrganizationLogo(
  drive: SdkworkDriveAppClient,
  organizationId: string,
  file: File,
): Promise<SdkworkIamOrganizationLogoMediaResource> {
  const uploaded = await createSdkworkIamOrganizationDriveUploadImageService(drive).upload({
    appResourceId: organizationId,
    file,
  });
  const driveMetadata = uploaded.metadata?.drive;
  const spaceId = driveMetadata?.spaceId;
  const nodeId = driveMetadata?.nodeId;
  if (!spaceId || !nodeId) {
    throw new Error("Drive did not return the uploaded logo identity");
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
 * Transient display URL for a stored logo snapshot. Drive-backed resources
 * resolve through the shared bounded preview reader; other sources keep their
 * own delivery URL. The result is presentation-only state and never persisted.
 */
async function resolveOrganizationLogoUrl(
  drive: SdkworkDriveAppClient,
  logo: SdkworkIamOrganizationLogoMediaResource,
): Promise<string | undefined> {
  const directUrl = logo.publicUrl || logo.url;
  if (logo.source !== "drive") {
    return directUrl || logo.uri || undefined;
  }
  if (!logo.uri) {
    return directUrl || undefined;
  }
  const previewUrl = await createSdkworkIamOrganizationDriveUploadImageService(drive).resolvePreview({
    uri: logo.uri,
  });
  return previewUrl ?? undefined;
}
