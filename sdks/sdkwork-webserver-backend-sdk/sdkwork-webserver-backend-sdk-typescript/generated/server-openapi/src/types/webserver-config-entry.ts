export interface WebserverConfigEntry {
  /** Stable, content-independent catalog id (SHA-256 over the catalog identity). */
  id: string;
  kind: 'default-config' | 'import-config' | 'module-config';
  name: string;
  /** Posix path relative to the owning group root. */
  path: string;
  /** Editor language id for the file content. */
  language: string;
  size?: string;
  /** Last modification time, Unix seconds. */
  updatedAt?: string;
  /** Module sidecar configs are read-only by ownership contract. */
  writable: boolean;
}
