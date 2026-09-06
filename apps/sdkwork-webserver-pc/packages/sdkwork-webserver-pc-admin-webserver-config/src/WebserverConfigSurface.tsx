import { Editor } from "@monaco-editor/react";
import {
  CircleAlert,
  FileCog,
  FileCode2,
  HardDrive,
  Loader2,
  Lock,
  Puzzle,
  RefreshCw,
  Save,
  Search,
  ShieldCheck,
  X,
} from "lucide-react";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useEffect, useMemo, useState } from "react";
import type {
  WebserverConfigCatalog,
  WebserverConfigEntry,
  WebserverConfigFile,
  WebserverConfigWriteResult,
} from "@sdkwork/webserver-pc-admin-core";
import {
  CONFIG_KIND_DESCRIPTION,
  CONFIG_KIND_LABEL,
  CONFIG_KIND_ORDER,
  validateConfigContent,
  type WebserverConfigKindId,
} from "./config-language.ts";
import { createWebserverConfigClient } from "./webserver-config-client.ts";

export interface WebserverConfigSurfaceProps {
  backendApiBaseUrl: string;
  permissionScope?: readonly string[];
  resource: "webserver-config";
  tokenManager: AuthTokenManager;
}

interface SaveOutcome {
  result: WebserverConfigWriteResult;
}

function formatBytes(bytes: string | number | undefined): string {
  if (bytes === undefined || bytes === "") return "";
  const parsed = typeof bytes === "number" ? bytes : Number(bytes);
  if (!Number.isFinite(parsed) || parsed < 0) return "";
  if (parsed < 1024) return `${parsed} B`;
  if (parsed < 1024 * 1024) return `${(parsed / 1024).toFixed(1)} KB`;
  return `${(parsed / (1024 * 1024)).toFixed(1)} MB`;
}

function formatUnixSeconds(seconds: string | undefined): string {
  if (!seconds) return "";
  const parsed = Number(seconds);
  if (!Number.isFinite(parsed) || parsed <= 0) return "";
  return new Date(parsed * 1000).toLocaleString();
}

function hasPermission(scope: readonly string[], required: string): boolean {
  if (scope.includes("*")) return true;
  return scope.includes(required);
}

function messageOf(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  return String(reason);
}

const KIND_BADGE: Record<WebserverConfigKindId, string> = {
  "default-config": "bg-indigo-100 text-indigo-800",
  "import-config": "bg-emerald-100 text-emerald-800",
  "module-config": "bg-slate-200 text-slate-600",
};

