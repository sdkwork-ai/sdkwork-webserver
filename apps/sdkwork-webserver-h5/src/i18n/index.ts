import {
  createWebserverH5MessageCatalog,
  webserverCommonsMessageSources,
} from "@sdkwork/webserver-h5-commons";
import { webserverApplicationsMessageSources } from "@sdkwork/webserver-h5-applications";

import { webserverShellApplicationEnUs } from "./en-US/webserver/shell/application.ts";
import { webserverShellApplicationZhCn } from "./zh-CN/webserver/shell/application.ts";

/**
 * The root catalog. Contribution order is the override order: commons chrome
 * first, then each feature package, then this root's own shell fragment, so a
 * more specific surface always wins over a shared default. Every package owns
 * its own fragment and no package reads another package's keys.
 */
export const webserverH5MessageCatalog = createWebserverH5MessageCatalog({
  sources: [
    webserverCommonsMessageSources,
    webserverApplicationsMessageSources,
    {
      "en-US": webserverShellApplicationEnUs,
      "zh-CN": webserverShellApplicationZhCn,
    },
  ],
});

/** Resolver bound to one locale; the shape feature packages receive. */
export type WebserverH5MessageResolver = (key: string) => string;

export function createWebserverH5MessageResolver(locale: string): WebserverH5MessageResolver {
  return (key) => webserverH5MessageCatalog.resolve(locale, key);
}
