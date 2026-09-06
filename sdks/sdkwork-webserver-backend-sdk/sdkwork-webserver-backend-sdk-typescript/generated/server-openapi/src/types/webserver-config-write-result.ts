export interface WebserverConfigWriteResult {
  id: string;
  path: string;
  size: string;
  /** SHA-256 hex digest of the written content. */
  sha256: string;
  /** Backup file holding the previous content, when an existing file was overwritten. */
  backupPath?: string;
  /** Modification time of the written file, Unix seconds. */
  updatedAt: string;
}
