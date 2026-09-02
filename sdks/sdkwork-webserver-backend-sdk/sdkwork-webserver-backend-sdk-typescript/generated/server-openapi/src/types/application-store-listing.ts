import type { MediaResource } from './media-resource';

export interface ApplicationStoreListing {
  icon?: MediaResource;
  cover?: MediaResource;
  previews?: MediaResource[];
  shortDescription?: string;
  fullDescription?: string;
  releaseNotes?: string;
  category?: string;
  keywords?: string[];
  supportUrl?: string;
  privacyPolicyUrl?: string;
  officialWebsiteUrl?: string;
}
