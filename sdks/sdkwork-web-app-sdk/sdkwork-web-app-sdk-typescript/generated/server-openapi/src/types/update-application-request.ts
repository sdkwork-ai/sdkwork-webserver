import type { ApplicationStoreListing } from './application-store-listing';

export interface UpdateApplicationRequest {
  name?: string;
  description?: string;
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
}
