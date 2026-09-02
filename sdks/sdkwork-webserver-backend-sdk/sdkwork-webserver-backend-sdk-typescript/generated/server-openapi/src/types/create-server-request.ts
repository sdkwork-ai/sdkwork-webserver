export interface CreateServerRequest {
  name: string;
  host: string;
  /** Irreversible tenant scope bound to runtime-set delivery for this node. */
  tenantScopeHash: string;
  sshPort: number;
}
