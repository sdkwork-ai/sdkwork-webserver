// @vitest-environment jsdom

import { createWebserverAdminApplicationRegistry, webserverModule as applicationsModule } from "@sdkwork/webserver-pc-admin-applications";
import { createWebserverAdminSdkClient, type WebserverAdminSdkClient } from "@sdkwork/webserver-pc-admin-core";
import { WebserverWorkspace, type ApplicationMediaStorage, type ApplicationSourceStorage } from "@sdkwork/webserver-pc-commons";
import { createTokenManager } from "@sdkwork/sdk-common";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("admin workspace idempotency keys", () => {
  it("opens create dialogs when crypto.randomUUID is unavailable", async () => {
    const originalCrypto = globalThis.crypto;
    Object.defineProperty(globalThis, "crypto", {
      configurable: true,
      value: {
        getRandomValues(target: Uint8Array) {
          for (let index = 0; index < target.length; index += 1) {
            target[index] = (index * 13) & 0xff;
          }
          return target;
        },
      },
    });

    try {
      const registry = createWebserverAdminApplicationRegistry(client({}), testSourceStorage(), testMediaStorage());
      renderWorkspace("/admin/applications", registry);
      fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
      expect(screen.getByTestId("application-creation-drawer")).toBeTruthy();
    } finally {
      Object.defineProperty(globalThis, "crypto", {
        configurable: true,
        value: originalCrypto,
      });
    }
  });
});

