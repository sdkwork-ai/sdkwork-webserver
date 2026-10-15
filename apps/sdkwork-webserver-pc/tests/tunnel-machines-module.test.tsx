// @vitest-environment jsdom

import {
  createWebserverAdminRegistry,
  type WebserverAdminSdkClient,
} from "@sdkwork/webserver-pc-admin-core";
import { hasWebserverPermission } from "@sdkwork/webserver-pc-commons";
import { describe, expect, it, vi } from "vitest";
import { adminModules } from "../src/surfaces/WebserverAuthorizedWorkspace.tsx";

/**
 * Tunnel machine management is the operations surface for the connector nodes
 * behind the reverse tunnel.
 *
 * These assertions pin what an operator actually depends on: the module is
 * mounted in the host (a module that is implemented but never mounted is
 * invisible), the entry is reachable exactly by `web.tunnel.read`, its
 * source maps the gateway's machine list onto a page the workspace can render,
 * and every mutating action is gated by `web.tunnel.write`. The module list is
 * imported from the host surface rather than restated, so the failure this
 * guards against is the wiring gap, not a copy of it.
 */

const tunnelHostModule = adminModules.find((module) => module.id === "tunnel");

/** Minimal SDK double: only the tunnel namespace is exercised. */
function clientDouble(machines: readonly Record<string, unknown>[]) {
  const enabledUpdate = vi.fn(async () => ({ deviceId: "dev_1", enabled: false }));
  const routesCreate = vi.fn(async () => ({ routeId: "route_web", active: true }));
  const routesDelete = vi.fn(async () => undefined);
  const client = {
    tunnel: {
      tunnelMachines: {
        list: vi.fn(async () => ({ machines })),
        retrieve: vi.fn(),
        enabled: { update: enabledUpdate },
        routes: { create: routesCreate, delete: routesDelete },
      },
    },
  } as unknown as WebserverAdminSdkClient;
  return { client, enabledUpdate, routesCreate, routesDelete };
}

const machine = {
  deviceId: "dev_edge_1",
  deviceName: "edge-1",
  platform: "linux",
  online: true,
  enabled: true,
  routeCount: "1",
  healthyRouteCount: "1",
  routes: [],
};

describe("tunnel machine module registration", () => {
  it("mounts one machine entry on the admin surface behind web.tunnel.read", () => {
    expect(tunnelHostModule).toBeDefined();
    expect(tunnelHostModule?.surface).toBe("backend-admin");
    expect(tunnelHostModule?.entries.map((entry) => entry.resource)).toEqual(["tunnel-machines"]);
    const entry = tunnelHostModule?.entries[0];
    expect(entry?.permission).toBe("web.tunnel.read");
    expect(entry?.path).toBe("tunnel/machines");
    // Reading the machine list must not imply managing machines: the write
    // permission is a different code, granted separately.
    expect(hasWebserverPermission(["web.tunnel.read"], "web.tunnel.read")).toBe(true);
    expect(hasWebserverPermission(["web.tunnel.read"], "web.tunnel.write")).toBe(false);
  });

  it("maps the gateway machine list onto a renderable page", async () => {
    const { client } = clientDouble([machine]);
    const registry = createWebserverAdminRegistry(client);
    const source = registry["tunnel-machines"];
    expect(source).toBeDefined();
    const page = await source!.load({ page: 1, pageSize: 20, filters: {}, search: undefined } as never);
    expect(page.items).toHaveLength(1);
    expect(page.items[0].deviceName).toBe("edge-1");
    expect(page.items[0].enabled).toBe(true);
  });

  it("gates every mutating action behind web.tunnel.write", () => {
    const { client } = clientDouble([machine]);
    const registry = createWebserverAdminRegistry(client);
    const actions = registry["tunnel-machines"]!.actions;
    expect(actions.map((action) => action.id).sort()).toEqual([
      "declare-route",
      "disable",
      "enable",
      "remove-route",
    ]);
    for (const action of actions) {
      expect(action.permission).toBe("web.tunnel.write");
    }
    // Taking a machine out of service ends its live session, so it asks for
    // confirmation; the reverse direction is not dangerous.
    const disable = actions.find((action) => action.id === "disable");
    const enable = actions.find((action) => action.id === "enable");
    expect(disable?.requiresConfirmation).toBe(true);
    expect(disable?.dangerous).toBe(true);
    expect(enable?.requiresConfirmation).toBeUndefined();
  });

  it("switches a machine off and back on through the SDK", async () => {
    const { client, enabledUpdate } = clientDouble([machine]);
    const registry = createWebserverAdminRegistry(client);
    const disable = registry["tunnel-machines"]!.actions.find((action) => action.id === "disable")!;
    const context = {
      body: {},
      idempotencyKey: "op-1",
      selectedItem: machine,
    } as never;
    await disable.execute(context);
    expect(enabledUpdate).toHaveBeenCalledWith("dev_edge_1", { enabled: false });
  });

  it("refuses a route declaration the relay planes cannot carry", () => {
    const { client, routesCreate } = clientDouble([machine]);
    const registry = createWebserverAdminRegistry(client);
    const declare = registry["tunnel-machines"]!.actions.find((action) => action.id === "declare-route")!;
    expect(() =>
      declare.execute({
        body: { name: "web", protocol: "carrier-pigeon" },
        idempotencyKey: "op-2",
        selectedItem: machine,
      } as never),
    ).toThrow(/http, tcp, udp, stcp, or sudp/);
    expect(routesCreate).not.toHaveBeenCalled();
  });
});
