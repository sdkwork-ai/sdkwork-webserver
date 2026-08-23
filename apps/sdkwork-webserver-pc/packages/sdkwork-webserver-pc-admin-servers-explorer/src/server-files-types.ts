/**
 * ServerFilesExplorer domain model.
 *
 * These types describe the backend REST contract exposed by the Web Server
 * local-projects / node file-system service (see the `ServerFiles` API spec).
 * Every response is normalized here so the UI never depends on raw wire
 * shapes.
 */

/** A managed deployment node (host). */
export interface ServerNode {
  id: string;
  name: string;
  host: string;
  sshPort: number;
  status: "online" | "offline" | "unknown";
  /** Absolute filesystem root the node authorizes browsing under, e.g. `/opt/deploy`. */
  filesystemRoot: string;
  region?: string;
}

/** File-system entry kind. */
export type ServerEntryKind = "directory" | "file" | "symlink";

/**
 * Project type classification for a directory, driven by the presence of
 * known manifest files and folder shapes (project-detection.ts).
 */
export type ServerProjectType =
  | "h5-app"      // mobile web app (Vite / uni-app / Taro manifest)
  | "pc-app"      // desktop web app (Vite / webpack + browser target)
  | "flutter-app" // Flutter (pubspec.yaml)
  | "rust-backend"// Cargo.toml workspace/bin
  | "node-backend"// package.json + server entry
  | "sdkwork-workspace" // sdkwork.app.config.json monorepo root
  | "generic";

export interface ServerEntry {
  name: string;
  kind: ServerEntryKind;
  /** Absolute path within the node filesystem root. */
  path: string;
  /** Byte size for files; `undefined` for directories. */
  size?: number;
  /** RFC3339 last-modified timestamp when known. */
  modifiedAt?: string;
  /** Classified project type; only meaningful for directories. */
  projectType?: ServerProjectType;
  /** True when the directory itself owns a project manifest (not just nests one). */
  isProjectRoot?: boolean;
}

export interface ServerDirectoryListing {
  nodeId: string;
  /** Canonical absolute path of the browsed directory. */
  path: string;
  /** Parent path, or null at the filesystem root. */
  parentPath: string | null;
  entries: ServerEntry[];
}

export interface ServerFileContent {
  nodeId: string;
  path: string;
  content: string;
  /** Byte size of the file. */
  size: number;
  modifiedAt?: string;
}

/** An executable project operation offered by a project root. */
export type ServerProjectOperationKind =
  | "build"
  | "package"
  | "start"
  | "deploy"
  | "stop"
  | "restart";

export interface ServerProjectOperation {
  id: string;
  kind: ServerProjectOperationKind;
  label: string;
  /** Permission required to invoke the operation. */
  permission: string;
  /** Short help text shown in the operation menu. */
  description?: string;
  /** Danger flag surfaces a confirmation prompt before execution. */
  dangerous?: boolean;
}

export interface ServerProjectOperations {
  nodeId: string;
  path: string;
  projectType: ServerProjectType;
  operations: ServerProjectOperation[];
}

export interface ServerOperationResult {
  operationId: string;
  /** Async operation tracker when the command is long-running. */
  jobId?: string;
  exitCode?: number;
  stdout?: string;
  stderr?: string;
}
