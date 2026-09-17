import { createWebserverAdminRegistry, type WebserverAdminSdkClient } from "@sdkwork/webserver-pc-admin-core";
import { describe, expect, it, vi } from "vitest";

describe("admin control-plane capability", () => {
  it("uses canonical Nginx, server, and audit SDK contracts", async () => {
    const createConfig = vi.fn().mockResolvedValue({ id: "config-1" });
    const updateConfig = vi.fn().mockResolvedValue({ id: "config-1" });
    const createServer = vi.fn().mockResolvedValue({ id: "server-1", agentToken: "one-time-token" });
    const listAudit = vi.fn().mockResolvedValue({ items: [], pageInfo: { page: 1, pageSize: 20, hasMore: false } });
    const client = {
      nginx: {
        configs: {
          list: vi.fn().mockResolvedValue({ items: [], pageInfo: { page: 1, pageSize: 20, hasMore: false } }),
          create: createConfig,
          update: updateConfig,
          validate: vi.fn(),
          deploy: vi.fn(),
        },
        reload: { create: vi.fn() },
        status: { retrieve: vi.fn().mockResolvedValue({ status: "ok" }) },
      },
      server: {
        list: vi.fn().mockResolvedValue({ items: [], pageInfo: { page: 1, pageSize: 20, hasMore: false } }),
        create: createServer,
      },
      audit: { auditLogs: { list: listAudit } },
    } as unknown as WebserverAdminSdkClient;
    const registry = createWebserverAdminRegistry(client);

    await registry.nginx?.actions.find((candidate) => candidate.id === "create")?.execute({ body: { siteId: "site-1", configType: 1, configName: "edge", configContent: "events {}" }, idempotencyKey: "config-create-1" });
    await registry.nginx?.actions.find((candidate) => candidate.id === "update")?.execute({ body: { configName: "edge-v2", configContent: "events {}" }, idempotencyKey: "config-update-1", selectedItem: { id: "config-1" } });
    expect(createConfig).toHaveBeenCalledWith({ siteId: "site-1", configType: 1, configName: "edge", configContent: "events {}" }, { idempotencyKey: "config-create-1" });
    expect(updateConfig).toHaveBeenCalledWith("config-1", { configName: "edge-v2", configContent: "events {}" }, { idempotencyKey: "config-update-1" });

    const register = registry.servers?.actions.find((candidate) => candidate.id === "create");
    const tenantScopeHash = "a".repeat(64);
    await register?.execute({ body: { name: "edge-1", host: "10.0.0.8", sshPort: 22, tenantScopeHash }, idempotencyKey: "server-create-1" });
    expect(register?.resultFields).toContain("agentToken");
    expect(createServer).toHaveBeenCalledWith({ name: "edge-1", host: "10.0.0.8", sshPort: 22, tenantScopeHash }, { idempotencyKey: "server-create-1" });

    await registry.audit?.load({
      filters: { targetType: "deployment", action: "sites.rollback", operatorId: "42", startDate: "2026-07-01", endDate: "2026-07-28" },
      page: 2,
      pageSize: 20,
    });
    expect(listAudit).toHaveBeenCalledWith({
      cursor: undefined,
      pageSize: 20,
      targetType: "deployment",
      action: "sites.rollback",
      operatorId: "42",
      startDate: "2026-07-01",
      endDate: "2026-07-28",
    });
  });

  it("rejects invalid Nginx and server inputs before generated SDK calls", async () => {
    const createConfig = vi.fn();
    const createServer = vi.fn();
    const client = {
      nginx: { configs: { create: createConfig } },
      server: { create: createServer },
    } as unknown as WebserverAdminSdkClient;
    const registry = createWebserverAdminRegistry(client);
    const createNginx = registry.nginx?.actions.find((candidate) => candidate.id === "create");
    const registerServer = registry.servers?.actions.find((candidate) => candidate.id === "create");
    if (!createNginx || !registerServer) throw new Error("admin control-plane actions are unavailable");

    await expect(createNginx.execute({
      body: { configType: 1, configName: "edge", configContent: "events {}" },
      idempotencyKey: "invalid-nginx-site",
    })).rejects.toThrow("Site ID is required");
    await expect(createNginx.execute({
      body: { siteId: "site-1", configType: 1, configName: "edge", configContent: "x".repeat(1024 * 1024 + 1) },
      idempotencyKey: "oversized-nginx-config",
    })).rejects.toThrow("must not exceed 1 MiB");
    expect(createConfig).not.toHaveBeenCalled();

    await expect(registerServer.execute({
      body: { name: "edge-1", host: "10.0.0.8", sshPort: 0, tenantScopeHash: "a".repeat(64) },
      idempotencyKey: "invalid-server-port",
    })).rejects.toThrow("SSH port must be between 1 and 65535");
    await expect(registerServer.execute({
      body: { name: "edge-1", host: "10.0.0.8", sshPort: 22, tenantScopeHash: "tenant-hash" },
      idempotencyKey: "invalid-server-scope",
    })).rejects.toThrow("lowercase SHA-256 digest");
    expect(createServer).not.toHaveBeenCalled();
  });
});
