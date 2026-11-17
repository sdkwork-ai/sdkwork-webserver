import type { AppKind } from './app-kind';
import type { ApplicationStoreListing } from './application-store-listing';

export interface ApplicationResponse {
  id: string;
  name: string;
  slug: string;
  description?: string;
  appKind?: AppKind;
  /** The `deploy_app.id` this carrier record carries. Absent only on a
row created before the link existed; those rows are backfilled by an
operator from the Deploy inventory (migration `0031`).
 */
  deployApplicationId?: string;
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
