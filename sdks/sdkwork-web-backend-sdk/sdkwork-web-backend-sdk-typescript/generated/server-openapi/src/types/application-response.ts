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
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
  createdAt: string;
  updatedAt: string;
}
