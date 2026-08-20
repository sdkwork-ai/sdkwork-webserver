import type { AppKind } from './app-kind';
import type { ApplicationStoreListing } from './application-store-listing';

export interface CreateApplicationRequest {
  name: string;
  slug?: string;
  description?: string;
  appKind: AppKind;
  runtimeConfig?: { buildCommand?: string; outputDirectory?: string; nodeVersion?: string; installCommand?: string; startCommand?: string; };
  storeListing?: ApplicationStoreListing;
}
