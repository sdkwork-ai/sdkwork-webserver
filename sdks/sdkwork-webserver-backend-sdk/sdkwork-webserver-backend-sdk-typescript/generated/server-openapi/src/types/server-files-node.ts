export interface ServerFilesNode {
  id: string;
  name: string;
  host: string;
  sshPort: number;
  status: 'online' | 'offline' | 'unknown';
  /** Authorized filesystem root the node may browse (e.g. /opt/deploy). */
  filesystemRoot: string;
  region?: string;
}
