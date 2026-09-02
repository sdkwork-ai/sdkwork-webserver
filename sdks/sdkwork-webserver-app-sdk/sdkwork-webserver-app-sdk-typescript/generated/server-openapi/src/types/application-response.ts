import type { AppKind } from './app-kind';
import type { ApplicationStoreListing } from './application-store-listing';

export interface ApplicationResponse {
  id?: string;
  name?: string;
  slug?: string;
  description?: string;
  /** The application's backing site id (internal carrier) */
  siteId?: string;
  appKind?: AppKind;
  /** Internal carrier site type derived from the app kind */
  siteType?: number;
  status?: number;
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
  createdAt?: string;
  updatedAt?: string;
}