export function WebserverConfigSurface({
  backendApiBaseUrl,
  permissionScope = [],
  tokenManager,
}: WebserverConfigSurfaceProps) {
  const client = useMemo(
    () => createWebserverConfigClient(backendApiBaseUrl, tokenManager),
    [backendApiBaseUrl, tokenManager],
  );
  const canWrite = hasPermission(permissionScope, "web.servers.files.write");

  const [monacoReady, setMonacoReady] = useState(false);
  const [catalog, setCatalog] = useState<WebserverConfigCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState("");
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [reloadToken, setReloadToken] = useState(0);

  const [file, setFile] = useState<WebserverConfigFile | null>(null);
  const [fileLoading, setFileLoading] = useState(false);
  const [fileError, setFileError] = useState("");
  const [draft, setDraft] = useState("");

  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [saveOutcome, setSaveOutcome] = useState<SaveOutcome | null>(null);

  const selectedEntry = useMemo(
    () => catalog?.items.find((entry) => entry.id === selectedId) ?? null,
    [catalog, selectedId],
  );
  const writable = Boolean(selectedEntry && selectedEntry.writable && canWrite);
  const dirty = Boolean(file) && draft !== file?.content;
  const validationMessage = useMemo(
    () => (selectedEntry ? validateConfigContent(selectedEntry.name, draft) : null),
    [selectedEntry, draft],
  );

  // Monaco is bundled from the local npm package and loaded asynchronously.
  useEffect(() => {
    let active = true;
    void import("./monaco-setup.ts")
      .then(() => {
        if (active) setMonacoReady(true);
      })
      .catch(() => {
        // The surface still works read-only without Monaco (the catalog and
        // status stay available); the editor pane shows the load failure.
      });
    return () => {
      active = false;
    };
  }, []);

  async function reloadCatalog(keepSelection: boolean): Promise<void> {
    setCatalogLoading(true);
    setCatalogError("");
    try {
      const next = await client.listConfigs();
      setCatalog(next);
      const stillListed =
        keepSelection && next.items.some((entry) => entry.id === selectedId);
      if (!stillListed) {
        const preferred =
          next.items.find((entry) => entry.writable) ?? next.items[0] ?? null;
        setSelectedId(preferred?.id ?? "");
      }
    } catch (reason) {
      setCatalogError(messageOf(reason));
    } finally {
      setCatalogLoading(false);
    }
  }

  // Load the catalog once on mount.
  useEffect(() => {
    void reloadCatalog(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client]);

  // Load the selected file; switching away from a dirty draft asks first.
  useEffect(() => {
    if (!selectedId) {
      setFile(null);
      setDraft("");
      return;
    }
    let active = true;
    setFileLoading(true);
    setFileError("");
    setSaveError("");
    setSaveOutcome(null);
    void client
      .readConfig(selectedId)
      .then((next) => {
        if (!active) return;
        setFile(next);
        setDraft(next.content);
      })
      .catch((reason) => {
        if (active) {
          setFileError(messageOf(reason));
          setFile(null);
          setDraft("");
        }
      })
      .finally(() => {
        if (active) setFileLoading(false);
      });
    return () => {
      active = false;
    };
  }, [client, selectedId, reloadToken]);

  async function saveDraft(): Promise<void> {
    if (!selectedEntry || !file || !writable || saving) return;
    if (validationMessage) {
      setSaveError(validationMessage);
      return;
    }
    if (!dirty) return;
    setSaving(true);
    setSaveError("");
    setSaveOutcome(null);
    try {
      const result = await client.updateConfig(
        selectedEntry.id,
        draft,
        file.sha256,
      );
      setSaveOutcome({ result });
      setFile({ ...file, content: draft, sha256: result.sha256, size: result.size });
    } catch (reason) {
      setSaveError(messageOf(reason));
    } finally {
      setSaving(false);
    }
  }

  // Ctrl/Cmd+S saves the open configuration file.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void saveDraft();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedEntry, file, draft, writable, saving, validationMessage]);

  function selectEntry(entry: WebserverConfigEntry): void {
    if (entry.id === selectedId) return;
    if (dirty && !window.confirm("Discard unsaved changes to the open file?")) {
      return;
    }
    setSelectedId(entry.id);
  }

  const filteredGroups = useMemo(() => {
    const query = search.trim().toLowerCase();
    const groups = CONFIG_KIND_ORDER.map((kind) => ({
      kind,
      entries: (catalog?.items ?? []).filter(
        (entry) =>
          entry.kind === kind &&
          (!query ||
            entry.name.toLowerCase().includes(query) ||
            entry.path.toLowerCase().includes(query)),
      ),
    })).filter((group) => group.entries.length > 0);
    return groups;
  }, [catalog, search]);

  return (
    <div className="webserver-config-surface flex h-full flex-col gap-4 p-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold text-slate-900">Server Config</h1>
          <p className="text-sm text-slate-500">
            Edit the deployed configuration online — default config, import
            plane, and module sidecars. Every save is syntax-checked and backed
            up on the server.
          </p>
        </div>
        <div className="flex items-center gap-2">
          {!canWrite && (
            <span className="inline-flex items-center gap-1 rounded-md bg-amber-50 px-2 py-1 text-xs font-medium text-amber-700">
              <Lock size={13} /> Read-only access
            </span>
          )}
          <button
            className="inline-flex items-center gap-1.5 rounded-md border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
            disabled={catalogLoading}
            onClick={() => void reloadCatalog(true)}
            type="button"
          >
            {catalogLoading ? (
              <Loader2 className="animate-spin" size={14} />
            ) : (
              <RefreshCw size={14} />
            )}
            Refresh catalog
          </button>
        </div>
      </header>

      {catalogError && (
        <Banner tone="error" onDismiss={() => setCatalogError("")}>
          {catalogError}
        </Banner>
      )}

      <div className="flex min-h-0 flex-1 gap-4">
        {/* Catalog sidebar */}
        <aside className="flex w-80 shrink-0 flex-col overflow-hidden rounded-lg border border-slate-200 bg-white">
          <div className="border-b border-slate-200 p-2.5">
            <div className="relative">
              <Search
                className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-400"
                size={14}
              />
              <input
                className="w-full rounded-md border border-slate-300 py-1.5 pl-8 pr-2 text-sm text-slate-800 placeholder:text-slate-400 focus:border-blue-500 focus:outline-none"
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Filter configuration files"
                type="search"
                value={search}
              />
            </div>
          </div>
          <nav className="min-h-0 flex-1 overflow-y-auto p-2">
            {catalogLoading && !catalog ? (
              <div className="flex items-center justify-center gap-2 p-8 text-sm text-slate-500">
                <Loader2 className="animate-spin" size={15} /> Loading catalog...
              </div>
            ) : filteredGroups.length === 0 ? (
              <p className="p-6 text-center text-sm text-slate-400">
                No configuration files match.
              </p>
            ) : (
              filteredGroups.map((group) => (
                <section className="mb-3 last:mb-0" key={group.kind}>
                  <h2 className="px-1.5 pb-1 pt-1 text-xs font-semibold uppercase tracking-wide text-slate-500">
                    {CONFIG_KIND_LABEL[group.kind]}
                    <span className="ml-1.5 font-normal normal-case text-slate-400">
                      {group.entries.length}
                    </span>
                  </h2>
                  <p className="px-1.5 pb-1.5 text-xs text-slate-400">
                    {CONFIG_KIND_DESCRIPTION[group.kind]}
                  </p>
                  <ul className="space-y-0.5">
                    {group.entries.map((entry) => (
                      <li key={entry.id}>
                        <EntryButton
                          active={entry.id === selectedId}
                          entry={entry}
                          onSelect={selectEntry}
                        />
                      </li>
                    ))}
                  </ul>
                </section>
              ))
            )}
          </nav>
        </aside>

        {/* Editor pane */}
        <section className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-slate-200 bg-white">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-200 px-3 py-2">
            <div className="flex min-w-0 items-center gap-2 text-sm font-medium text-slate-700">
              <FileCode2 className="shrink-0 text-slate-400" size={15} />
              <span className="truncate" title={selectedEntry?.path}>
                {selectedEntry?.path ?? "No configuration selected"}
              </span>
              {selectedEntry && !selectedEntry.writable && (
                <span className="inline-flex shrink-0 items-center gap-1 rounded-md bg-slate-100 px-1.5 py-0.5 text-xs font-medium text-slate-600">
                  <Lock size={11} /> read-only
                </span>
              )}
              {dirty && (
                <span className="shrink-0 rounded-full bg-amber-100 px-2 py-0.5 text-xs font-medium text-amber-700">
                  unsaved changes
                </span>
              )}
            </div>
            <div className="flex items-center gap-2">
              <button
                className="inline-flex items-center gap-1.5 rounded-md border border-slate-300 bg-white px-2.5 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
                disabled={!selectedId || fileLoading || saving}
                onClick={() => setReloadToken((token) => token + 1)}
                type="button"
              >
                <RefreshCw size={13} /> Reload
              </button>
              <button
                className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={!writable || !dirty || saving || Boolean(validationMessage)}
                onClick={() => void saveDraft()}
                title={validationMessage ?? undefined}
                type="button"
              >
                {saving ? (
                  <Loader2 className="animate-spin" size={13} />
                ) : (
                  <Save size={13} />
                )}
                Save
              </button>
            </div>
          </div>

          <div className="min-h-0 flex-1">
            {fileLoading ? (
              <div className="flex h-full items-center justify-center gap-2 text-sm text-slate-500">
                <Loader2 className="animate-spin" size={16} /> Loading file...
              </div>
            ) : fileError ? (
              <div className="flex h-full items-center justify-center p-6">
                <Banner tone="error" onDismiss={() => setFileError("")}>
                  {fileError}
                </Banner>
              </div>
            ) : !file || !selectedEntry ? (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-sm text-slate-400">
                <FileCog size={28} strokeWidth={1.5} />
                Select a configuration file to view or edit it.
              </div>
            ) : !monacoReady ? (
              <div className="flex h-full items-center justify-center gap-2 text-sm text-slate-500">
                <Loader2 className="animate-spin" size={16} /> Loading editor...
              </div>
            ) : (
              <Editor
                key={selectedEntry.id}
                defaultLanguage={file.language}
                defaultValue={file.content}
                onChange={(value) => setDraft(value ?? "")}
                options={{
                  readOnly: !writable,
                  minimap: { enabled: false },
                  fontSize: 13,
                  lineNumbers: "on",
                  scrollBeyondLastLine: false,
                  automaticLayout: true,
                  tabSize: 2,
                  renderWhitespace: "selection",
                  bracketPairColorization: { enabled: true },
                  fixedOverflowWidgets: true,
                }}
                path={selectedEntry.path}
                theme="vs"
              />
            )}
          </div>

          {(saveError || validationMessage) && (
            <div className="border-t border-red-200 bg-red-50 px-3 py-2">
              <div className="flex items-start gap-2 text-sm text-red-700">
                <CircleAlert size={15} className="mt-0.5 shrink-0" />
                <span>{saveError || validationMessage}</span>
                <button
                  aria-label="Dismiss"
                  className="ml-auto shrink-0"
                  onClick={() => {
                    setSaveError("");
                  }}
                  type="button"
                >
                  <X size={14} />
                </button>
              </div>
            </div>
          )}
          {saveOutcome && !saveError && (
            <div className="border-t border-emerald-200 bg-emerald-50 px-3 py-2">
              <div className="flex items-start gap-2 text-sm text-emerald-700">
                <ShieldCheck size={15} className="mt-0.5 shrink-0" />
                <span>
                  Saved. New digest{" "}
                  <code className="rounded bg-emerald-100 px-1 py-0.5 text-xs">
                    {saveOutcome.result.sha256.slice(0, 12)}…
                  </code>
                  {saveOutcome.result.backupPath
                    ? ` · previous content backed up to ${saveOutcome.result.backupPath}`
                    : ""}
                </span>
                <button
                  aria-label="Dismiss"
                  className="ml-auto shrink-0"
                  onClick={() => setSaveOutcome(null)}
                  type="button"
                >
                  <X size={14} />
                </button>
              </div>
            </div>
          )}

          <footer className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-slate-200 bg-slate-50 px-3 py-1.5 text-xs text-slate-500">
            <span className="inline-flex items-center gap-1">
              <HardDrive size={12} />
              {file ? formatBytes(file.size) : "—"}
            </span>
            <span className="inline-flex items-center gap-1">
              <Puzzle size={12} />
              {file ? file.language : "—"}
            </span>
            <span title={file?.sha256}>
              digest {file ? `${file.sha256.slice(0, 12)}…` : "—"}
            </span>
            <span>updated {formatUnixSeconds(file?.updatedAt)}</span>
            <span className="ml-auto truncate" title={catalog?.configRoot}>
              root {catalog?.configRoot ?? "—"}
            </span>
          </footer>
        </section>
      </div>
    </div>
  );
}

function EntryButton({
  entry,
  active,
  onSelect,
}: {
  entry: WebserverConfigEntry;
  active: boolean;
  onSelect(entry: WebserverConfigEntry): void;
}) {
  return (
    <button
      className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm ${
        active
          ? "bg-blue-50 text-blue-800 ring-1 ring-blue-200"
          : "text-slate-700 hover:bg-slate-100"
      }`}
      onClick={() => onSelect(entry)}
      title={entry.path}
      type="button"
    >
      <FileCode2
        className={`shrink-0 ${active ? "text-blue-500" : "text-slate-400"}`}
        size={14}
      />
      <span className="min-w-0 flex-1 truncate">{entry.name}</span>
      {!entry.writable && <Lock className="shrink-0 text-slate-400" size={12} />}
      <span
        className={`shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium ${KIND_BADGE[entry.kind]}`}
      >
        {CONFIG_KIND_LABEL[entry.kind].replace(" Config", "")}
      </span>
    </button>
  );
}

function Banner({
  tone,
  children,
  onDismiss,
}: {
  tone: "error";
  children: React.ReactNode;
  onDismiss?(): void;
}) {
  return (
    <div
      className={`flex items-start gap-2 rounded-md border p-3 text-sm ${
        tone === "error"
          ? "border-red-200 bg-red-50 text-red-700"
          : "border-slate-200 bg-slate-50 text-slate-700"
      }`}
    >
      <CircleAlert size={16} className="mt-0.5 shrink-0" />
      <span>{children}</span>
      {onDismiss && (
        <button aria-label="Dismiss" className="ml-auto" onClick={onDismiss} type="button">
          <X size={14} />
        </button>
      )}
    </div>
  );
}
