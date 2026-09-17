import type { SdkworkAuthController } from "@sdkwork/auth-pc-react";
import { describe, expect, it, vi } from "vitest";
import { dedupeAuthControllerBootstrap } from "../src/bootstrap/authBootstrapDedupe.ts";

/**
 * The guard only needs the controller surface it actually wraps, so the fixture
 * stays a partial stand-in instead of a full IAM controller.
 */
function createController(
  bootstrap: SdkworkAuthController["bootstrap"],
): SdkworkAuthController {
  return {
    applySession: vi.fn(),
    bootstrap,
    getState: () => ({ isBootstrapped: false }),
    service: {},
    signOut: vi.fn(async () => undefined),
    subscribe: () => () => undefined,
  } as unknown as SdkworkAuthController;
}

describe("webserver auth bootstrap de-duplication", () => {
  it("collapses concurrent callers onto a single in-flight request", async () => {
    let resolveRequest: (value: unknown) => void = () => undefined;
    const bootstrap = vi.fn(() => new Promise((resolve) => { resolveRequest = resolve; }));
    const controller = dedupeAuthControllerBootstrap(createController(bootstrap as never));

    const first = controller.bootstrap();
    const second = controller.bootstrap();

    expect(bootstrap).toHaveBeenCalledTimes(1);

    resolveRequest({ isBootstrapped: true });

    await expect(first).resolves.toEqual({ isBootstrapped: true });
    await expect(second).resolves.toEqual({ isBootstrapped: true });
    expect(bootstrap).toHaveBeenCalledTimes(1);
  });

  it("reaches the network again once the in-flight request has settled", async () => {
    const bootstrap = vi.fn(async () => ({ isBootstrapped: true }));
    const controller = dedupeAuthControllerBootstrap(createController(bootstrap as never));

    await controller.bootstrap();
    await controller.bootstrap();

    expect(bootstrap).toHaveBeenCalledTimes(2);
  });

  it("does not cache a failure, so an explicit retry issues a new request", async () => {
    const bootstrap = vi.fn()
      .mockRejectedValueOnce(new Error("session unavailable"))
      .mockResolvedValueOnce({ isBootstrapped: true });
    const controller = dedupeAuthControllerBootstrap(createController(bootstrap as never));

    await expect(controller.bootstrap()).rejects.toThrow("session unavailable");
    await expect(controller.bootstrap()).resolves.toEqual({ isBootstrapped: true });

    expect(bootstrap).toHaveBeenCalledTimes(2);
  });

  it("passes the rest of the controller surface through unchanged", async () => {
    const bootstrap = vi.fn(async () => ({ isBootstrapped: true }));
    const controller = dedupeAuthControllerBootstrap(createController(bootstrap as never));

    expect(controller.getState()).toEqual({ isBootstrapped: false });
    await expect(controller.signOut()).resolves.toBeUndefined();

    controller.applySession({ accessToken: "access", authToken: "auth" });

    expect(controller.applySession).toHaveBeenCalledTimes(1);
    expect(controller.service).toEqual({});
  });
});
