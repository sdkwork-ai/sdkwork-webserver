import type { WebserverH5MessageSources } from "@sdkwork/webserver-h5-commons";

import { webserverApplicationsListEnUs } from "./en-US/webserver/applications/list.ts";
import { webserverApplicationsListZhCn } from "./zh-CN/webserver/applications/list.ts";

/**
 * This package's contribution to the root catalog. The application root spreads
 * `sources` in order (commons first, feature packages next, root last) when it
 * builds the catalog, so a package never reaches into another package's fragment.
 */
export const webserverApplicationsMessageSources: WebserverH5MessageSources = {
  "en-US": webserverApplicationsListEnUs,
  "zh-CN": webserverApplicationsListZhCn,
};
