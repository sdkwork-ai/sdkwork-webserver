import { describe, expect, it, vi } from "vitest";

import { createSdkworkIamUserAvatarService, resolveUserAvatarUrl, uploadUserAvatar } from "@sdkwork/webserver-pc-admin-iam";

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
  } as unknown as Parameters<typeof createSdkworkIamUserAvatarService>[0];
}

describe("webserver-pc IAM admin user avatar upload", () => {
  it("uploads through the declared avatar intent and returns the drive-backed resource", async () => {
    const drive = fakeDriveClient();
    const resource = await uploadUserAvatar(drive, "user-1", fakeAvatarFile());

    expect(drive.uploader.uploadAvatar).toHaveBeenCalledWith(
      expect.objectContaining({
        // DRIVE_SPEC §18: declaration-imported intent only; no ambient identity.
        appResourceId: "user-1",
        appResourceType: "profile.avatar",
        scene: "avatar",
        source: "sdkwork-webserver-pc",
      }),
    );
    expect(resource).toMatchObject({
      // DRIVE_SPEC §10 drive-backed mapping.
      fileName: "avatar.png",
      id: "node_1",
      kind: "image",
      metadata: { drive: { nodeId: "node_1", spaceId: "space_1" } },
      mimeType: "image/png",
      sizeBytes: "12",
      source: "drive",
      uri: "drive://spaces/space_1/nodes/node_1",
    });
  });

  it("resolves a drive-backed avatar through the bounded same-origin content read", async () => {
    const drive = fakeDriveClient();
    const service = createSdkworkIamUserAvatarService(drive);

    const url = await service.resolveAvatarUrl({
      kind: "image",
      metadata: { drive: { nodeId: "node_1", spaceId: "space_1" } },
      source: "drive",
      uri: "drive://spaces/space_1/nodes/node_1",
    });

    expect(drive.drive.nodes.content.retrieve).toHaveBeenCalledWith("node_1", {
      encoding: "base64",
      maxBytes: expect.any(Number),
    });
    expect(url).toBe("data:image/png;base64,YXZhdGFyLWJ5dGVz");
  });

  it("parses the node id from the drive uri when the metadata block is missing", async () => {
    const drive = fakeDriveClient();

    await resolveUserAvatarUrl(drive, {
      kind: "image",
      source: "drive",
      uri: "drive://spaces/space_9/nodes/node_9",
    });

    expect(drive.drive.nodes.content.retrieve).toHaveBeenCalledWith("node_9", expect.anything());
  });

  it("returns the delivery URL for non-drive avatars without touching the content API", async () => {
    const drive = fakeDriveClient();

    const url = await resolveUserAvatarUrl(drive, {
      kind: "image",
      publicUrl: "https://cdn.example.com/a.png",
      source: "external_url",
    });

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

    const url = await resolveUserAvatarUrl(drive, {
      kind: "image",
      metadata: { drive: { nodeId: "node_1" } },
      source: "drive",
    });

    expect(url).toBeUndefined();
  });
});
