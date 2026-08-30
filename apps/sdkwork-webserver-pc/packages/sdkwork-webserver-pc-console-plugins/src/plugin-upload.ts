import { Sha256Hasher } from "@sdkwork/utils/crypto";
import { hexEncode } from "@sdkwork/utils/encoding";
import type { SdkworkDriveAppClient } from "@sdkwork/webserver-pc-console-core";

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

export async function uploadPluginArchive(
  drive: SdkworkDriveAppClient,
  file: File,
): Promise<PluginArchiveUploadResult> {
  const checksumSha256 = await calculateSha256(file);
  const uploaded = await drive.uploader.uploadArchive({
    file,
    appResourceType: "web.plugin.package",
    appResourceId: file.name,
    scene: "plugin-package",
    source: "sdkwork-webserver-pc",
    originalFileName: file.name,
    contentType: file.type || "application/zip",
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