describe("admin workspace application controls", () => {
  it("renders constrained application fields as selects", async () => {
    const create = vi.fn().mockResolvedValue({ id: "app-1" });
    const update = vi.fn().mockResolvedValue({ id: "app-1" });
    const createSourceVersion = vi.fn().mockResolvedValue({ id: "source-version-1", status: 1 });
    const createDeployment = vi.fn().mockResolvedValue({ id: "deployment-1", status: 0 });
    const sourceStorage = testSourceStorage();
    const mediaStorage = testMediaStorage();
    const registry = createWebserverAdminApplicationRegistry(client({ create, createDeployment, createSourceVersion, update }), sourceStorage, mediaStorage);
    renderWorkspace("/admin/applications", registry);

    const createButton = await screen.findByRole("button", { name: "Create application" });
    fireEvent.click(createButton);

    const drawer = screen.getByTestId("application-creation-drawer");
    expect(drawer.classList.contains("application-creation-drawer")).toBe(true);
    expect(drawer.parentElement?.classList.contains("application-creation-drawer-backdrop")).toBe(true);
    expect(document.body.classList.contains("dialog-open")).toBe(true);
    const applicationType = screen.getByLabelText("Application type");
    const siteType = screen.getByLabelText("Runtime type");
    expect(applicationType.tagName).toBe("SELECT");
    expect(siteType.tagName).toBe("SELECT");
    expect(Array.from((applicationType as HTMLSelectElement).options, (option) => option.text)).toEqual([
      "Web application",
      "API service",
    ]);
    expect(Array.from((siteType as HTMLSelectElement).options, (option) => option.text)).toEqual([
      "Static site",
      "Single-page application (SPA)",
      "Node.js",
      "PHP",
      "Python",
      "Other",
    ]);
    const wizardProgress = screen.getByTestId("application-wizard-progress");
    expect(wizardProgress).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Application basics" })).toBeTruthy();
    expect(screen.getByText("Step 1 of 5")).toBeTruthy();
    expect((screen.getByRole("button", { name: "2. Store media" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Public API" } });
    fireEvent.change(applicationType, { target: { value: "API" } });
    fireEvent.change(siteType, { target: { value: "6" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Store media and listing" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Store listing details" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Upload application icon" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Upload cover image" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Generate default" })).toBeNull();
    expect(within(screen.getByRole("group", { name: "Preview image source" })).queryByRole("button", { name: "Upload" })).toBeNull();
    expect(within(screen.getByRole("list", { name: "Preview images" })).getByRole("button", { name: "Add preview" })).toBeTruthy();
    fireEvent.change(screen.getByLabelText(/Short description/), { target: { value: "Public API gateway" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("button", { name: "Choose ZIP package" })).toBeTruthy();
    expect(screen.getByText("No source selected")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Source directory" }));
    expect(screen.getByRole("button", { name: "Choose source directory" })).toBeTruthy();
    const directoryInput = screen.getByTestId("application-source-input") as HTMLInputElement;
    expect(directoryInput.multiple).toBe(true);
    expect(directoryInput.hasAttribute("webkitdirectory")).toBe(true);
    fireEvent.change(directoryInput, {
      target: { files: [new File(["a"], "a.ts"), new File(["b"], "b.ts")] },
    });
    expect(screen.getByText("2 source files selected")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "ZIP package" }));
    expect(screen.getByText("No source selected")).toBeTruthy();
    fireEvent.change(screen.getByTestId("application-source-input"), {
      target: { files: [new File(["source"], "source.zip", { type: "application/zip" })] },
    });
    expect(screen.getByText("source.zip")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Deployment configuration" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    expect(screen.getByRole("heading", { name: "Review and create" })).toBeTruthy();
    fireEvent.click(screen.getAllByRole("button", { name: "Create application" }).at(-1)!);

    await waitFor(() => expect(create).toHaveBeenCalledWith(
      expect.objectContaining({ name: "Public API", applicationType: "API", siteType: 6 }),
      { idempotencyKey: expect.any(String) },
    ));
    expect(sourceStorage.store).toHaveBeenCalledWith(expect.objectContaining({ applicationId: "app-1" }));
    expect(mediaStorage.createDefaultIcon).toHaveBeenCalledWith("Public API");
    expect(update).toHaveBeenCalledWith("app-1", expect.objectContaining({
      storeListing: expect.objectContaining({
        icon: expect.objectContaining({
          source: "drive",
          uri: "drive://spaces/space-1/nodes/app-1-icon-0",
        }),
      }),
    }), { idempotencyKey: expect.any(String) });
    expect(createSourceVersion).toHaveBeenCalledWith("app-1", expect.objectContaining({
      artifactDriveUri: "drive://spaces/space-1/nodes/node-1",
      sourceType: "ARCHIVE",
      versionTag: "v1.0.0",
    }), { idempotencyKey: expect.any(String) });
    expect(createDeployment).toHaveBeenCalledWith("app-1", expect.objectContaining({
      sourceVersionId: "source-version-1",
      versionTag: "v1.0.0",
    }), { idempotencyKey: expect.any(String) });
  });

  it("keeps the creation order explicit and preserves basic input when going back", async () => {
    const registry = createWebserverAdminApplicationRegistry(client({}), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("alert").textContent).toContain("Enter an application name");
    expect(screen.getByRole("heading", { name: "Application basics" })).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Store listing details" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Application basics" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Source version" })).toBeTruthy();
    expect((screen.getByLabelText("Version") as HTMLInputElement).value).toBe("v1.0.0");

    const sourceTrigger = screen.getByRole("button", { name: "Choose ZIP package" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const releaseAlert = screen.getByRole("alert");
    expect(releaseAlert.textContent).toContain("Select the initial application source");
    expect(releaseAlert.textContent).not.toContain("version");
    await waitFor(() => expect(document.activeElement).toBe(sourceTrigger));

    fireEvent.change(screen.getByTestId("application-source-input"), {
      target: { files: [new File(["source"], "source.zip", { type: "application/zip" })] },
    });
    const versionInput = screen.getByLabelText("Version") as HTMLInputElement;
    fireEvent.change(versionInput, { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("alert").textContent).toContain("Enter a version");
    expect(screen.getByRole("alert").textContent).not.toContain("source");
    await waitFor(() => expect(document.activeElement).toBe(versionInput));

    fireEvent.change(versionInput, { target: { value: "v1.0.1" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Deployment configuration" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    expect(screen.getByRole("heading", { name: "Review and create" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("heading", { name: "Deployment configuration" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("heading", { name: "Source version" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("heading", { name: "Store listing details" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("heading", { name: "Application basics" })).toBeTruthy();
    expect((screen.getByLabelText("Application name") as HTMLInputElement).value).toBe("Portal");
  });

  it("previews and removes icon and cover selections from image placeholders", async () => {
    class PreviewUrl extends URL {
      static createObjectURL(file: Blob): string {
        return `blob:${(file as File).name}`;
      }

      static revokeObjectURL(): void {}
    }
    vi.stubGlobal("URL", PreviewUrl);
    vi.stubGlobal("createImageBitmap", vi.fn(async (file: Blob) => ({
      close: vi.fn(),
      height: (file as File).name.includes("cover") ? 500 : 1024,
      width: 1024,
    })));
    const registry = createWebserverAdminApplicationRegistry(client({}), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Media portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const icon = new File(["icon"], "icon.png", { type: "image/png" });
    const cover = new File(["cover"], "cover.webp", { type: "image/webp" });

    fireEvent.change(screen.getByTestId("application-icon-input"), { target: { files: [icon] } });
    expect(await screen.findByRole("img", { name: "Application icon" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Remove application icon" }));
    expect(screen.queryByRole("img", { name: "Application icon" })).toBeNull();

    fireEvent.change(screen.getByTestId("application-icon-input"), { target: { files: [icon] } });
    expect(await screen.findByRole("img", { name: "Application icon" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Remove application icon" }));

    fireEvent.change(screen.getByTestId("application-cover-input"), { target: { files: [cover] } });
    expect(await screen.findByRole("img", { name: "Cover image" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Remove cover image" }));
    expect(screen.queryByRole("img", { name: "Cover image" })).toBeNull();

    expect(screen.getByRole("heading", { name: "Store listing details" })).toBeTruthy();
  });

  it("validates and reviews a Git repository as the initial deployment source", async () => {
    const registry = createWebserverAdminApplicationRegistry(
      client({}),
      testSourceStorage(),
      testMediaStorage(),
    );
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Git portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Git repository" }));

    const repositoryInput = screen.getByLabelText("HTTPS Git repository") as HTMLInputElement;
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("alert").textContent).toContain("Enter a Git repository");
    expect(screen.getByRole("alert").textContent).not.toContain("version");
    await waitFor(() => expect(document.activeElement).toBe(repositoryInput));

    fireEvent.change(repositoryInput, { target: { value: "http://github.com/sdkwork/example.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("alert").textContent).toContain("Enter a valid HTTPS Git repository");
    await waitFor(() => expect(document.activeElement).toBe(repositoryInput));

    fireEvent.change(repositoryInput, { target: { value: "https://github.com/sdkwork/example.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Deployment configuration" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    expect(screen.getByRole("heading", { name: "Review and create" })).toBeTruthy();
    expect(screen.getByText("https://github.com/sdkwork/example.git")).toBeTruthy();
  });

  it("manages preview images as an ordered ten-slot strip", async () => {
    class PreviewUrl extends URL {
      static createObjectURL(file: Blob): string {
        return `blob:${(file as File).name}`;
      }

      static revokeObjectURL(): void {}
    }
    vi.stubGlobal("URL", PreviewUrl);
    vi.stubGlobal("createImageBitmap", vi.fn().mockResolvedValue({
      close: vi.fn(),
      height: 800,
      width: 1200,
    }));
    const registry = createWebserverAdminApplicationRegistry(client({}), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Preview portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const previewInput = screen.getByTestId("application-preview-input") as HTMLInputElement;
    const first = new File(["first"], "first.png", { lastModified: 1, type: "image/png" });
    const second = new File(["second"], "second.png", { lastModified: 2, type: "image/png" });
    const third = new File(["third"], "third.png", { lastModified: 3, type: "image/png" });

    fireEvent.change(previewInput, { target: { files: [first, second] } });
    await screen.findByText("2 / 10");
    fireEvent.change(previewInput, { target: { files: [third, first] } });
    await screen.findByText("3 / 10");

    const previewList = screen.getByRole("list", { name: "Preview images" });
    await waitFor(() => {
      expect(within(previewList).getAllByRole("listitem").map((item) => item.getAttribute("aria-label"))).toEqual([
        "first.png",
        "second.png",
        "third.png",
      ]);
    });
    fireEvent.click(screen.getByRole("button", { name: "Move preview 3 left" }));
    expect(within(previewList).getAllByRole("listitem").map((item) => item.getAttribute("aria-label"))).toEqual([
      "first.png",
      "third.png",
      "second.png",
    ]);
    fireEvent.click(screen.getByRole("button", { name: "Remove preview 1" }));
    expect(screen.getByText("2 / 10")).toBeTruthy();

    const overflow = Array.from({ length: 9 }, (_, index) => new File(
      [`overflow-${index}`],
      `overflow-${index}.png`,
      { lastModified: index + 10, type: "image/png" },
    ));
    fireEvent.change(previewInput, { target: { files: overflow } });
    expect((await screen.findByRole("alert")).textContent).toContain("no more than 10 preview images");
    expect(screen.getByText("2 / 10")).toBeTruthy();
  });

  it("reports media validation errors on the exact icon and preview fields", async () => {
    class PreviewUrl extends URL {
      static createObjectURL(file: Blob): string {
        return `blob:${(file as File).name}`;
      }

      static revokeObjectURL(): void {}
    }
    vi.stubGlobal("URL", PreviewUrl);
    vi.stubGlobal("createImageBitmap", vi.fn(async (file: Blob) => {
      const name = (file as File).name;
      return {
        close: vi.fn(),
        height: name === "bad-icon.png" ? 768 : name === "bad-preview.png" ? 100 : 1024,
        width: 1024,
      };
    }));
    const registry = createWebserverAdminApplicationRegistry(client({}), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Media validation portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    const iconInput = screen.getByTestId("application-icon-input");
    fireEvent.change(iconInput, { target: { files: [new File(["icon"], "bad-icon.png", { type: "image/png" })] } });
    const iconAlert = await screen.findByRole("alert");
    expect(iconAlert.textContent).toContain("square 1:1");
    expect(iconAlert.textContent).toContain("(actual 1024x768)");

    fireEvent.change(iconInput, { target: { files: [new File(["icon"], "good-icon.png", { type: "image/png" })] } });
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());

    const previewInput = screen.getByTestId("application-preview-input");
    fireEvent.change(previewInput, {
      target: { files: [new File(["bad"], "bad-preview.png", { type: "image/png" })] },
    });
    const previewAlert = await screen.findByRole("alert");
    expect(previewAlert.textContent).toContain("Preview image 1:");
    expect(previewAlert.textContent).toContain("Preview images must be 320-3840 px");
    expect(previewAlert.textContent).toContain("(actual 1024x100)");
    expect(screen.queryByText("1 / 10")).toBeNull();
  });

  it("prevents duplicate submissions and locks dismissal while application creation is running", async () => {
    let resolveCreate: ((value: { id: string }) => void) | undefined;
    const create = vi.fn(() => new Promise<{ id: string }>((resolve) => {
      resolveCreate = resolve;
    }));
    const createDeployment = vi.fn().mockResolvedValue({ id: "deployment-1", status: 0 });
    const sourceStorage = testSourceStorage();
    const registry = createWebserverAdminApplicationRegistry(
      client({ create, createDeployment }),
      sourceStorage,
      testMediaStorage(),
    );
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Commercial portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByTestId("application-source-input"), {
      target: { files: [new File(["source"], "source.zip", { type: "application/zip" })] },
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    const dialog = screen.getByRole("dialog");
    fireEvent.submit(dialog);
    fireEvent.submit(dialog);

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1));
    expect(sourceStorage.prepare).toHaveBeenCalledWith(expect.objectContaining({
      signal: expect.any(AbortSignal),
    }));
    expect((screen.getByRole("button", { name: "Close" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole("button", { name: "Submitting..." })).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeTruthy();

    resolveCreate?.({ id: "app-1" });
    await waitFor(() => expect(createDeployment).toHaveBeenCalledTimes(1));
  });

  it("shows a recoverable draft message when initial deployment creation fails", async () => {
    const create = vi.fn().mockResolvedValue({ id: "app-1" });
    const createDeployment = vi.fn().mockRejectedValue(new Error("provider detail must remain hidden"));
    const registry = createWebserverAdminApplicationRegistry(
      client({ create, createDeployment }),
      testSourceStorage(),
      testMediaStorage(),
    );
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Commercial portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByTestId("application-source-input"), {
      target: { files: [new File(["source"], "source.zip", { type: "application/zip" })] },
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Create application" }).at(-1)!);

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Application app-1 was created, but the initial deployment command was not accepted.",
    );
    expect(screen.queryByText("provider detail must remain hidden")).toBeNull();
  });

  it("shows the structured reason and support reference when application creation is rejected", async () => {
    const create = vi.fn().mockRejectedValue({
      code: "VALIDATION_ERROR",
      httpStatus: 422,
      problem: {
        code: 42201,
        detail: "Invalid organization context; sign in again before retrying",
        status: 422,
        traceId: "trace-application-create-42201",
      },
    });
    const registry = createWebserverAdminApplicationRegistry(
      client({ create }),
      testSourceStorage(),
      testMediaStorage(),
    );
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Commercial portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByTestId("application-source-input"), {
      target: { files: [new File(["source"], "source.zip", { type: "application/zip" })] },
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Create application" }).at(-1)!);

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("The current inputs or context cannot be processed");
    expect(alert.textContent).toContain("Invalid organization context; sign in again before retrying");
    expect(alert.textContent).toContain("Support reference: trace-application-create-42201");
    expect(alert.textContent).not.toContain("Review the inputs and try again.");
  });

  it("keeps the application draft recoverable when store media upload fails", async () => {
    const create = vi.fn().mockResolvedValue({ id: "app-1" });
    const mediaStorage = testMediaStorage();
    vi.mocked(mediaStorage.store).mockRejectedValue(new Error("provider detail must remain hidden"));
    const registry = createWebserverAdminApplicationRegistry(
      client({ create }),
      testSourceStorage(),
      mediaStorage,
    );
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Create application" }));
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Commercial portal" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByTestId("application-source-input"), {
      target: { files: [new File(["source"], "source.zip", { type: "application/zip" })] },
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Create application" }).at(-1)!);

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Application app-1 was created as a draft, but its store assets were not saved.",
    );
    expect(screen.queryByText("provider detail must remain hidden")).toBeNull();
  });

  it("closes with Escape and restores focus to the invoking command", async () => {
    const registry = createWebserverAdminApplicationRegistry(client({}), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    const createButton = await screen.findByRole("button", { name: "Create application" });
    createButton.focus();
    fireEvent.click(createButton);
    expect(document.activeElement).toBe(screen.getByLabelText("Application name"));

    const drawer = screen.getByTestId("application-creation-drawer");
    const closeButton = screen.getByRole("button", { name: "Close" });
    const continueButton = screen.getByRole("button", { name: "Continue" });
    continueButton.focus();
    fireEvent.keyDown(drawer, { key: "Tab" });
    expect(document.activeElement).toBe(closeButton);
    closeButton.focus();
    fireEvent.keyDown(drawer, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(continueButton);

    fireEvent.keyDown(document, { key: "Escape" });

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.body.classList.contains("dialog-open")).toBe(false);
    await waitFor(() => expect(document.activeElement).toBe(createButton));
  });

  it("prefills updates from the selected application", async () => {
    const update = vi.fn().mockResolvedValue({ id: "app-1" });
    const registry = createWebserverAdminApplicationRegistry(client({
      applicationItems: [{ id: "app-1", name: "Public API", description: "Current description", status: 1 }],
      update,
    }), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByText("Public API"));
    fireEvent.click(screen.getByRole("button", { name: "Edit Public API" }));

    const drawer = screen.getByTestId("application-edit-drawer");
    expect(drawer.classList.contains("application-creation-drawer")).toBe(true);
    expect(drawer.parentElement?.classList.contains("application-creation-drawer-backdrop")).toBe(true);
    expect(document.body.classList.contains("dialog-open")).toBe(true);
    expect(screen.getByRole("heading", { name: "Application basics" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Store listing details" })).toBeTruthy();
    expect((screen.getByLabelText("Application name") as HTMLInputElement).value).toBe("Public API");
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("Current description");
    fireEvent.change(screen.getByLabelText("Application name"), { target: { value: "Public API v2" } });
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(update).toHaveBeenCalledWith(
      "app-1",
      expect.objectContaining({
        name: "Public API v2",
        description: "Current description",
        storeListing: expect.objectContaining({ icon: expect.any(Object) }),
      }),
      { idempotencyKey: expect.any(String) },
    ));
  });

  it("uploads a new immutable source version from an independent row dialog", async () => {
    const createSourceVersion = vi.fn().mockResolvedValue({ id: "source-version-2", status: 1 });
    const sourceStorage = testSourceStorage();
    const registry = createWebserverAdminApplicationRegistry(client({
      applicationItems: [{ id: "app-1", name: "Public API", status: 2 }],
      createSourceVersion,
    }), sourceStorage, testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Update code Public API" }));

    const dialog = screen.getByTestId("application-source-update-dialog");
    expect(dialog.classList.contains("application-creation-drawer")).toBe(false);
    expect(dialog.parentElement?.classList.contains("application-creation-drawer-backdrop")).toBe(false);
    fireEvent.change(screen.getByLabelText("Version"), { target: { value: "v2.0.0" } });
    const archive = new File(["source-v2"], "source-v2.zip", { type: "application/zip" });
    fireEvent.change(screen.getByTestId("application-source-input"), {
      target: { files: [archive] },
    });
    fireEvent.click(screen.getByRole("button", { name: "Upload new code" }));

    await waitFor(() => expect(createSourceVersion).toHaveBeenCalledWith(
      "app-1",
      expect.objectContaining({
        artifactDriveUri: "drive://spaces/space-1/nodes/node-1",
        sourceType: "ARCHIVE",
        versionTag: "v2.0.0",
      }),
      { idempotencyKey: expect.any(String) },
    ));
    expect(sourceStorage.prepare).toHaveBeenCalledWith(expect.objectContaining({
      files: [archive],
      mode: "archive",
    }));
    expect(sourceStorage.store).toHaveBeenCalledWith(expect.objectContaining({ applicationId: "app-1" }));
  });

  it("prefills the latest Git source and refreshes it as a new version", async () => {
    const importGitSourceVersion = vi.fn().mockResolvedValue({ id: "source-version-3", status: 1 });
    const registry = createWebserverAdminApplicationRegistry(client({
      applicationItems: [{ id: "app-1", name: "Public API", status: 2 }],
      importGitSourceVersion,
      sourceVersionItems: [{
        id: "source-version-2",
        sourceRef: "https://github.com/sdkwork/public-api.git",
        sourceType: "GIT",
        versionTag: "v2.0.0",
      }],
    }), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByRole("button", { name: "Update code Public API" }));

    const repository = await screen.findByLabelText("HTTPS Git repository") as HTMLInputElement;
    await waitFor(() => expect(repository.value).toBe("https://github.com/sdkwork/public-api.git"));
    fireEvent.change(screen.getByLabelText("Version"), { target: { value: "v2.1.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Refresh repository" }));

    await waitFor(() => expect(importGitSourceVersion).toHaveBeenCalledWith(
      "app-1",
      {
        repositoryUrl: "https://github.com/sdkwork/public-api.git",
        versionTag: "v2.1.0",
      },
      { idempotencyKey: expect.any(String) },
    ));
  });

  it("publishes a retained source version from the application row and synchronizes its version tag", async () => {
    const createDeployment = vi.fn().mockResolvedValue({ id: "deployment-1", status: 0 });
    const registry = createWebserverAdminApplicationRegistry(client({
      applicationItems: [{ id: "app-1", name: "Public API", status: 2 }],
      createDeployment,
      sourceVersionItems: [{
        id: "source-version-2",
        retained: true,
        sourceType: "ARCHIVE",
        status: 1,
        versionTag: "v2.4.0",
      }],
    }), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    expect(await screen.findByRole("columnheader", { name: "Actions" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Edit Public API" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Update code Public API" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Delete Public API" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Publish Public API" }));

    expect(await screen.findByRole("option", { name: "v2.4.0 · ARCHIVE" })).toBeTruthy();
    const version = screen.getByLabelText("Version") as HTMLInputElement;
    expect(version.value).toBe("v2.4.0");
    expect(version.readOnly).toBe(true);
    fireEvent.click(screen.getByText("I understand the impact and want to continue."));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(createDeployment).toHaveBeenCalledWith(
      "app-1",
      {
        deployType: 1,
        environment: "production",
        sourceVersionId: "source-version-2",
        versionTag: "v2.4.0",
      },
      { idempotencyKey: expect.any(String) },
    ));
  });

  it("confirms deletion from the application row and disables deletion while running", async () => {
    const deleteApplication = vi.fn().mockResolvedValue(undefined);
    const registry = createWebserverAdminApplicationRegistry(client({
      applicationItems: [
        { id: "app-stopped", name: "Stopped app", status: 2 },
        { id: "app-running", name: "Running app", status: 1 },
      ],
      deleteApplication,
    }), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    const runningDelete = await screen.findByRole("button", { name: "Delete Running app" });
    expect((runningDelete as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Delete Stopped app" }));
    const confirm = screen.getByRole("button", { name: "Confirm" });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByText("I understand the impact and want to continue."));
    fireEvent.click(confirm);

    await waitFor(() => expect(deleteApplication).toHaveBeenCalledWith(
      "app-stopped",
      { idempotencyKey: expect.any(String) },
    ));
  });

  it("requires explicit confirmation before disabling an active application", async () => {
    const pause = vi.fn().mockResolvedValue({ id: "app-1", status: 2 });
    const registry = createWebserverAdminApplicationRegistry(client({
      applicationItems: [{ id: "app-1", name: "Public API", status: 1 }],
      pause,
    }), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry);

    fireEvent.click(await screen.findByText("Public API"));
    fireEvent.click(screen.getByRole("button", { name: "Disable" }));
    const confirm = screen.getByRole("button", { name: "Confirm" });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByText("I understand the impact and want to continue."));
    fireEvent.click(confirm);

    await waitFor(() => expect(pause).toHaveBeenCalledWith("app-1", { idempotencyKey: expect.any(String) }));
  });

  it("offers rollback only for a successful deployment and confirms the command", async () => {
    const rollback = vi.fn().mockResolvedValue({ id: "rollback-1", status: 0 });
    const registry = createWebserverAdminApplicationRegistry(client({
      deploymentItems: [{ id: "deployment-1", environment: "production", status: 2 }],
      rollback,
    }), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/application-deployments", registry);

    fireEvent.click(await screen.findByText("Succeeded"));
    const rollbackButton = screen.getByRole("button", { name: "Restore this version" });
    expect((rollbackButton as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(rollbackButton);
    fireEvent.click(screen.getByText("I understand the impact and want to continue."));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(rollback).toHaveBeenCalledWith(
      "app-1",
      "deployment-1",
      { idempotencyKey: expect.any(String) },
    ));
  });

  it("hides lifecycle commands without write permission", async () => {
    const registry = createWebserverAdminApplicationRegistry(client({}), testSourceStorage(), testMediaStorage());
    renderWorkspace("/admin/applications", registry, ["web.sites.read"]);

    await screen.findByText("Public API");
    expect(screen.queryByRole("button", { name: "Create application" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
  });
});

function client(overrides: {
  applicationItems?: Record<string, unknown>[];
  create?: ReturnType<typeof vi.fn>;
  createDeployment?: ReturnType<typeof vi.fn>;
  createSourceVersion?: ReturnType<typeof vi.fn>;
  deleteApplication?: ReturnType<typeof vi.fn>;
  deploymentItems?: Record<string, unknown>[];
  importGitSourceVersion?: ReturnType<typeof vi.fn>;
  sourceVersionItems?: Record<string, unknown>[];
  pause?: ReturnType<typeof vi.fn>;
  rollback?: ReturnType<typeof vi.fn>;
  update?: ReturnType<typeof vi.fn>;
}): WebserverAdminSdkClient {
  return {
    application: {
      list: vi.fn().mockResolvedValue({ items: overrides.applicationItems ?? [{ id: "app-1", name: "Public API" }], pageInfo: { page: 1, pageSize: 20, hasMore: false } }),
      create: overrides.create ?? vi.fn(),
      update: overrides.update ?? vi.fn(),
      activate: vi.fn(),
      pause: overrides.pause ?? vi.fn(),
      delete: overrides.deleteApplication ?? vi.fn(),
    },
    applicationDeployment: {
      applications: {
        deployments: {
          list: vi.fn().mockResolvedValue({ items: overrides.deploymentItems ?? [], pageInfo: { page: 1, pageSize: 20, hasMore: false } }),
          create: overrides.createDeployment ?? vi.fn(),
          rollback: overrides.rollback ?? vi.fn(),
        },
      },
    },
    applicationSourceVersion: {
      applications: {
        sourceVersions: {
          list: vi.fn().mockResolvedValue({
            items: overrides.sourceVersionItems ?? [],
            pageInfo: { page: 1, pageSize: 20, hasMore: false },
          }),
          create: overrides.createSourceVersion ?? vi.fn().mockResolvedValue({ id: "source-version-1", status: 1 }),
          gitImport: {
            create: overrides.importGitSourceVersion ?? vi.fn().mockResolvedValue({ id: "source-version-1", status: 1 }),
          },
          retrieve: vi.fn(),
        },
      },
    },
  } as unknown as WebserverAdminSdkClient;
}

function testSourceStorage(): ApplicationSourceStorage {
  return {
    prepare: vi.fn(async ({ files, mode }) => ({
      archive: files[0] ?? new File(["source"], "source.zip", { type: "application/zip" }),
      archiveHash: "a".repeat(64),
      inputMode: mode,
      sourceFileCount: files.length || 1,
      uncompressedSize: files[0]?.size ?? 6,
    })),
    store: vi.fn().mockResolvedValue({
      archiveDriveUri: "drive://spaces/space-1/nodes/node-1",
      archiveHash: "a".repeat(64),
      archiveSize: "6",
      extractedCount: "1",
      configSnapshot: {
        appConfigDetected: true,
        appConfigPath: "sdkwork.app.config.json",
        deploymentConfigDetected: true,
        deploymentConfigPath: "etc/sdkwork.deployment.config.json",
      },
    }),
  };
}

function testMediaStorage(): ApplicationMediaStorage {
  return {
    createDefaultIcon: vi.fn().mockResolvedValue(new File(["icon"], "application-icon.png", { type: "image/png" })),
    store: vi.fn(async ({ altText, applicationId, file, role, sequence = 0 }) => ({
      id: `${applicationId}-${role}-${sequence}`,
      kind: "image" as const,
      source: "drive" as const,
      uri: `drive://spaces/space-1/nodes/${applicationId}-${role}-${sequence}`,
      fileName: file.name,
      mimeType: file.type,
      sizeBytes: String(file.size),
      width: role === "cover" ? 1024 : 1024,
      height: role === "cover" ? 500 : 1024,
      altText,
      metadata: { drive: { nodeId: `${applicationId}-${role}-${sequence}`, spaceId: "space-1" } },
    })),
  };
}

function renderWorkspace(
  path: string,
  registry: ReturnType<typeof createWebserverAdminApplicationRegistry>,
  permissionScope: readonly string[] = ["*"],
): void {
  render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          path="/admin/*"
          element={
            <WebserverWorkspace
              locale="en-US"
              modules={[applicationsModule]}
              permissionScope={permissionScope}
              registry={registry}
              surface="backend-admin"
            />
          }
        />
      </Routes>
    </MemoryRouter>,
  );
}
