import {
  Activity,
  ArrowUp,
  Boxes,
  ChevronRight,
  File,
  FileCode2,
  FileJson,
  FileText,
  Folder,
  FolderOpen,
  Hammer,
  Loader2,
  Package,
  Play,
  RefreshCw,
  RotateCcw,
  Rocket,
  Server,
  Square,
  TriangleAlert,
  X,
} from "lucide-react";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useEffect, useMemo, useState } from "react";
import type {
  ServerDirectoryListing,
  ServerEntry,
  ServerNode,
  ServerProjectOperation,
  ServerProjectOperations,
  ServerProjectType,
} from "./server-files-types.ts";
import { createServerFilesClient, ServerFilesClient } from "./server-files-client.ts";
import { PROJECT_TYPE_LABEL } from "./project-detection.ts";

export interface ServerFilesExplorerSurfaceProps {
  backendApiBaseUrl: string;
  permissionScope?: readonly string[];
  resource: "servers-explorer";
  tokenManager: AuthTokenManager;
}

const PROJECT_TYPE_BADGE: Record<ServerProjectType, string> = {
  "h5-app": "bg-sky-100 text-sky-800",
  "pc-app": "bg-indigo-100 text-indigo-800",
  "flutter-app": "bg-teal-100 text-teal-800",
  "rust-backend": "bg-orange-100 text-orange-800",
  "node-backend": "bg-emerald-100 text-emerald-800",
  "sdkwork-workspace": "bg-violet-100 text-violet-800",
  generic: "bg-slate-100 text-slate-600",
};

const TEXT_FILE_EXTENSIONS = new Set([
  "ts", "tsx", "js", "jsx", "json", "html", "css", "scss", "less", "vue", "svelte",
  "rs", "toml", "md", "yml", "yaml", "sh", "env", "txt", "xml", "sql", "proto",
  "go", "py", "java", "kt", "dart", "c", "cpp", "h", "hpp", "properties", "conf",
]);

const NAVIGABLE_NODE_LABEL: Record<ServerProjectType, string> = PROJECT_TYPE_LABEL;

function splitPathSegments(path: string): string[] {
  const normalized = path.replace(/\\/g, "/").replace(/^\/+/, "");
  if (!normalized) return [];
  return normalized.split("/").filter(Boolean);
}

function isTextFile(name: string): boolean {
  const extension = name.includes(".") ? name.split(".").pop()?.toLowerCase() : "";
  return extension ? TEXT_FILE_EXTENSIONS.has(extension) : false;
}

