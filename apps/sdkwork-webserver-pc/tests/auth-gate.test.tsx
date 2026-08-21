// @vitest-environment jsdom

import { createSdkworkAuthController } from "@sdkwork/auth-pc-react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WebserverAuthGate } from "../src/auth/WebserverAuthGate.tsx";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("WebserverAuthGate", () => {
  it("bootstraps an anonymous session before redirecting to login", async () => {
    const getCurrentSession = vi.fn().mockResolvedValue(null);
    const controller = createSdkworkAuthController({ service: { getCurrentSession } });

    renderGate(controller, "/console/sites?tab=active");

    expect(screen.getByText("正在验证登录状态...")).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("location").textContent).toBe(
      "/auth/login?redirect=%2Fconsole%2Fsites%3Ftab%3Dactive",
    ));
    expect(getCurrentSession).toHaveBeenCalledOnce();
    expect(screen.getByText("auth routes")).toBeTruthy();
  });

  it("treats a failed bootstrap as anonymous and redirects to login", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const getCurrentSession = vi.fn().mockRejectedValue(new Error("offline"));
    const controller = createSdkworkAuthController({ service: { getCurrentSession } });

    renderGate(controller, "/console");

    await waitFor(() => expect(screen.getByTestId("location").textContent).toBe(
      "/auth/login?redirect=%2Fconsole",
    ));
    expect(getCurrentSession).toHaveBeenCalledOnce();
    expect(screen.getByText("auth routes")).toBeTruthy();
  });

  it("redirects an authenticated user away from an auth route", async () => {
    const controller = createSdkworkAuthController({
      initialState: {
        isBootstrapped: true,
        session: {
          accessToken: "access-token",
          authToken: "auth-token",
        },
      },
    });

    renderGate(controller, "/auth/login?redirect=%2Fadmin%2Fservers");

    await waitFor(() => expect(screen.getByTestId("location").textContent).toBe(
      "/admin/servers",
    ));
    expect(screen.getByText("protected application")).toBeTruthy();
  });
});

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{`${location.pathname}${location.search}`}</output>;
}

function renderGate(
  controller: ReturnType<typeof createSdkworkAuthController>,
  initialEntry: string,
) {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <WebserverAuthGate
        authRoutes={<div>auth routes</div>}
        controller={controller}
        locale="zh-CN"
      >
        <div>protected application</div>
      </WebserverAuthGate>
      <LocationProbe />
    </MemoryRouter>,
  );
}
