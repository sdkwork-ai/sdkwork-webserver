import { useRef, useState, type FormEvent } from "react";
import type { SdkworkDriveAppClient } from "@sdkwork/webserver-pc-console-core";
import {
  isValidApplicationGitRepositoryUrl,
  normalizeApplicationGitRepositoryUrl,
} from "@sdkwork/webserver-pc-commons";
import { usePluginsT } from "./locale.tsx";
import { ArchiveIcon, CheckIcon, GitBranchIcon, UploadIcon } from "./plugin-icons.tsx";
import { ConsoleField, ConsoleFormSection } from "./plugin-form-kit.tsx";
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
  const [supportedHostTools, setSupportedHostTools] = useState<PluginHostToolId[]>(["workbuddy"]);
  const [contributedCapabilities, setContributedCapabilities] = useState<PluginContributionKind[]>([]);
  const [hostToolsError, setHostToolsError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const gitLooksValid = gitRepository.trim().length === 0
    || isValidApplicationGitRepositoryUrl(gitRepository);
  const keyTrimmed = pluginKey.trim();
  const keyHint = !keyTrimmed
    ? t("create.hint.pluginKey")
    : isValidPluginKey(keyTrimmed)
      ? (
          <span className="plugin-hint-valid">
            <CheckIcon size={12} strokeWidth={2.4} />
            {t("create.hint.pluginKeyValid")}
          </span>
        )
      : <span className="plugin-field-warning">{t("create.error.pluginKey")}</span>;

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
    const key = pluginKey.trim();
    if (!isValidPluginKey(key)) {
      setError(t("create.error.pluginKey"));
      return;
    }
    if (existingKeys.some((item) => item === key)) {
      setError(t("create.error.duplicateKey", { key }));
      return;
    }
    if (supportedHostTools.length === 0) {
      setHostToolsError(t("create.error.hostToolsRequired"));
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
    <form className="skills-console-form plugin-console-form" onSubmit={onSubmit}>
      {error ? (
        <p className="skills-console-error" role="alert">
          {error}
        </p>
      ) : null}

      <ConsoleFormSection title={t("create.section.source")}>
        <div className="plugin-source-toggle" role="group" aria-label={t("create.source.git")}>
          <button
            type="button"
            aria-pressed={sourceKind === "git"}
            onClick={() => switchSource("git")}
          >
            <GitBranchIcon size={15} />
            <span>{t("create.source.git")}</span>
          </button>
          <button
            type="button"
            aria-pressed={sourceKind === "archive"}
            onClick={() => switchSource("archive")}
          >
            <ArchiveIcon size={15} />
            <span>{t("create.source.archive")}</span>
          </button>
        </div>
        {sourceKind === "git" ? (
          <>
            <ConsoleField
              htmlFor="plugin-create-git-repository"
              label={t("create.field.gitRepository")}
              required
              hint={
                gitRepository.trim() && !gitLooksValid
                  ? <span className="plugin-field-warning">{t("create.hint.gitInvalid")}</span>
                  : t("create.hint.gitHttps")
              }
            >
              <input
                id="plugin-create-git-repository"
                value={gitRepository}
                onChange={(event) => setGitRepository(event.target.value)}
                placeholder={t("create.placeholder.gitRepository")}
                required
              />
            </ConsoleField>
            <ConsoleField
              htmlFor="plugin-create-git-ref"
              label={t("create.field.gitRef")}
              optionalLabel={t("create.field.optional")}
            >
              <input
                id="plugin-create-git-ref"
                value={gitRef}
                onChange={(event) => setGitRef(event.target.value)}
                placeholder={t("create.placeholder.gitRef")}
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
              <UploadIcon size={18} className="plugin-upload-zone-icon" />
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
                  htmlFor="plugin-create-artifact-ref"
                  label={t("create.upload.artifact")}
                >
                  <input
                    id="plugin-create-artifact-ref"
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

      <ConsoleFormSection title={t("create.section.identity")}>
        <ConsoleField
          htmlFor="plugin-create-key"
          label={t("create.field.pluginKey")}
          required
          hint={keyHint}
        >
          <input
            id="plugin-create-key"
            value={pluginKey}
            onChange={(event) => setPluginKey(event.target.value)}
            placeholder={t("create.placeholder.pluginKey")}
            required
          />
        </ConsoleField>
        <ConsoleField htmlFor="plugin-create-display-name" label={t("create.field.displayName")} required>
          <input
            id="plugin-create-display-name"
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
            placeholder={t("create.placeholder.displayName")}
            required
          />
        </ConsoleField>
        <ConsoleField
          htmlFor="plugin-create-summary"
          label={t("create.field.summary")}
          optionalLabel={t("create.field.optional")}
        >
          <textarea
            id="plugin-create-summary"
            value={summary}
            onChange={(event) => setSummary(event.target.value)}
            placeholder={t("create.placeholder.summary")}
            rows={3}
          />
        </ConsoleField>
        <ConsoleField htmlFor="plugin-create-version" label={t("create.field.version")} required>
          <input
            id="plugin-create-version"
            value={version}
            onChange={(event) => setVersion(event.target.value)}
            placeholder={t("create.placeholder.version")}
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