function formatBytes(bytes: number | undefined): string {
  if (bytes === undefined) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ServerFilesExplorerSurface({
  backendApiBaseUrl,
  permissionScope = [],
  tokenManager,
}: ServerFilesExplorerSurfaceProps) {
  const client = useMemo(
    () => createServerFilesClient(backendApiBaseUrl, tokenManager),
    [backendApiBaseUrl, tokenManager],
  );

  const [nodes, setNodes] = useState<ServerNode[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string>("");
  const [currentPath, setCurrentPath] = useState<string>("/");
  const [listing, setListing] = useState<ServerDirectoryListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>("");

  const selectedNode = nodes.find((node) => node.id === selectedNodeId);

  // Load the node inventory once.
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    void client
      .listNodes()
      .then((items) => {
        if (!active) return;
        setNodes(items);
        if (items.length > 0 && !selectedNodeId) {
          setSelectedNodeId(items[0].id);
          setCurrentPath(items[0].filesystemRoot || "/");
        }
      })
      .catch((reason) => {
        if (active) setError(messageOf(reason));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client]);

  // Browse the current path whenever the node or path changes.
  useEffect(() => {
    if (!selectedNodeId) return;
    let active = true;
    setLoading(true);
    setError("");
    void client
      .browseDirectory(selectedNodeId, currentPath)
      .then((result) => {
        if (!active) return;
        setListing(result);
      })
      .catch((reason) => {
        if (active) {
          setError(messageOf(reason));
          setListing(null);
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [client, selectedNodeId, currentPath]);

  const segments = splitPathSegments(currentPath);
  const canWrite = hasPermission(permissionScope, "web.servers.files.write");

  function navigateTo(path: string): void {
    setCurrentPath(path);
  }

  function openEntry(entry: ServerEntry): void {
    if (entry.kind === "directory") {
      navigateTo(entry.path);
      return;
    }
    // File inspection is handled by the detail pane below (left as an
    // extension point); selecting a file toggles the inspector.
    setInspectingFile(entry);
  }

  // ---- file inspector state ----
  const [inspectingFile, setInspectingFile] = useState<ServerEntry | null>(null);
  const [fileContent, setFileContent] = useState<{ content: string; size: number } | null>(null);
  const [fileLoading, setFileLoading] = useState(false);
  const [fileError, setFileError] = useState("");

  useEffect(() => {
    if (!inspectingFile || !selectedNodeId || !isTextFile(inspectingFile.name)) {
      setFileContent(null);
      setFileError("");
      return;
    }
    let active = true;
    setFileLoading(true);
    setFileError("");
    void client
      .readFile(selectedNodeId, inspectingFile.path)
      .then((result) => {
        if (active) setFileContent({ content: result.content, size: result.size });
      })
      .catch((reason) => {
        if (active) setFileError(messageOf(reason));
      })
      .finally(() => {
        if (active) setFileLoading(false);
      });
    return () => {
      active = false;
    };
  }, [client, selectedNodeId, inspectingFile]);

  // ---- operation runner state ----
  const [operationsFor, setOperationsFor] = useState<ServerProjectOperations | null>(null);
  const [operationLoading, setOperationLoading] = useState(false);
  const [operationRunningId, setOperationRunningId] = useState("");
  const [operationOutput, setOperationOutput] = useState("");
  const [operationError, setOperationError] = useState("");

  async function inspectOperations(entry: ServerEntry): Promise<void> {
    if (!selectedNodeId || entry.kind !== "directory") return;
    setOperationLoading(true);
    setOperationError("");
    try {
      const operations = await client.operationsFor(selectedNodeId, entry.path);
      setOperationsFor(operations);
    } catch (reason) {
      setOperationError(messageOf(reason));
    } finally {
      setOperationLoading(false);
    }
  }

  async function runOperation(operation: ServerProjectOperation): Promise<void> {
    if (!operationsFor || !selectedNodeId) return;
    setOperationRunningId(operation.id);
    setOperationError("");
    setOperationOutput("");
    try {
      const result = await client.runOperation(selectedNodeId, operationsFor.path, operation.id);
      setOperationOutput(
        [
          result.stdout ? `stdout:\n${result.stdout}` : "",
          result.stderr ? `stderr:\n${result.stderr}` : "",
        ]
          .filter(Boolean)
          .join("\n") || (result.exitCode === undefined ? "Operation started." : `Exit code ${result.exitCode}`),
      );
    } catch (reason) {
      setOperationError(messageOf(reason));
    } finally {
      setOperationRunningId("");
    }
  }

  return (
    <div className="servers-explorer-surface flex h-full flex-col gap-4 p-4">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold text-slate-900">Server Files Explorer</h1>
          <p className="text-sm text-slate-500">
            Browse, classify, and operate project directories on deployment nodes.
          </p>
        </div>
        <div className="flex items-center gap-2">
          {!canWrite && (
            <span className="inline-flex items-center gap-1 rounded-md bg-amber-50 px-2 py-1 text-xs font-medium text-amber-700">
              <TriangleAlert size={13} /> Read-only access
            </span>
          )}
          <button
            className="inline-flex items-center gap-1.5 rounded-md border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
            disabled={loading}
            onClick={() => {
              setCurrentPath((path) => path);
              void reloadBrowsing(client, selectedNodeId, currentPath, setLoading, setError, setListing);
            }}
            type="button"
          >
            <RefreshCw size={14} /> Refresh
          </button>
        </div>
      </div>

      {/* Node selector */}
      <div className="flex flex-wrap items-center gap-2 rounded-lg border border-slate-200 bg-white p-3">
        <Server className="text-slate-400" size={16} />
        <span className="text-sm font-medium text-slate-600">Node</span>
        <select
          className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-800"
          value={selectedNodeId}
          onChange={(event) => {
            const next = nodes.find((node) => node.id === event.target.value);
            setSelectedNodeId(event.target.value);
            if (next) setCurrentPath(next.filesystemRoot || "/");
            setInspectingFile(null);
            setOperationsFor(null);
          }}
        >
          {nodes.length === 0 && <option value="">No nodes</option>}
          {nodes.map((node) => (
            <option key={node.id} value={node.id}>
              {node.name} ({node.host})
            </option>
          ))}
        </select>
        {selectedNode && (
          <span className="ml-auto inline-flex items-center gap-2 text-xs text-slate-500">
            <span className="inline-flex items-center gap-1">
              <span
                className={`inline-block h-2 w-2 rounded-full ${
                  selectedNode.status === "online" ? "bg-emerald-500" : "bg-slate-300"
                }`}
              />
              {selectedNode.status}
            </span>
            <code className="rounded bg-slate-100 px-1.5 py-0.5">{selectedNode.filesystemRoot}</code>
          </span>
        )}
      </div>

      {error && (
        <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          <TriangleAlert size={16} className="mt-0.5 shrink-0" />
          <span>{error}</span>
          <button className="ml-auto" onClick={() => setError("")} type="button" aria-label="Dismiss">
            <X size={14} />
          </button>
        </div>
      )}

      {/* Breadcrumb */}
      <BreadcrumbTrail
        rootLabel={selectedNode?.filesystemRoot ?? "/"}
        segments={segments}
        onNavigate={(path) => navigateTo(path)}
        onUp={() => navigateTo(listing?.parentPath ?? currentPath)}
        canGoUp={Boolean(listing?.parentPath)}
      />

      {/* Directory listing */}
      <div className="flex-1 overflow-auto rounded-lg border border-slate-200 bg-white">
        {loading ? (
          <div className="flex items-center justify-center gap-2 p-10 text-sm text-slate-500">
            <Loader2 className="animate-spin" size={16} /> Loading directory...
          </div>
        ) : listing ? (
          <EntryTable
            entries={listing.entries}
            onOpen={openEntry}
            onInspectOperations={inspectOperations}
            operationsLoading={operationLoading}
            canWrite={canWrite}
          />
        ) : (
          <div className="p-10 text-center text-sm text-slate-400">No directory selected.</div>
        )}
      </div>

      {/* Detail panes */}
      {(inspectingFile || operationsFor) && (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {inspectingFile && (
            <FileInspector
              entry={inspectingFile}
              loading={fileLoading}
              content={fileContent}
              error={fileError}
              onClose={() => setInspectingFile(null)}
            />
          )}
          {operationsFor && (
            <OperationsPanel
              operations={operationsFor}
              runningId={operationRunningId}
              output={operationOutput}
              error={operationError}
              canWrite={canWrite}
              onClose={() => setOperationsFor(null)}
              onRun={runOperation}
            />
          )}
        </div>
      )}
    </div>
  );
}

async function reloadBrowsing(
  client: ServerFilesClient,
  nodeId: string,
  path: string,
  setLoading: (value: boolean) => void,
  setError: (value: string) => void,
  setListing: (value: ServerDirectoryListing | null) => void,
): Promise<void> {
  if (!nodeId) return;
  setLoading(true);
  setError("");
  try {
    setListing(await client.browseDirectory(nodeId, path));
  } catch (reason) {
    setError(messageOf(reason));
    setListing(null);
  } finally {
    setLoading(false);
  }
}

function BreadcrumbTrail({
  rootLabel,
  segments,
  onNavigate,
  onUp,
  canGoUp,
}: {
  rootLabel: string;
  segments: string[];
  onNavigate(path: string): void;
  onUp(): void;
  canGoUp: boolean;
}) {
  const crumbs = segments.map((segment, index) => ({
    name: segment,
    path: `/${segments.slice(0, index + 1).join("/")}`,
  }));
  return (
    <div className="flex items-center gap-1 overflow-x-auto rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm">
      {canGoUp && (
        <button
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-slate-500 hover:bg-slate-100"
          onClick={onUp}
          title="Up one level"
          type="button"
        >
          <ArrowUp size={14} />
        </button>
      )}
      <button
        className="font-medium text-slate-600 hover:text-blue-600"
        onClick={() => onNavigate("/")}
        type="button"
      >
        {rootLabel}
      </button>
      {crumbs.map((crumb, index) => (
        <span className="inline-flex items-center" key={crumb.path}>
          <ChevronRight size={14} className="text-slate-300" />
          <button
            className="rounded px-1.5 py-0.5 text-slate-600 hover:bg-slate-100 hover:text-blue-600"
            onClick={() => onNavigate(crumb.path)}
            type="button"
          >
            {crumb.name}
          </button>
        </span>
      ))}
    </div>
  );
}

function EntryTable({
  entries,
  onOpen,
  onInspectOperations,
  operationsLoading,
  canWrite,
}: {
  entries: ServerEntry[];
  onOpen(entry: ServerEntry): void;
  onInspectOperations(entry: ServerEntry): void;
  operationsLoading: boolean;
  canWrite: boolean;
}) {
  const sorted = [...entries].sort((a, b) => {
    if (a.kind === "directory" && b.kind !== "directory") return -1;
    if (a.kind !== "directory" && b.kind === "directory") return 1;
    return a.name.localeCompare(b.name);
  });
  return (
    <table className="w-full border-collapse text-sm">
      <thead>
        <tr className="border-b border-slate-200 bg-slate-50 text-left text-xs font-semibold uppercase tracking-wide text-slate-500">
          <th className="px-3 py-2">Name</th>
          <th className="px-3 py-2">Kind</th>
          <th className="px-3 py-2">Project</th>
          <th className="px-3 py-2">Size</th>
          <th className="px-3 py-2 text-right">Actions</th>
        </tr>
      </thead>
      <tbody>
        {sorted.map((entry) => (
          <tr
            className="border-b border-slate-100 hover:bg-slate-50"
            key={entry.path}
          >
            <td className="px-3 py-2">
              <button
                className="inline-flex items-center gap-2 font-medium text-slate-700 hover:text-blue-600"
                onClick={() => onOpen(entry)}
                title={entry.path}
                type="button"
              >
                {entry.kind === "directory" ? (
                  <Folder className="text-amber-400" size={16} />
                ) : (
                  <FileIcon name={entry.name} />
                )}
                {entry.name}
              </button>
            </td>
            <td className="px-3 py-2 text-xs text-slate-400">{entry.kind}</td>
            <td className="px-3 py-2">
              {entry.projectType && entry.projectType !== "generic" ? (
                <span
                  className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium ${PROJECT_TYPE_BADGE[entry.projectType]}`}
                >
                  <Boxes size={11} />
                  {PROJECT_TYPE_LABEL[entry.projectType]}
                </span>
              ) : (
                <span className="text-xs text-slate-300">—</span>
              )}
            </td>
            <td className="px-3 py-2 text-xs text-slate-500">{formatBytes(entry.size)}</td>
            <td className="px-3 py-2 text-right">
              {entry.kind === "directory" ? (
                <button
                  className="inline-flex items-center gap-1 rounded-md border border-slate-300 bg-white px-2 py-1 text-xs font-medium text-slate-600 hover:bg-slate-100 disabled:opacity-50"
                  disabled={operationsLoading}
                  onClick={() => void onInspectOperations(entry)}
                  type="button"
                >
                  <Hammer size={12} /> Operations
                </button>
              ) : null}
            </td>
          </tr>
        ))}
        {sorted.length === 0 && (
          <tr>
            <td className="px-3 py-8 text-center text-slate-400" colSpan={5}>
              Empty directory.
            </td>
          </tr>
        )}
      </tbody>
    </table>
  );
}

function FileIcon({ name }: { name: string }) {
  const lower = name.toLowerCase();
  if (lower.endsWith(".json")) return <FileJson className="text-emerald-500" size={16} />;
  if (lower.endsWith(".ts") || lower.endsWith(".tsx") || lower.endsWith(".js") || lower.endsWith(".jsx")) {
    return <FileCode2 className="text-sky-500" size={16} />;
  }
  if (lower.endsWith(".md")) return <FileText className="text-slate-400" size={16} />;
  return <File className="text-slate-300" size={16} />;
}

function FileInspector({
  entry,
  loading,
  content,
  error,
  onClose,
}: {
  entry: ServerEntry;
  loading: boolean;
  content: { content: string; size: number } | null;
  error: string;
  onClose(): void;
}) {
  const previewable = isTextFile(entry.name);
  return (
    <section className="rounded-lg border border-slate-200 bg-white">
      <header className="flex items-center justify-between border-b border-slate-200 px-3 py-2">
        <div className="flex items-center gap-2 text-sm font-medium text-slate-700">
          <FileCode2 size={15} className="text-slate-400" />
          <span className="max-w-64 truncate" title={entry.path}>
            {entry.path}
          </span>
          <span className="text-xs font-normal text-slate-400">
            {entry.size !== undefined ? formatBytes(entry.size) : ""}
          </span>
        </div>
        <button
          className="rounded p-1 text-slate-400 hover:bg-slate-100"
          onClick={onClose}
          type="button"
          aria-label="Close file"
        >
          <X size={15} />
        </button>
      </header>
      <div className="p-3">
        {!previewable ? (
          <p className="text-sm text-slate-500">
            This file type is not previewable in the browser. Download or operate on it via the
            project actions.
          </p>
        ) : loading ? (
          <div className="flex items-center gap-2 text-sm text-slate-500">
            <Loader2 className="animate-spin" size={15} /> Loading file...
          </div>
        ) : error ? (
          <p className="text-sm text-red-600">{error}</p>
        ) : content ? (
          <pre className="max-h-80 overflow-auto rounded-md bg-slate-950 p-3 text-xs leading-relaxed text-slate-100">
            {content.content.length > 40_000 ? `${content.content.slice(0, 40_000)}\n… truncated` : content.content}
          </pre>
        ) : (
          <p className="text-sm text-slate-400">No content.</p>
        )}
      </div>
    </section>
  );
}

const OPERATION_ICON: Record<string, typeof Play> = {
  build: Hammer,
  package: Package,
  start: Play,
  deploy: Rocket,
  stop: Square,
  restart: RotateCcw,
};

function OperationsPanel({
  operations,
  runningId,
  output,
  error,
  canWrite,
  onClose,
  onRun,
}: {
  operations: ServerProjectOperations;
  runningId: string;
  output: string;
  error: string;
  canWrite: boolean;
  onClose(): void;
  onRun(operation: ServerProjectOperation): void;
}) {
  return (
    <section className="rounded-lg border border-slate-200 bg-white">
      <header className="flex items-center justify-between border-b border-slate-200 px-3 py-2">
        <div className="flex items-center gap-2 text-sm font-medium text-slate-700">
          <Activity size={15} className="text-slate-400" />
          <span className="max-w-64 truncate" title={operations.path}>
            {operations.path}
          </span>
          <span
            className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium ${PROJECT_TYPE_BADGE[operations.projectType]}`}
          >
            <Boxes size={11} />
            {PROJECT_TYPE_LABEL[operations.projectType]}
          </span>
        </div>
        <button
          className="rounded p-1 text-slate-400 hover:bg-slate-100"
          onClick={onClose}
          type="button"
          aria-label="Close operations"
        >
          <X size={15} />
        </button>
      </header>
      <div className="p-3">
        {!canWrite ? (
          <p className="text-sm text-slate-500">
            You need the <code className="text-slate-700">web.servers.files.write</code> permission
            to run project operations.
          </p>
        ) : operations.operations.length === 0 ? (
          <p className="text-sm text-slate-400">No operations are available for this project type.</p>
        ) : (
          <div className="flex flex-wrap gap-2">
            {operations.operations.map((operation) => {
              const Icon = OPERATION_ICON[operation.kind] ?? Play;
              const busy = runningId === operation.id;
              return (
                <button
                  className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-sm font-medium disabled:opacity-50 ${
                    operation.dangerous
                      ? "border-red-300 bg-red-50 text-red-700 hover:bg-red-100"
                      : "border-slate-300 bg-white text-slate-700 hover:bg-slate-50"
                  }`}
                  disabled={Boolean(runningId)}
                  key={operation.id}
                  onClick={() => onRun(operation)}
                  title={operation.description}
                  type="button"
                >
                  {busy ? <Loader2 className="animate-spin" size={14} /> : <Icon size={14} />}
                  {operation.label}
                </button>
              );
            })}
          </div>
        )}
        {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
        {output && (
          <pre className="mt-3 max-h-64 overflow-auto rounded-md bg-slate-950 p-3 text-xs leading-relaxed text-slate-100">
            {output}
          </pre>
        )}
      </div>
    </section>
  );
}

function hasPermission(scope: readonly string[], required: string): boolean {
  if (scope.includes("*")) return true;
  return scope.includes(required);
}

function messageOf(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  return String(reason);
}
