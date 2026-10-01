import { Sha256Hasher } from "@sdkwork/utils/crypto";
import { hexEncode } from "@sdkwork/utils/encoding";
import {
  WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD,
  WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD_RESOURCE_ID,
  type SdkworkDriveAppClient,
} from "@sdkwork/webserver-pc-console-core";

export interface PluginArchiveUploadResult {
  artifactRef: string;
  checksumSha256: string;
  sizeBytes: string;
}

async function calculateSha256(file: File): Promise<string> {
  const hasher = new Sha256Hasher();
  const reader = file.stream().getReader();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      hasher.update(value);
    }
  } finally {
    reader.releaseLock();
  }
  return hexEncode(hasher.digest());
}

/**
 * The honest content type for an accepted plugin archive, derived from the
 * file extension. `file.type` is a browser guess that is frequently empty and
 * mislabels `.tar` as `application/zip`; the upload declaration contract
 * (DRIVE_SPEC §18) requires the declared statistic dimensions to be truthful,
 * and the content type is one of them.
 */
export function pluginArchiveContentType(file: File): string {
  const name = file.name.toLowerCase();
  if (name.endsWith(".tar.gz") || name.endsWith(".tgz") || name.endsWith(".gz")) {
    return "application/gzip";
  }
  if (name.endsWith(".tar")) {
    return "application/x-tar";
  }
  return "application/zip";
}

export async function uploadPluginArchive(
  drive: SdkworkDriveAppClient,
  file: File,
): Promise<PluginArchiveUploadResult> {
  const checksumSha256 = await calculateSha256(file);
  const uploaded = await drive.uploader.uploadArchive({
    file,
    appResourceType: WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD.appResourceType,
    appResourceId: WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD_RESOURCE_ID,
    scene: WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD.scene,
    source: WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD.source,
    originalFileName: file.name,
    contentType: pluginArchiveContentType(file),
    checksumSha256Hex: `sha256:${checksumSha256}`,
    fileFingerprint: checksumSha256,
  });
  const spaceId = uploaded.uploadSession.spaceId ?? uploaded.uploadItem.spaceId;
  const nodeId = uploaded.uploadSession.nodeId ?? uploaded.uploadItem.nodeId;
  if (!spaceId || !nodeId) {
    throw new Error("Drive did not return the plugin archive identity");
  }
  return {
    artifactRef: `drive://spaces/${spaceId}/nodes/${nodeId}`,
    checksumSha256,
    sizeBytes: String(file.size),
  };
}
