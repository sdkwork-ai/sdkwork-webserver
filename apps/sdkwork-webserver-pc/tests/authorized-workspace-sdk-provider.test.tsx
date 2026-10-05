// @vitest-environment jsdom

import { createSdkworkAuthController } from "@sdkwork/auth-pc-react";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { Suspense } from "react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BootstrappedWebserverPcRuntime } from "../src/bootstrap/runtime.ts";
import { WebserverAuthorizedWorkspace } from "../src/surfaces/WebserverAuthorizedWorkspace.tsx";

/**
 * The workspace body reads the console SDK context (`useCloudAccountOptions`
 * inside it), and a component cannot see the context its own JSX provides —
 * only its descendants can. Mounting the body directly beside the provider
 * instead of inside it builds and type-checks, then dies in the browser with
 * "WebserverConsoleSdkProvider is required" before a single page paints. That
 * failure mode is invisible to the source-level suites, so this test mounts the
 * real workspace over the real provider and follows the read that broke: the
 * cloud-account list must be fetched from inside the provider's subtree.
 */
describe("WebserverAuthorizedWorkspace", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("mounts the body inside the console SDK provider", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const listProviderAccounts = vi.fn().mockResolvedValue({
      items: [{ id: "acct-1", displayName: "Primary cloud account" }],
    });
    const controller = createSdkworkAuthController({
      initialState: {
        isBootstrapped: true,
        session: {
          accessToken: "access-token",
          authToken: "auth-token",
          context: { permissionScope: ["web.applications.*"], tenantId: "tenant-1", userId: "user-1" },
          user: {
            displayName: "Operator",
            email: "operator@example.com",
            firstName: "Operator",
            id: "user-1",
            initials: "O",
            lastName: "Test",
          },
        },
      },
    });
    // `use()` in the workspace re-suspends on every new promise instance, and the
    // real runtime hands back one cached promise — mirror that contract here.
    const consoleClients = Promise.resolve({
      iam: { backend: { iam: { providerAccounts: { list: listProviderAccounts } } } },
    });
    const runtime = {
      attachSdkClientBoundaries: vi.fn(),
      authController: controller,
      config: {
        appApiBaseUrl: "/",
        backendApiBaseUrl: "/",
        deployAppApiBaseUrl: "/",
        driveAppApiBaseUrl: "/",
        messagingPcUrl: "/messaging",
      },
      loadConsoleClients: () => consoleClients,
      locale: "en-US",
      setLocale: vi.fn(),
      tokenManager: { getToken: () => "access-token" },
    } as unknown as BootstrappedWebserverPcRuntime;

    // `use()` suspends the first pass even on a resolved promise, so the render
    // must be awaited inside `act` for the retry to land.
    await act(async () => {
      render(
        <MemoryRouter initialEntries={["/console"]}>
          <Suspense fallback={null}>
            <WebserverAuthorizedWorkspace locale="en-US" runtime={runtime} />
          </Suspense>
        </MemoryRouter>,
      );
    });

    // A provider-less mount throws "WebserverConsoleSdkProvider is required"
    // during render, failing the test before this point; reaching the account
    // center proves the body rendered inside the provider.
    await waitFor(() => expect(listProviderAccounts).toHaveBeenCalledWith({ page: 1, pageSize: 200 }));
  });

  /**
   * Composing the IAM face throws whenever the generated IAM SDK lags IAM's
   * registry, and the console-core getter keeps throwing on every read. The
   * cloud-account hook renders above the route-level error boundaries, so an
   * unguarded read there blanks the console and the admin surface together —
   * the exact "rest of the console still boots" contract the bootstrap comment
   * promises. A throwing face must therefore degrade to "no account
   * suggestions" and leave the workspace mounted.
   */
  it("keeps the workspace mounted when the IAM face composition throws", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const controller = createSdkworkAuthController({
      initialState: {
        isBootstrapped: true,
        session: {
          accessToken: "access-token",
          authToken: "auth-token",
          context: { permissionScope: ["web.applications.*"], tenantId: "tenant-1", userId: "user-1" },
          user: {
            displayName: "Operator",
            email: "operator@example.com",
            firstName: "Operator",
            id: "user-1",
            initials: "O",
            lastName: "Test",
          },
        },
      },
    });
    const consoleClients = Promise.resolve({
      // Mirrors `createWebserverConsoleSdkClients`: a face whose composition
      // failed stays undefined, so every property read throws anew.
      get iam(): never {
        throw new Error("The IAM backend SDK adapter is required to manage IAM-owned resources.");
      },
    });
    const runtime = {
      attachSdkClientBoundaries: vi.fn(),
      authController: controller,
      config: {
        appApiBaseUrl: "/",
        backendApiBaseUrl: "/",
        deployAppApiBaseUrl: "/",
        driveAppApiBaseUrl: "/",
        messagingPcUrl: "/messaging",
      },
      loadConsoleClients: () => consoleClients,
      locale: "en-US",
      setLocale: vi.fn(),
      tokenManager: { getToken: () => "access-token" },
    } as unknown as BootstrappedWebserverPcRuntime;

    await act(async () => {
      render(
        <MemoryRouter initialEntries={["/console"]}>
          <Suspense fallback={null}>
            <WebserverAuthorizedWorkspace locale="en-US" runtime={runtime} />
          </Suspense>
        </MemoryRouter>,
      );
    });

    // The workspace shell is up — its `<main class="workspace">` landmark
    // rendered and stayed rendered, proving the throw did not unmount the
    // route tree into a white screen.
    await waitFor(() => expect(screen.getByRole("main")).toBeTruthy());
  });
});
