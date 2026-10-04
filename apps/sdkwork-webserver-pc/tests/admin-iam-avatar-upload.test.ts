import { describe, expect, it, vi } from "vitest";

import { createSdkworkIamUserDriveUploadImageService } from "@sdkwork/webserver-pc-admin-iam";

/**
 * Host avatar capability contract for the IAM admin user directory.
 *
 * The webserver owns exactly one thing: the declared upload intent bound into
 * the shared `DriveUploadImageService` (`DRIVE_SPEC.md` §18 — the service
 * layer, not the UI, supplies declared values) and the bounded same-origin
 * preview reader. Picking, parking, and uploading live in the shared
 * `DriveUploadImage` component (pinned by the iam workspace tests).
 */

function fakeAvatarFile(): File {
  return new File(["avatar-bytes"], "avatar.png", { type: "image/png" });
}

function fakeDriveClient() {
  return {
    drive: {
      nodes: {
        content: {
          retrieve: vi.fn().mockResolvedValue({
            content: "YXZhdGFyLWJ5dGVz",
            contentType: "image/png",
            hasMore: false,
            nodeId: "node_1",
          }),
        },
      },
    },
    uploader: {
      uploadAvatar: vi.fn().mockResolvedValue({
        uploadItem: {
          contentLength: "12",
          contentType: "image/png",
          nodeId: "node_1",
          originalFileName: "avatar.png",
          spaceId: "space_1",
        },
      }),
    },
  } as unknown as Parameters<typeof createSdkworkIamUserDriveUploadImageService>[0];
}

describe("webserver-pc IAM admin user avatar upload", () => {
  it("binds the declared avatar intent and returns the persist-safe drive value", async () => {
    const drive = fakeDriveClient();
    const service = createSdkworkIamUserDriveUploadImageService(drive);

    const value = await service.upload({ file: fakeAvatarFile(), appResourceId: "user-1" });

    expect(drive.uploader.uploadAvatar).toHaveBeenCalledWith(
      expect.objectContaining({
        // DRIVE_SPEC §18: declaration-imported intent only; no ambient identity.
        appResourceId: "user-1",
        appResourceType: "profile.avatar",
        scene: "avatar",
        source: "sdkwork-webserver-pc",
      }),
    );
    expect(value).toMatchObject({
      // Persist-safe reference only: stable uri, source tag, drive identity.
      metadata: { drive: { nodeId: "node_1", spaceId: "space_1" } },
      source: "drive",
      uri: "drive://spaces/space_1/nodes/node_1",
    });
  });

  it("refuses to upload without an entity anchor (persist-first guard)", async () => {
    const drive = fakeDriveClient();
    const service = createSdkworkIamUserDriveUploadImageService(drive);

    await expect(
      service.upload({ file: fakeAvatarFile(), appResourceId: "" }),
    ).rejects.toMatchObject({ code: "missing-app-resource-id" });
    expect(drive.uploader.uploadAvatar).not.toHaveBeenCalled();
  });

  it("resolves a drive-backed avatar through the bounded same-origin content read", async () => {
    const drive = fakeDriveClient();
    const service = createSdkworkIamUserDriveUploadImageService(drive);

    const url = await service.resolvePreview({
      uri: "drive://spaces/space_1/nodes/node_1",
    });

    expect(drive.drive.nodes.content.retrieve).toHaveBeenCalledWith("node_1", {
      encoding: "base64",
      maxBytes: expect.any(Number),
    });
    expect(url).toBe("data:image/png;base64,YXZhdGFyLWJ5dGVz");
  });

  it("passes external avatar URLs through without touching the content API", async () => {
    const drive = fakeDriveClient();
    const service = createSdkworkIamUserDriveUploadImageService(drive);

    const url = await service.resolvePreview({ uri: "https://cdn.example.com/a.png" });

    expect(url).toBe("https://cdn.example.com/a.png");
    expect(drive.drive.nodes.content.retrieve).not.toHaveBeenCalled();
  });

  it("degrades to no preview when the bounded read reports truncation", async () => {
    const drive = fakeDriveClient();
    (drive.drive.nodes.content.retrieve as ReturnType<typeof vi.fn>).mockResolvedValue({
      content: "cGFydGlhbA==",
      contentType: "image/png",
      hasMore: true,
      nodeId: "node_1",
    });
    const service = createSdkworkIamUserDriveUploadImageService(drive);

    const url = await service.resolvePreview({
      uri: "drive://spaces/space_1/nodes/node_1",
    });

    expect(url).toBeNull();
  });
});
