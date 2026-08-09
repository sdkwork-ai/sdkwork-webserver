import { createBrowserPortalStatistics } from "../src/bootstrap/portalHost.ts";
import { describe, expect, it, vi } from "vitest";

describe("createBrowserPortalStatistics", () => {
  it("reads the active application total through the generated App SDK client", async () => {
    const list = vi.fn().mockResolvedValue({
      items: [{ id: "site-1" }],
      pageInfo: { hasMore: true, mode: "offset", totalItems: "0042" },
    });
    const loadClient = vi.fn().mockResolvedValue({ application: { list } } as never);
    const statistics = createBrowserPortalStatistics(loadClient);

    expect(loadClient).not.toHaveBeenCalled();
    await expect(statistics.load()).resolves.toEqual({ deployedApplications: "42" });
    expect(loadClient).toHaveBeenCalledOnce();
    expect(list).toHaveBeenCalledWith({ page: 1, pageSize: 1, status: 1 });
  });

  it("uses an explicit bounded fallback when the API omits totalItems", async () => {
    const statistics = createBrowserPortalStatistics(async () => ({
      application: {
        list: vi.fn().mockResolvedValue({
          items: [{ id: "site-1" }],
          pageInfo: { hasMore: true, mode: "offset" },
        }),
      },
    } as never));

    await expect(statistics.load()).resolves.toEqual({ deployedApplications: "1+" });
  });
});
