import type { MediaChecksum } from './media-checksum';

export interface MediaResource {
  id?: string;
  kind: 'image' | 'video' | 'audio' | 'voice' | 'document' | 'archive' | 'model' | 'other';
  source: 'drive' | 'external_url' | 'data_url' | 'provider_asset' | 'generated';
  url?: string;
  publicUrl?: string;
  uri?: string;
  objectBlobId?: string;
  fileName?: string;
  mimeType?: string;
  sizeBytes?: string;
  checksum?: MediaChecksum;
  width?: number;
  height?: number;
  durationSeconds?: number;
  altText?: string;
  title?: string;
  metadata?: Record<string, unknown>;
}
