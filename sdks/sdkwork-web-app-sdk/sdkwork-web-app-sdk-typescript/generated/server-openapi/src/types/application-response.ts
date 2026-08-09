import type { ApplicationStoreListing } from './application-store-listing';

export interface ApplicationResponse {
  id?: string;
  name?: string;
  slug?: string;
  description?: string;
  /** The application's backing site id (internal carrier) */
  siteId?: string;
  applicationType?: 'WEB' | 'API';
  siteType?: number;
  status?: number;
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
  createdAt?: string;
  updatedAt?: string;
}
