export interface WebserverConfigFile {
  id: string;
  kind: 'default-config' | 'import-config' | 'module-config';
  name: string;
  path: string;
  language: string;
  writable: boolean;
  /** Decoded text content, bounded by the read size limit. */
  content: string;
  size: string;
  /** SHA-256 hex digest of the on-disk bytes. */
  sha256: string;
  /** Last modification time, Unix seconds. */
  updatedAt?: string;
}
