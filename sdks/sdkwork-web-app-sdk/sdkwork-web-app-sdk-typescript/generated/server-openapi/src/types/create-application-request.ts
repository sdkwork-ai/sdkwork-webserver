import type { ApplicationStoreListing } from './application-store-listing';

export interface CreateApplicationRequest {
  name: string;
  slug?: string;
  description?: string;
  applicationType?: 'WEB' | 'API';
  siteType: 1 | 2 | 3 | 4 | 5 | 6;
  runtimeConfig?: { buildCommand?: string; outputDirectory?: string; nodeVersion?: string; installCommand?: string; startCommand?: string; };
  storeListing?: ApplicationStoreListing;
}
