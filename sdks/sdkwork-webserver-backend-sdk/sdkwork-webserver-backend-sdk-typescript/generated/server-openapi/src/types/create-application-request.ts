import type { AppKind } from './app-kind';
import type { ApplicationStoreListing } from './application-store-listing';

export interface CreateApplicationRequest {
  name: string;
  slug?: string;
  description?: string;
  appKind: AppKind;
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
}
