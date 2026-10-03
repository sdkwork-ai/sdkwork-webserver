// @vitest-environment jsdom

import { createSdkworkAuthController } from "@sdkwork/auth-pc-react";
import { act, cleanup, render, waitFor } from "@testing-library/react";
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
});
