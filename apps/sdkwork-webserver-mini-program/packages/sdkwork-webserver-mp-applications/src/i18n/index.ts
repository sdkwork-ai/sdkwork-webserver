import type { WebserverMpMessageSources } from "@sdkwork/webserver-mp-commons";

import { webserverApplicationsListEnUs } from "./en-US/webserver/applications/list.ts";
import { webserverApplicationsListZhCn } from "./zh-CN/webserver/applications/list.ts";

/**
 * This package's contribution to the root catalog. The runtime spreads `sources`
 * in order (commons first, capability packages next, root last) when it builds
 * the catalog, so a package never reaches into another package's fragment.
 */
export const webserverApplicationsMessageSources: WebserverMpMessageSources = {
  "en-US": webserverApplicationsListEnUs,
  "zh-CN": webserverApplicationsListZhCn,
};
