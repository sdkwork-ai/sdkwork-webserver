import { useRef, useState, type FormEvent, type ReactNode } from "react";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import {
  isValidApplicationGitRepositoryUrl,
  normalizeApplicationGitRepositoryUrl,
} from "@sdkwork/webserver-pc-commons";
import { usePluginsT } from "./locale.tsx";
import {
  PluginContributionMultiSelect,
  PluginHostToolMultiSelect,
} from "./PluginToolMultiSelect.tsx";
import { normalizePluginGitRef, type PluginRecord } from "./plugin-model.ts";
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

export function EditPluginForm({
  drive,
  plugin,
  onCancel,
  onSuccess,
}: {
  drive: SdkworkDriveAppClient;
  plugin: PluginRecord;
  onCancel?: () => void;
  onSuccess?: (record: PluginRecord) => void | Promise<void>;
}) {
  const t = usePluginsT();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [displayName, setDisplayName] = useState(plugin.displayName);
  const [summary, setSummary] = useState(plugin.summary);
  const [version, setVersion] = useState(plugin.version);
  const [gitRepository, setGitRepository] = useState(plugin.gitRepository ?? "");
  const [gitRef, setGitRef] = useState(plugin.gitRef ?? "main");
  const [artifactRef, setArtifactRef] = useState(plugin.artifactRef ?? "");
  const [checksumSha256, setChecksumSha256] = useState(plugin.checksumSha256 ?? "");
  const [archiveFileName, setArchiveFileName] = useState(plugin.archiveFileName);
  const [supportedHostTools, setSupportedHostTools] = useState<PluginHostToolId[]>([...plugin.supportedHostTools]);
  const [contributedCapabilities, setContributedCapabilities] = useState<PluginContributionKind[]>([
    ...plugin.contributedCapabilities,
  ]);
  const [hostToolsError, setHostToolsError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const gitLooksValid = plugin.sourceKind !== "git"
    || gitRepository.trim().length === 0
    || isValidApplicationGitRepositoryUrl(gitRepository);

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
    setHostToolsError(null);
    if (supportedHostTools.length === 0) {
      setHostToolsError(t("create.error.hostToolsRequired"));
      return;
    }
    try {
      const next: PluginRecord = {
        ...plugin,
        displayName: displayName.trim() || plugin.pluginKey,
        summary: summary.trim(),
        version: version.trim() || plugin.version,
        supportedHostTools: [...supportedHostTools],
        contributedCapabilities: [...contributedCapabilities],
        updatedAt: new Date().toISOString(),
        gitRepository: plugin.sourceKind === "git"
          ? normalizeApplicationGitRepositoryUrl(gitRepository)
          : plugin.gitRepository,
        gitRef: plugin.sourceKind === "git" ? normalizePluginGitRef(gitRef) : plugin.gitRef,
        artifactRef: plugin.sourceKind === "archive" ? artifactRef : plugin.artifactRef,
        checksumSha256: plugin.sourceKind === "archive" ? checksumSha256 : plugin.checksumSha256,
        archiveFileName: plugin.sourceKind === "archive" ? archiveFileName : plugin.archiveFileName,
      };
      if (plugin.sourceKind === "archive" && !next.artifactRef?.startsWith("drive://")) {
        setError(t("create.error.needArtifact"));
        return;
      }
      setSubmitting(true);
      await onSuccess?.(next);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSubmitting(false);
    }
  }

  const canSubmit = !submitting
    && !uploading
    && supportedHostTools.length > 0
    && (plugin.sourceKind === "git"
      ? isValidApplicationGitRepositoryUrl(gitRepository)
      : artifactRef.startsWith("drive://"));

  return (
    <form className="skills-console-form" onSubmit={onSubmit}>
      {error ? (
        <p className="skills-console-error" role="alert">
          {error}
        </p>
      ) : null}
      <Field label={t("create.field.pluginKey")}>
        <input value={plugin.pluginKey} readOnly aria-readonly="true" />
      </Field>
      <Field label={t("create.field.displayName")}>
        <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} required />
      </Field>
      <Field label={t("create.field.summary")}>
        <textarea value={summary} onChange={(event) => setSummary(event.target.value)} rows={3} />
      </Field>
      <Field label={t("create.field.version")}>
        <input value={version} onChange={(event) => setVersion(event.target.value)} required />
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
      {plugin.sourceKind === "git" ? (
        <>
          <Field
            label={t("create.field.gitRepository")}
            hint={
              gitRepository.trim() && !gitLooksValid
                ? <span className="plugin-field-warning">{t("create.hint.gitInvalid")}</span>
                : t("create.hint.gitHttps")
            }
          >
            <input value={gitRepository} onChange={(event) => setGitRepository(event.target.value)} required />
          </Field>
          <Field label={t("create.field.gitRef")}>
            <input value={gitRef} onChange={(event) => setGitRef(event.target.value)} />
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
          <input value={artifactRef} readOnly placeholder={t("create.field.artifactRef")} />
        </Field>
      )}
      <div className="sdkwork-surface-drawer-form-actions">
        {onCancel ? (
          <button type="button" onClick={onCancel}>
            {t("dialog.cancel")}
          </button>
        ) : null}
        <button type="submit" className="skills-console-primary" disabled={!canSubmit}>
          {t("edit.save")}
        </button>
      </div>
    </form>
  );
}
