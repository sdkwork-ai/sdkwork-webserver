import { useRef, useState, type FormEvent } from "react";
import type { SdkworkDriveAppClient } from "@sdkwork/webserver-pc-console-core";
import {
  isValidApplicationGitRepositoryUrl,
  normalizeApplicationGitRepositoryUrl,
} from "@sdkwork/webserver-pc-commons";
import { usePluginsT } from "./locale.tsx";
import { CheckIcon } from "./plugin-icons.tsx";
import { ConsoleField, ConsoleFormSection } from "./plugin-form-kit.tsx";
import {
  PluginContributionMultiSelect,
  PluginHostToolMultiSelect,
} from "./PluginToolMultiSelect.tsx";
import { normalizePluginGitRef, type PluginRecord } from "./plugin-model.ts";
import type { PluginContributionKind, PluginHostToolId } from "./plugin-tool-catalog.ts";
import { uploadPluginArchive } from "./plugin-upload.ts";

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
      // Allow re-picking the same file after a failed upload.
      if (fileInputRef.current) fileInputRef.current.value = "";
      setArchiveFileName(null);
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
    <form className="skills-console-form plugin-console-form" onSubmit={onSubmit}>
      {error ? (
        <p className="skills-console-error" role="alert">
          {error}
        </p>
      ) : null}

      <ConsoleFormSection title={t("create.section.identity")}>
        <div className="skills-console-field">
          <span className="skills-console-field-label">{t("create.field.pluginKey")}</span>
          <p className="plugin-key-static">{plugin.pluginKey}</p>
        </div>
        <ConsoleField htmlFor="plugin-edit-display-name" label={t("create.field.displayName")} required>
          <input
            id="plugin-edit-display-name"
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
            required
          />
        </ConsoleField>
        <ConsoleField
          htmlFor="plugin-edit-summary"
          label={t("create.field.summary")}
          optionalLabel={t("create.field.optional")}
        >
          <textarea
            id="plugin-edit-summary"
            value={summary}
            onChange={(event) => setSummary(event.target.value)}
            rows={3}
          />
        </ConsoleField>
        <ConsoleField htmlFor="plugin-edit-version" label={t("create.field.version")} required>
          <input
            id="plugin-edit-version"
            value={version}
            onChange={(event) => setVersion(event.target.value)}
            required
          />
        </ConsoleField>
      </ConsoleFormSection>

      <ConsoleFormSection title={t("create.section.compatibility")}>
        <div className="skills-console-field">
          <span className="skills-console-field-label">
            {t("create.field.hostTools")}
            <span className="skills-console-field-required" aria-hidden="true">*</span>
          </span>
          <PluginHostToolMultiSelect
            value={supportedHostTools}
            onChange={(next) => {
              setSupportedHostTools(next);
              if (next.length > 0) setHostToolsError(null);
            }}
            error={hostToolsError}
          />
          <small className="skills-console-field-hint">{t("create.hint.hostTools")}</small>
        </div>
        <div className="skills-console-field">
          <span className="skills-console-field-label">
            {t("create.field.capabilities")}
            <span className="skills-console-field-optional">{t("create.field.optional")}</span>
          </span>
          <PluginContributionMultiSelect
            value={contributedCapabilities}
            onChange={setContributedCapabilities}
          />
          <small className="skills-console-field-hint">{t("create.hint.capabilities")}</small>
        </div>
      </ConsoleFormSection>

      <ConsoleFormSection title={t("create.section.source")}>
        {plugin.sourceKind === "git" ? (
          <>
            <ConsoleField
              htmlFor="plugin-edit-git-repository"
              label={t("create.field.gitRepository")}
              required
              hint={
                gitRepository.trim() && !gitLooksValid
                  ? <span className="plugin-field-warning">{t("create.hint.gitInvalid")}</span>
                  : t("create.hint.gitHttps")
              }
            >
              <input
                id="plugin-edit-git-repository"
                value={gitRepository}
                onChange={(event) => setGitRepository(event.target.value)}
                required
              />
            </ConsoleField>
            <ConsoleField
              htmlFor="plugin-edit-git-ref"
              label={t("create.field.gitRef")}
              optionalLabel={t("create.field.optional")}
            >
              <input
                id="plugin-edit-git-ref"
                value={gitRef}
                onChange={(event) => setGitRef(event.target.value)}
              />
            </ConsoleField>
          </>
        ) : (
          <div className="skills-console-field">
            <span className="skills-console-field-label">{t("create.field.archive")}</span>
            <label className={`plugin-upload-zone${uploading ? " plugin-upload-zone--busy" : ""}`}>
              <input
                ref={fileInputRef}
                type="file"
                accept=".zip,.tar,.gz,.tgz,application/zip,application/gzip"
                onChange={() => void onUpload()}
              />
              <span className="plugin-upload-zone-title">
                {uploading ? t("create.uploading") : t("create.upload.dropzone")}
              </span>
              <small className="plugin-upload-zone-hint">{t("create.upload.hint")}</small>
            </label>
            {archiveFileName ? (
              <>
                <p className="plugin-upload-result">
                  <CheckIcon size={13} strokeWidth={2.4} />
                  <span>{t("create.uploadedFile", { name: archiveFileName })}</span>
                </p>
                <ConsoleField
                  htmlFor="plugin-edit-artifact-ref"
                  label={t("create.upload.artifact")}
                >
                  <input
                    id="plugin-edit-artifact-ref"
                    value={artifactRef}
                    readOnly
                    className="plugin-artifact-ref"
                  />
                </ConsoleField>
              </>
            ) : null}
          </div>
        )}
      </ConsoleFormSection>

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
