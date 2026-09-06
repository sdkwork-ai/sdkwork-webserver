import type { WebserverConfigEntry } from './webserver-config-entry';

export interface WebserverConfigCatalog {
  /** Web Server runtime configuration root the catalog was enumerated from. */
  configRoot: string;
  items: WebserverConfigEntry[];
}
