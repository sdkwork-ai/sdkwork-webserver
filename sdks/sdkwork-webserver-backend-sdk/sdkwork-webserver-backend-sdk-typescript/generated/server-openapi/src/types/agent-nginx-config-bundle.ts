export interface AgentNginxConfigBundle {
  configId: string;
  domain: string;
  configContent: string;
  /** SHA-256 hex digest of configContent. */
  fingerprint: string;
  /** Config revision number as a string to avoid JavaScript precision loss. */
  version: string;
}
