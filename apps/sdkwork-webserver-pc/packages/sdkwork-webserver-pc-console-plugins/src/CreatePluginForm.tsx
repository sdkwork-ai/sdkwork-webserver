import { useRef, useState, type FormEvent, type ReactNode } from "react";
import type { SdkworkDriveAppClient } from "@sdkwork/webserver-pc-console-core";
import {
  isValidApplicationGitRepositoryUrl,
  normalizeApplicationGitRepositoryUrl,
} from "@sdkwork/webserver-pc-commons";
import { usePluginsT } from "./locale.tsx";
import {
  PluginContributionMultiSelect,
  PluginHostToolMultiSelect,
} from "./PluginToolMultiSelect.tsx";
import {
  createPluginId,
  isValidPluginKey,
  normalizePluginGitRef,
  type PluginRecord,
  type PluginSourceKind,
} from "./plugin-model.ts";
import type { PluginContributionKind, PluginHostToolId } from "./plugin-tool-catalog.ts";
import { uploadPluginArchive } from "./plugin-upload.ts";

function Field({
  hint,
  label,
  children,
}: {
  hint?: ReactNode;
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="skills-console-field">
      <span className="skills-console-field-label">{label}</span>
      {children}
      {hint ? <small className="skills-console-field-hint">{hint}</small> : null}
    </label>
  );
}

