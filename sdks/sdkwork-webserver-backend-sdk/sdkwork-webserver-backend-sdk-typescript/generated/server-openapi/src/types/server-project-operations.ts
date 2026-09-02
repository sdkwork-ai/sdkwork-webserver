import type { ServerProjectOperation } from './server-project-operation';

export interface ServerProjectOperations {
  nodeId: string;
  path: string;
  projectType: 'h5-app' | 'pc-app' | 'flutter-app' | 'rust-backend' | 'node-backend' | 'sdkwork-workspace' | 'generic';
  operations: ServerProjectOperation[];
}
