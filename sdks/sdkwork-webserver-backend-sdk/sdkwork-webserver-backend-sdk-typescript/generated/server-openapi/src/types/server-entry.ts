export interface ServerEntry {
  name: string;
  kind: 'directory' | 'file' | 'symlink';
  path: string;
  size?: string;
  projectType?: 'h5-app' | 'pc-app' | 'flutter-app' | 'rust-backend' | 'node-backend' | 'sdkwork-workspace' | 'generic';
  isProjectRoot?: boolean;
}
