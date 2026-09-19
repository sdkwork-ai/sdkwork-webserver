import type { AppKind } from './app-kind';
import type { ApplicationStoreListing } from './application-store-listing';

export interface ApplicationResponse {
  id: string;
  name: string;
  slug: string;
  description?: string;
  appKind?: AppKind;
  siteType: number;
  status: number;
  /** Whether the application already has a source version. The application
list and detail projections derive it in a single `EXISTS` query over
`webserver_source_version`, so callers never probe
`applications/{applicationId}/source_versions` row by row.
 */
  hasSourceVersion?: boolean;
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
  createdAt: string;
  updatedAt: string;
}
