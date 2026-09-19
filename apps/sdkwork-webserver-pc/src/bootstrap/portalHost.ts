import type { WebserverConsoleSdkClient } from "@sdkwork/webserver-pc-console-core";
import type { PortalClipboardPort, PortalStatisticsPort } from "@sdkwork/webserver-pc-portal";

export const browserPortalClipboard: PortalClipboardPort = {
  async writeText(value) {
    if (globalThis.navigator.clipboard?.writeText) {
      try {
        await globalThis.navigator.clipboard.writeText(value);
        return;
      } catch {
        // Some embedded browser contexts expose Clipboard but reject writes.
      }
    }

    const input = document.createElement("textarea");
    input.value = value;
    input.setAttribute("readonly", "");
    input.style.position = "fixed";
    input.style.inset = "-9999px auto auto -9999px";
    document.body.append(input);
    input.select();
    const copied = document.execCommand("copy");
    input.remove();
    if (!copied) throw new Error("Clipboard write is unavailable.");
  },
};

export function createBrowserPortalStatistics(
  loadClient: () => Promise<WebserverConsoleSdkClient>,
): PortalStatisticsPort {
  return {
    async load() {
      const client = await loadClient();
      // The tenant application count comes from the deployments-owned
      // `deploy_app` list. `apps.list` takes only `environment` / `page` /
      // `page_size`, so the count is the tenant's full application total — the
      // same figure the retired webserver face produced, whose extra `status`
      // argument was never a contract parameter and was dropped by the server.
      const result = await client.app.list({ page: 1, pageSize: 1 });
      const totalItems = result.pageInfo.totalItems?.trim();
      if (totalItems && /^\d+$/.test(totalItems)) {
        return { deployedApplications: totalItems.replace(/^0+(?=\d)/, "") };
      }
      return {
        deployedApplications: result.pageInfo.hasMore
          ? `${result.items.length}+`
          : String(result.items.length),
      };
    },
  };
}