export function CreatePluginForm({
  drive,
  existingKeys = [],
  onCancel,
  onSuccess,
}: {
  drive: SdkworkDriveAppClient;
  existingKeys?: readonly string[];
  onCancel?: () => void;
  onSuccess?: (record: PluginRecord) => void | Promise<void>;
}) {
  const t = usePluginsT();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [sourceKind, setSourceKind] = useState<PluginSourceKind>("git");
  const [pluginKey, setPluginKey] = useState("plugin.workspace.sample");
  const [displayName, setDisplayName] = useState("");
  const [summary, setSummary] = useState("");
  const [version, setVersion] = useState("1.0.0");
  const [gitRepository, setGitRepository] = useState("");
  const [gitRef, setGitRef] = useState("main");
  const [artifactRef, setArtifactRef] = useState("");
  const [checksumSha256, setChecksumSha256] = useState("");
  const [archiveFileName, setArchiveFileName] = useState<string | null>(null);
  const [supportedHostTools, setSupportedHostTools] = useState<PluginHostToolId[]>(["cursor"]);
  const [contributedCapabilities, setContributedCapabilities] = useState<PluginContributionKind[]>([]);
  const [hostToolsError, setHostToolsError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const gitLooksValid = gitRepository.trim().length === 0
    || isValidApplicationGitRepositoryUrl(gitRepository);

  function switchSource(next: PluginSourceKind) {
    if (next === sourceKind) return;
    setError(null);
    setSourceKind(next);
    if (next === "git") {
      setArtifactRef("");
      setChecksumSha256("");
      setArchiveFileName(null);
      if (fileInputRef.current) fileInputRef.current.value = "";
    } else {
      setGitRepository("");
      setGitRef("main");
    }
  }

  async function onUpload() {
    const file = fileInputRef.current?.files?.[0];
    if (!file) {
      setError(t("create.error.selectFile"));
      return;
    }
    setUploading(true);
    setError(null);
    try {
      const uploaded = await uploadPluginArchive(drive, file);
      setArtifactRef(uploaded.artifactRef);
      setChecksumSha256(uploaded.checksumSha256);
      setArchiveFileName(file.name);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setUploading(false);
    }
  }

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    const key = pluginKey.trim();
    if (!isValidPluginKey(key)) {
      setError(t("create.error.pluginKey"));
      return;
    }
    if (existingKeys.some((item) => item === key)) {
      setError(t("create.error.duplicateKey", { key }));
      return;
    }
    const now = new Date().toISOString();
    try {
      let record: PluginRecord;
      if (sourceKind === "git") {
        if (!gitRepository.trim()) {
          setError(t("create.error.gitRequired"));
          return;
        }
        const repository = normalizeApplicationGitRepositoryUrl(gitRepository);
        record = {
          id: createPluginId(),
          pluginKey: key,
          displayName: displayName.trim() || key,
          summary: summary.trim(),
          version: version.trim() || "1.0.0",
          supportedHostTools: [...supportedHostTools],
          contributedCapabilities: [...contributedCapabilities],
          sourceKind: "git",
          gitRepository: repository,
          gitRef: normalizePluginGitRef(gitRef),
          artifactRef: null,
          checksumSha256: null,
          archiveFileName: null,
          status: "active",
          createdAt: now,
          updatedAt: now,
        };
      } else {
        if (!artifactRef.startsWith("drive://")) {
          setError(t("create.error.needArtifact"));
          return;
        }
        record = {
          id: createPluginId(),
          pluginKey: key,
          displayName: displayName.trim() || key,
          summary: summary.trim(),
          version: version.trim() || "1.0.0",
          supportedHostTools: [...supportedHostTools],
          contributedCapabilities: [...contributedCapabilities],
          sourceKind: "archive",
          gitRepository: null,
          gitRef: null,
          artifactRef,
          checksumSha256,
          archiveFileName,
          status: "active",
          createdAt: now,
          updatedAt: now,
        };
      }
      setSubmitting(true);
      await onSuccess?.(record);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSubmitting(false);
    }
  }

  const canSubmit = !submitting
    && !uploading
    && supportedHostTools.length > 0
    && (sourceKind === "git"
      ? isValidApplicationGitRepositoryUrl(gitRepository)
      : artifactRef.startsWith("drive://"));

  return (
    <form className="skills-console-form" onSubmit={onSubmit}>
      {error ? (
        <p className="skills-console-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="plugin-source-toggle" role="group" aria-label={t("create.source.git")}>
        <button
          type="button"
          aria-pressed={sourceKind === "git"}
          onClick={() => switchSource("git")}
        >
          {t("create.source.git")}
        </button>
        <button
          type="button"
          aria-pressed={sourceKind === "archive"}
          onClick={() => switchSource("archive")}
        >
          {t("create.source.archive")}
        </button>
      </div>
      <Field label={t("create.field.pluginKey")}>
        <input
          value={pluginKey}
          onChange={(event) => setPluginKey(event.target.value)}
          placeholder={t("create.placeholder.pluginKey")}
          required
        />
      </Field>
      <Field label={t("create.field.displayName")}>
        <input
          value={displayName}
          onChange={(event) => setDisplayName(event.target.value)}
          placeholder={t("create.placeholder.displayName")}
          required
        />
      </Field>
      <Field label={t("create.field.summary")}>
        <textarea
          value={summary}
          onChange={(event) => setSummary(event.target.value)}
          placeholder={t("create.placeholder.summary")}
          rows={3}
        />
      </Field>
      <Field label={t("create.field.version")}>
        <input
          value={version}
          onChange={(event) => setVersion(event.target.value)}
          placeholder={t("create.placeholder.version")}
          required
        />
      </Field>
      <Field label={t("create.field.hostTools")} hint={t("create.hint.hostTools")}>
        <PluginHostToolMultiSelect
          value={supportedHostTools}
          onChange={(next) => {
            setSupportedHostTools(next);
            if (next.length > 0) setHostToolsError(null);
          }}
          error={hostToolsError}
        />
      </Field>
      <Field label={t("create.field.capabilities")} hint={t("create.hint.capabilities")}>
        <PluginContributionMultiSelect
          value={contributedCapabilities}
          onChange={setContributedCapabilities}
        />
      </Field>
      {sourceKind === "git" ? (
        <>
          <Field
            label={t("create.field.gitRepository")}
            hint={
              gitRepository.trim() && !gitLooksValid
                ? <span className="plugin-field-warning">{t("create.hint.gitInvalid")}</span>
                : t("create.hint.gitHttps")
            }
          >
            <input
              value={gitRepository}
              onChange={(event) => setGitRepository(event.target.value)}
              placeholder={t("create.placeholder.gitRepository")}
              required
            />
          </Field>
          <Field label={t("create.field.gitRef")}>
            <input
              value={gitRef}
              onChange={(event) => setGitRef(event.target.value)}
              placeholder={t("create.placeholder.gitRef")}
            />
          </Field>
        </>
      ) : (
        <Field label={t("create.field.archive")}>
          <input
            ref={fileInputRef}
            type="file"
            accept=".zip,.tar,.gz,.tgz,application/zip,application/gzip"
          />
          <button type="button" onClick={() => void onUpload()} disabled={uploading}>
            {uploading ? t("create.uploading") : t("create.upload")}
          </button>
          {archiveFileName ? <p>{t("create.uploadedFile", { name: archiveFileName })}</p> : null}
          <input
            value={artifactRef}
            readOnly
            placeholder={t("create.field.artifactRef")}
          />
        </Field>
      )}
      <div className="sdkwork-surface-drawer-form-actions">
        {onCancel ? (
          <button type="button" onClick={onCancel}>
            {t("dialog.cancel")}
          </button>
        ) : null}
        <button type="submit" className="skills-console-primary" disabled={!canSubmit}>
          {t("create.submit")}
        </button>
      </div>
    </form>
  );
}
