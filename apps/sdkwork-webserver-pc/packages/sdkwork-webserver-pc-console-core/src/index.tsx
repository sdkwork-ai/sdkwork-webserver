import {
  applicationStoreListing,
  createDefaultApplicationIcon,
  normalizeWebserverPage,
  normalizeApplicationGitRepositoryUrl,
  prepareApplicationSourcePackage,
  resolveApplicationStoreListing,
  validateApplicationMediaFile,
  validateApplicationArchiveEntries,
  WebserverActionError,
  type ApplicationMediaStorage,
  type ApplicationStoreListingInput,
  type ApplicationSourceStorage,
  type PreparedApplicationSource,
  type StoredApplicationSource,
  type WebserverResourceAction,
  type WebserverResourceActionContext,
  type WebserverResourceDataSource,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";
import { createDriveAppClient, type SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createClient as createWebAppClient,
  type CreateDeploymentRequest,
  type CreateEnvVariableRequest,
  type CreateHealthCheckRequest,
  type CreateSourceVersionRequest,
  type SdkworkAppClient as SdkworkWebAppClient,
  type UpdateApplicationRequest,
} from "@sdkwork/web-app-sdk";
import { createContext, useContext, type ReactNode } from "react";

export type WebserverConsoleSdkClient = SdkworkWebAppClient;

export interface WebserverConsoleSdkClients {
  drive: SdkworkDriveAppClient;
  web: SdkworkWebAppClient;
}

const Context = createContext<WebserverConsoleSdkClients | null>(null);

export function createWebserverConsoleSdkClient(baseUrl: string, tokenManager: AuthTokenManager): WebserverConsoleSdkClient { return createWebAppClient({ baseUrl, authMode: "dual-token", platform: "pc", tokenManager }); }
export function createWebserverConsoleSdkClients(baseUrls: { driveAppApiBaseUrl: string; webAppApiBaseUrl: string }, tokenManager: AuthTokenManager): WebserverConsoleSdkClients { return { drive: createDriveAppClient({ baseUrl: baseUrls.driveAppApiBaseUrl, authMode: "dual-token", platform: "pc", tokenManager }), web: createWebserverConsoleSdkClient(baseUrls.webAppApiBaseUrl, tokenManager) }; }
export function WebserverConsoleSdkProvider({ children, clients }: { children: ReactNode; clients: WebserverConsoleSdkClients }) { return <Context.Provider value={clients}>{children}</Context.Provider>; }
export function useWebserverConsoleSdk(): WebserverConsoleSdkClients { const clients = useContext(Context); if (!clients) throw new Error("WebserverConsoleSdkProvider is required"); return clients; }

export function createApplicationSourceStorage(
  driveClient: SdkworkDriveAppClient,
): ApplicationSourceStorage {
  return {
    prepare: prepareApplicationSourcePackage,
    async store(request): Promise<StoredApplicationSource> {
      const { archive, archiveHash } = request.package;
      request.signal?.throwIfAborted();
      request.onProgress?.(0);
      const taskId = await applicationSourceUploadTaskId(request.applicationId, archiveHash);
      request.signal?.throwIfAborted();
      const uploaded = await driveClient.uploader.uploadArchive({
        appResourceId: request.applicationId,
        appResourceType: "web.application.source",
        checksumSha256Hex: `sha256:${archiveHash}`,
        contentType: archive.type || "application/zip",
        file: archive,
        fileFingerprint: archiveHash,
        onProgress: (progress) => {
          const ratio = progress.totalBytes > 0
            ? progress.uploadedBytes / progress.totalBytes
            : 0;
          request.onProgress?.(Math.round(ratio * 88));
        },
        originalFileName: archive.name,
        scene: "application-source",
        source: "sdkwork-webserver-pc",
        signal: request.signal,
        taskId,
      });
      request.signal?.throwIfAborted();
      const { nodeId, spaceId } = uploaded.uploadSession;
      if (!nodeId || !spaceId) throw new Error("Drive did not return the source archive identity");
      request.onProgress?.(90);
      const archiveEntries = await driveClient.drive.archiveEntries.list(nodeId);
      request.signal?.throwIfAborted();
      const validated = validateApplicationArchiveEntries(archiveEntries.items, {
        excludeDriveSanitizedVcs: request.package.inputMode === "archive",
        hasMore: archiveEntries.pageInfo.hasMore,
      });
      if (
        request.package.inputMode === "directory"
        && (validated.sourceFileCount !== request.package.sourceFileCount
          || validated.uncompressedSize !== request.package.uncompressedSize)
      ) {
        throw new Error("Drive archive inspection did not match the prepared source package");
      }
      request.onProgress?.(94);
      const extracted = await driveClient.drive.archiveEntries.extract(nodeId, {
        entryPaths: [...validated.entryPaths],
      });
      request.signal?.throwIfAborted();
      const extractedCount = parseExtractedCount(extracted.extractedCount);
      if (extractedCount !== validated.sourceFileCount) {
        throw new Error("Drive did not extract every validated application source file");
      }
      request.onProgress?.(100);
      return {
        archiveDriveUri: `drive://spaces/${spaceId}/nodes/${nodeId}`,
        archiveHash,
        archiveSize: String(archive.size),
        extractedCount: String(extractedCount),
        configSnapshot: {
          appConfigDetected: validated.entryPaths.includes("sdkwork.app.config.json"),
          appConfigPath: "sdkwork.app.config.json",
          deploymentConfigDetected: validated.entryPaths.includes("etc/sdkwork.deployment.config.json"),
          deploymentConfigPath: "etc/sdkwork.deployment.config.json",
        },
      };
    },
  };
}

export function createApplicationMediaStorage(
  driveClient: SdkworkDriveAppClient,
): ApplicationMediaStorage {
  return {
    createDefaultIcon: createDefaultApplicationIcon,
    async store(request) {
      request.signal?.throwIfAborted();
      request.onProgress?.(0);
      const dimensions = await validateApplicationMediaFile(request.role, request.file);
      request.signal?.throwIfAborted();
      const checksum = await sha256Hex(request.file);
      const fileName = applicationMediaFileName(request.file.name, request.role, request.sequence);
      const identity = `${request.role}:${request.sequence ?? 0}:${checksum}`;
      const taskId = await applicationUploadTaskId("media", request.applicationId, identity);
      request.signal?.throwIfAborted();
      const uploaded = await driveClient.uploader.uploadImage({
        appResourceId: request.applicationId,
        appResourceType: `web.application.media.${request.role}`,
        checksumSha256Hex: `sha256:${checksum}`,
        contentType: request.file.type,
        file: request.file,
        fileFingerprint: checksum,
        onProgress: (progress) => {
          const ratio = progress.totalBytes > 0
            ? progress.uploadedBytes / progress.totalBytes
            : 0;
          request.onProgress?.(Math.round(ratio * 100));
        },
        originalFileName: fileName,
        scene: "application-store-listing",
        source: "sdkwork-webserver-pc",
        signal: request.signal,
        taskId,
      });
      request.signal?.throwIfAborted();
      const { nodeId, spaceId } = uploaded.uploadSession;
      if (!nodeId || !spaceId) throw new Error("Drive did not return the application media identity");
      request.onProgress?.(100);
      return {
        id: nodeId,
        kind: "image",
        source: "drive",
        uri: `drive://spaces/${spaceId}/nodes/${nodeId}`,
        fileName,
        mimeType: request.file.type,
        sizeBytes: String(request.file.size),
        checksum: { algorithm: "sha256", value: checksum },
        width: dimensions.width,
        height: dimensions.height,
        altText: request.altText,
        metadata: { drive: { nodeId, spaceId } },
      };
    },
  };
}

async function applicationSourceUploadTaskId(applicationId: string, archiveHash: string): Promise<string> {
  return applicationUploadTaskId("source", applicationId, archiveHash);
}

async function applicationUploadTaskId(kind: string, applicationId: string, identity: string): Promise<string> {
  const material = UTF8_ENCODER.encode(`${applicationId}\0${identity}`);
  const digest = await globalThis.crypto.subtle.digest("SHA-256", material);
  const hash = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `web-${kind}-${hash}`;
}

async function sha256Hex(file: File): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", await file.arrayBuffer());
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function applicationMediaFileName(
  value: string,
  role: "icon" | "cover" | "preview",
  sequence = 0,
): string {
  const normalized = value
    .trim()
    .replace(/[\u0000-\u001F\u007F]/g, "-")
    .slice(0, 512);
  return normalized || `application-${role}-${sequence}.png`;
}

function parseExtractedCount(value: string): number {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) {
    throw new Error("Drive returned an invalid extracted file count");
  }
  const count = Number(value);
  if (!Number.isSafeInteger(count)) {
    throw new Error("Drive returned an unsupported extracted file count");
  }
  return count;
}

const UTF8_ENCODER = new TextEncoder();

export function createWebserverConsoleRegistry(
  clients: WebserverConsoleSdkClients,
  sourceStorage: ApplicationSourceStorage = createApplicationSourceStorage(clients.drive),
  mediaStorage: ApplicationMediaStorage = createApplicationMediaStorage(clients.drive),
): WebserverResourceRegistry {
  const client = clients.web;
  return {
    applications: source((query) => client.application.list({ page: query.page, pageSize: query.pageSize, keyword: query.search }), [
      action(
        "create",
        "Create application",
        {
          name: "",
          description: "",
          applicationType: "WEB",
          siteType: 1,
          environment: "production",
          versionTag: "v1.0.0",
          sourceVersionRetentionLimit: 5,
          appConfigPath: "sdkwork.app.config.json",
          deploymentConfigPath: "etc/sdkwork.deployment.config.json",
          publicRoot: "dist",
          spaFallback: "index.html",
          shortDescription: "",
          fullDescription: "",
          releaseNotes: "",
          category: "",
          keywords: "",
          supportUrl: "",
          privacyPolicyUrl: "",
          officialWebsiteUrl: "",
        },
        (context) => createApplicationWithInitialVersion(clients, sourceStorage, mediaStorage, context),
        {
          applicationSubmission: "create",
          fieldOptions: {
            applicationType: ["WEB", "API"],
            siteType: [1, 2, 3, 4, 5, 6],
            environment: ["production", "staging", "test", "development"],
          },
          permission: "web.applications.write",
          requiredFields: ["name", "versionTag"],
          sourceInput: "archive-directory-or-git",
        },
      ),
      action("update", "Update", {
        name: "",
        description: "",
        shortDescription: "",
        fullDescription: "",
        releaseNotes: "",
        category: "",
        keywords: "",
        supportUrl: "",
        privacyPolicyUrl: "",
        officialWebsiteUrl: "",
      }, (context) => updateApplicationListing(clients, mediaStorage, context), { applicationSubmission: "update", permission: "web.applications.write", selection: true }),
      action("update-source", "Update code", { versionTag: "" }, (context) => storeApplicationSourceVersion(clients, sourceStorage, context), {
        loadSourceInputDefaults: async (context) => {
          const versions = await client.sourceVersion.applications.sourceVersions.list(
            selectedId(context, "siteId"),
            { pageSize: 1 },
          );
          const latest = versions.items[0];
          return latest?.sourceType === "GIT" && latest.sourceRef?.trim()
            ? { mode: "git", repository: latest.sourceRef }
            : {};
        },
        permission: "web.applications.write",
        requiredFields: ["versionTag"],
        selection: true,
        sourceInput: "archive-directory-or-git",
      }),
      action("publish", "Publish", { deployType: 1, sourceVersionId: "", environment: "production", versionTag: "" }, (context) => deployApplication(clients, context), {
        confirmation: true,
        fieldOptions: { deployType: [1], sourceVersionId: [], environment: ["production", "staging", "test", "development"] },
        loadFieldOptions: async (context) => {
          const versions = await client.sourceVersion.applications.sourceVersions.list(selectedId(context, "siteId"), { pageSize: 100 });
          return {
            sourceVersionId: versions.items
              .filter((version) => version.status === 1 && version.retained)
              .map((version) => ({
                label: `${version.versionTag} · ${version.sourceType}`,
                relatedValues: { versionTag: version.versionTag },
                value: version.id,
              })),
          };
        },
        permission: "web.applications.write",
        readOnlyFields: ["versionTag"],
        requiredFields: ["sourceVersionId", "versionTag"],
        selection: true,
      }),
      action("activate", "Activate", {}, (context) => client.application.activate(selectedId(context, "siteId")), { availableWhen: ({ selectedItem }) => Number(selectedItem?.status) !== 1, permission: "web.applications.write", selection: true }),
      action("pause", "Disable", {}, (context) => client.application.pause(selectedId(context, "siteId")), { availableWhen: ({ selectedItem }) => Number(selectedItem?.status) === 1, dangerous: true, permission: "web.applications.write", selection: true }),
      action("delete", "Delete", {}, (context) => client.application.delete(selectedId(context, "siteId")), { availableWhen: ({ selectedItem }) => Number(selectedItem?.status) !== 1, dangerous: true, permission: "web.applications.write", selection: true }),
    ]),
    configuration: scopedSource((query) => client.envVariable.applications.envVariables.list(requiredScope(query.scopeId)), [
      action("create-variable", "Add variable", { key: "", value: "", environment: "production", isSecret: false }, async (context) => client.envVariable.applications.envVariables.create(requiredScope(context.scopeId), createEnvVariableRequest(context.body), idempotencyParams(context)), { permission: "web.applications.write", scope: true }),
      action("create-check", "Add health check", { checkType: 1, checkUrl: "/health", checkInterval: 30, timeoutMs: 5_000, retryCount: 3 }, async (context) => client.monitor.applications.healthChecks.create(requiredScope(context.scopeId), createHealthCheckRequest(context.body), idempotencyParams(context)), { fieldOptions: { checkType: [1, 2, 3] }, permission: "web.applications.write", scope: true }),
    ]),
    "source-versions": scopedSource(
      (query) => client.sourceVersion.applications.sourceVersions.list(requiredScope(query.scopeId), { cursor: query.cursor, pageSize: query.pageSize }),
      [
        action(
          "create",
          "Save source version",
          { versionTag: "" },
          (context) => storeApplicationSourceVersion(clients, sourceStorage, context),
          {
            permission: "web.applications.write",
            requiredFields: ["versionTag"],
            scope: true,
            sourceInput: "archive-directory-or-git",
          },
        ),
      ],
    ),
    deployments: scopedSource((query) => client.deployment.applications.deployments.list(requiredScope(query.scopeId), { cursor: query.cursor, pageSize: query.pageSize }), [
      action("deploy", "Deploy", { deployType: 1, sourceVersionId: "", environment: "production", versionTag: "" }, (context) => deployApplication(clients, context), {
        confirmation: true,
        fieldOptions: { deployType: [1], sourceVersionId: [], environment: ["production", "staging", "test", "development"] },
        loadFieldOptions: async (context) => {
          const versions = await client.sourceVersion.applications.sourceVersions.list(requiredScope(context.scopeId), { pageSize: 100 });
          return {
            sourceVersionId: versions.items
              .filter((version) => version.status === 1 && version.retained)
              .map((version) => ({
                label: `${version.versionTag} · ${version.sourceType}`,
                relatedValues: { versionTag: version.versionTag },
                value: version.id,
              })),
          };
        },
        permission: "web.applications.write",
        requiredFields: ["sourceVersionId", "versionTag"],
        scope: true,
      }),
      action("rollback", "Restore this version", {}, (context) => client.deployment.applications.deployments.rollback(requiredScope(context.scopeId), selectedId(context, "deploymentId"), idempotencyParams(context)), {
        availableWhen: (context) => Number(context.selectedItem?.status) === 2,
        confirmation: true,
        permission: "web.applications.write",
        scope: true,
        selection: true,
      }),
    ]),
  };
}

function source(load: WebserverResourceDataSource["load"] extends (query: infer Q) => Promise<unknown> ? (query: Q) => Promise<unknown> : never, actions: readonly WebserverResourceAction[]): WebserverResourceDataSource { return { actions, async load(query) { return normalizeWebserverPage(await load(query)); } }; }
function scopedSource(load: Parameters<typeof source>[0], actions: readonly WebserverResourceAction[]): WebserverResourceDataSource { return { ...source(load, actions), requiresScope: true }; }
type WebserverConsoleActionOptions = Omit<
  WebserverResourceAction,
  "bodyTemplate" | "execute" | "id" | "label" | "requiresConfirmation" | "requiresFile" | "requiresScope" | "requiresSelection"
> & {
  confirmation?: boolean;
  file?: boolean;
  scope?: boolean;
  selection?: boolean;
};

function action(
  id: string,
  label: string,
  bodyTemplate: Record<string, unknown>,
  execute: WebserverResourceAction["execute"],
  options: WebserverConsoleActionOptions = {},
): WebserverResourceAction {
  const { confirmation, file, scope, selection, ...actionOptions } = options;
  return {
    id,
    label,
    bodyTemplate,
    execute,
    ...actionOptions,
    requiresConfirmation: confirmation,
    requiresFile: file,
    requiresScope: scope,
    requiresSelection: selection,
  };
}
function selectedId(context: WebserverResourceActionContext, key: string): string { const value = context.selectedItem?.[key] ?? context.selectedItem?.id; if (typeof value !== "string" && typeof value !== "number") throw new Error(`${key} is unavailable`); return String(value); }
function requiredScope(value: string | undefined): string { if (!value?.trim()) throw new Error("Site ID is required"); return value.trim(); }
function idempotencyParams(context: WebserverResourceActionContext): { idempotencyKey: string } { const idempotencyKey = context.idempotencyKey?.trim(); if (!idempotencyKey) throw new Error("Idempotency key is required"); return { idempotencyKey }; }

async function deployApplication(clients: WebserverConsoleSdkClients, context: WebserverResourceActionContext): Promise<unknown> {
  const siteId = context.scopeId?.trim()
    ? requiredScope(context.scopeId)
    : selectedId(context, "siteId");
  const idempotency = idempotencyParams(context);
  const request = deploymentRequest(
    context,
    requiredText(context.body.sourceVersionId, "Source version"),
  );
  let deployment: unknown;
  try {
    deployment = await clients.web.deployment.applications.deployments.create(
      siteId,
      request,
      idempotency,
    );
  } catch (error) {
    throw new WebserverActionError("deployment-source-stored", {}, { cause: error });
  }
  context.onProgress?.(100);
  return deployment;
}

async function createApplicationWithInitialVersion(
  clients: WebserverConsoleSdkClients,
  sourceStorage: ApplicationSourceStorage,
  mediaStorage: ApplicationMediaStorage,
  context: WebserverResourceActionContext,
): Promise<unknown> {
  const resolvedApplicationType = applicationType(context.body.applicationType);
  const resolvedSiteType = siteType(context.body.siteType);
  const siteRequest = {
    name: requiredText(context.body.name, "Application name"),
    description: optionalText(context.body.description),
    appKind: appKindFromCarrier(resolvedApplicationType, resolvedSiteType),
    runtimeConfig: deploymentConfiguration(context.body),
  };
  const idempotency = idempotencyParams(context);
  const prepared = context.sourceInputMode === "git"
    ? undefined
    : await prepareSource(sourceStorage, context, 0, 14);
  const site = await clients.web.application.create(siteRequest, idempotency);
  const siteId = site.id?.trim();
  if (!siteId) throw new Error("The created application did not return an ID");
  context.onProgress?.(16);
  try {
    const storeListing = await resolveApplicationStoreListing({
      applicationId: siteId,
      applicationName: siteRequest.name,
      body: context.body,
      mediaStorage,
      onProgress: (progress) => context.onProgress?.(scaleProgress(progress, 16, 46)),
      signal: context.signal,
      submission: requiredApplicationSubmission(context),
    });
    await clients.web.application.update(
      siteId,
      { storeListing: sdkStoreListing(storeListing) },
      idempotency,
    );
    context.onProgress?.(48);
  } catch (error) {
    throw new WebserverActionError(
      "application-draft-media-failed",
      { applicationId: siteId },
      { cause: error },
    );
  }
  let sourceVersionId: string;
  if (prepared) {
    try {
      const stored = await storeSource(sourceStorage, siteId, prepared, context, 48, 86);
      const sourceVersion = await clients.web.sourceVersion.applications.sourceVersions.create(
        siteId,
        sourceVersionRequest(context, stored, prepared.inputMode),
        idempotency,
      );
      sourceVersionId = sourceVersion.id;
      context.onProgress?.(92);
    } catch (error) {
      throw new WebserverActionError(
        "application-draft-source-failed",
        { applicationId: siteId },
        { cause: error },
      );
    }
  } else {
    try {
      const sourceVersion = await clients.web.sourceVersion.applications.sourceVersions.gitImport.create(
        siteId,
        {
          repositoryUrl: normalizeApplicationGitRepositoryUrl(context.sourceRepository),
          versionTag: requiredText(context.body.versionTag, "Version"),
        },
        idempotency,
      );
      sourceVersionId = sourceVersion.id;
      context.onProgress?.(92);
    } catch (error) {
      throw new WebserverActionError(
        "application-draft-source-failed",
        { applicationId: siteId },
        { cause: error },
      );
    }
  }
  try {
    const deployment = await clients.web.deployment.applications.deployments.create(
      siteId,
      deploymentRequest(context, sourceVersionId),
      idempotency,
    );
    context.onProgress?.(100);
    return { ...deployment, applicationId: siteId };
  } catch (error) {
    throw new WebserverActionError(
      "application-draft-deployment-failed",
      { applicationId: siteId },
      { cause: error },
    );
  }
}

async function updateApplicationListing(
  clients: WebserverConsoleSdkClients,
  mediaStorage: ApplicationMediaStorage,
  context: WebserverResourceActionContext,
): Promise<unknown> {
  const siteId = selectedId(context, "siteId");
  const applicationName = requiredText(context.body.name, "Application name");
  const storeListing = await resolveApplicationStoreListing({
    applicationId: siteId,
    applicationName,
    body: context.body,
    current: applicationStoreListing(context.selectedItem?.storeListing),
    mediaStorage,
    onProgress: context.onProgress,
    signal: context.signal,
    submission: requiredApplicationSubmission(context),
  });
  context.onProgress?.(96);
  const result = await clients.web.application.update(
    siteId,
    updateSiteRequest(context.body, storeListing),
    idempotencyParams(context),
  );
  context.onProgress?.(100);
  return result;
}

async function prepareSource(
  sourceStorage: ApplicationSourceStorage,
  context: WebserverResourceActionContext,
  start: number,
  end: number,
): Promise<PreparedApplicationSource> {
  return sourceStorage.prepare({
    files: sourceFiles(context),
    mode: packageSourceMode(context),
    onProgress: (progress) => context.onProgress?.(scaleProgress(progress, start, end)),
    signal: context.signal,
  });
}

async function storeSource(
  sourceStorage: ApplicationSourceStorage,
  applicationId: string,
  prepared: PreparedApplicationSource,
  context: WebserverResourceActionContext,
  start: number,
  end: number,
): Promise<StoredApplicationSource> {
  return sourceStorage.store({
    applicationId,
    package: prepared,
    onProgress: (progress) => context.onProgress?.(scaleProgress(progress, start, end)),
    signal: context.signal,
  });
}

async function storeApplicationSourceVersion(
  clients: WebserverConsoleSdkClients,
  sourceStorage: ApplicationSourceStorage,
  context: WebserverResourceActionContext,
): Promise<unknown> {
  const siteId = context.scopeId?.trim()
    ? requiredScope(context.scopeId)
    : selectedId(context, "siteId");
  const idempotency = idempotencyParams(context);
  if (context.sourceInputMode === "git") {
    context.onProgress?.(8);
    const sourceVersion = await clients.web.sourceVersion.applications.sourceVersions.gitImport.create(
      siteId,
      {
        repositoryUrl: normalizeApplicationGitRepositoryUrl(context.sourceRepository),
        versionTag: requiredText(context.body.versionTag, "Version"),
      },
      idempotency,
    );
    context.onProgress?.(100);
    return sourceVersion;
  }
  const prepared = await prepareSource(sourceStorage, context, 0, 24);
  const stored = await storeSource(sourceStorage, siteId, prepared, context, 24, 88);
  const sourceVersion = await clients.web.sourceVersion.applications.sourceVersions.create(
    siteId,
    sourceVersionRequest(context, stored, prepared.inputMode),
    idempotency,
  );
  context.onProgress?.(100);
  return sourceVersion;
}

function sourceVersionRequest(
  context: WebserverResourceActionContext,
  stored: StoredApplicationSource,
  inputMode: "archive" | "directory",
): CreateSourceVersionRequest {
  return {
    artifactDriveUri: stored.archiveDriveUri,
    artifactSize: stored.archiveSize,
    artifactHash: stored.archiveHash,
    configSnapshot: stored.configSnapshot,
    sourceType: inputMode === "directory" ? "DIRECTORY" : "ARCHIVE",
    versionTag: requiredText(context.body.versionTag, "Version"),
  };
}

function deploymentRequest(
  context: WebserverResourceActionContext,
  sourceVersionId: string,
): CreateDeploymentRequest {
  return {
    deployType: deploymentType(context.body.deployType),
    environment: deploymentEnvironment(context.body.environment),
    sourceVersionId,
    versionTag: requiredText(context.body.versionTag, "Version"),
  };
}

function deploymentConfiguration(body: Readonly<Record<string, unknown>>): Record<string, unknown> {
  const retentionLimit = Number(body.sourceVersionRetentionLimit ?? 5);
  if (!Number.isInteger(retentionLimit) || retentionLimit < 1 || retentionLimit > 50) {
    throw new Error("sourceVersionRetentionLimit must be between 1 and 50");
  }
  return {
    appConfigPath: requiredText(body.appConfigPath, "Application config path"),
    deploymentConfigPath: requiredText(body.deploymentConfigPath, "Deployment config path"),
    publicRoot: requiredText(body.publicRoot, "Public root"),
    sourceVersionRetentionLimit: retentionLimit,
    spaFallback: requiredText(body.spaFallback, "SPA fallback"),
  };
}

function deploymentType(value: unknown): 1 | 2 | 3 | 4 {
  const normalized = Number(value ?? 1);
  if (normalized === 1 || normalized === 2 || normalized === 3 || normalized === 4) {
    return normalized;
  }
  throw new Error("deployType is invalid");
}

function deploymentEnvironment(
  value: unknown,
): "development" | "test" | "staging" | "production" | undefined {
  const normalized = optionalText(value);
  if (normalized === undefined) return undefined;
  if (
    normalized === "development"
    || normalized === "test"
    || normalized === "staging"
    || normalized === "production"
  ) {
    return normalized;
  }
  throw new Error("environment is invalid");
}

function sourceFiles(context: WebserverResourceActionContext): readonly File[] {
  if (context.files?.length) return context.files;
  if (context.file) return [context.file];
  throw new Error("Application source is required");
}

function packageSourceMode(context: WebserverResourceActionContext): "archive" | "directory" {
  const mode = context.sourceInputMode ?? "archive";
  if (mode === "git") throw new Error("Git repositories do not use application source packages");
  return mode;
}

function scaleProgress(progress: number, start: number, end: number): number {
  return start + Math.round((Math.max(0, Math.min(100, progress)) / 100) * (end - start));
}

function applicationType(value: unknown): "WEB" | "API" {
  if (value === "WEB" || value === "API") return value;
  throw new Error("Application type is invalid");
}

function siteType(value: unknown): 1 | 2 | 3 | 4 | 5 | 6 {
  const parsed = Number(value);
  if ([1, 2, 3, 4, 5, 6].includes(parsed)) return parsed as 1 | 2 | 3 | 4 | 5 | 6;
  throw new Error("Runtime type is invalid");
}

/** Map legacy console carrier fields onto CreateApplicationRequest.appKind. */
function appKindFromCarrier(
  applicationTypeValue: "WEB" | "API",
  siteTypeValue: 1 | 2 | 3 | 4 | 5 | 6,
): "STATIC_WEB" | "SPA_WEB" | "API_SERVICE" {
  if (applicationTypeValue === "API") return "API_SERVICE";
  if (siteTypeValue === 1) return "STATIC_WEB";
  return "SPA_WEB";
}

function requiredText(value: unknown, label: string): string {
  const text = optionalText(value);
  if (!text) throw new Error(`${label} is required`);
  return text;
}

function optionalText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function updateSiteRequest(
  body: Readonly<Record<string, unknown>>,
  storeListing: ApplicationStoreListingInput,
): UpdateApplicationRequest {
  const name = boundedOptionalText(body.name, "Application name", 100, false);
  const description = boundedOptionalText(body.description, "Description", 500, true);
  return { name, description, storeListing: sdkStoreListing(storeListing) };
}

function sdkStoreListing(
  storeListing: ApplicationStoreListingInput,
): NonNullable<UpdateApplicationRequest["storeListing"]> {
  return {
    ...storeListing,
    keywords: storeListing.keywords ? [...storeListing.keywords] : undefined,
    previews: storeListing.previews ? [...storeListing.previews] : undefined,
  };
}

function requiredApplicationSubmission(
  context: WebserverResourceActionContext,
): NonNullable<WebserverResourceActionContext["applicationSubmission"]> {
  if (!context.applicationSubmission) throw new Error("Application store submission is required");
  return context.applicationSubmission;
}

function createEnvVariableRequest(body: Readonly<Record<string, unknown>>): CreateEnvVariableRequest {
  if (typeof body.value !== "string") throw new Error("Variable value is invalid");
  if (UTF8_ENCODER.encode(body.value).byteLength > MAX_ENV_VALUE_BYTES) {
    throw new Error("Variable value must not exceed 64 KiB");
  }
  const key = boundedRequiredText(body.key, "Variable key", 200);
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
    throw new Error("Variable key is invalid");
  }
  return {
    key,
    value: body.value,
    environment: environment(body.environment),
    isSecret: optionalBoolean(body.isSecret, "Secret variable"),
  };
}

function createHealthCheckRequest(body: Readonly<Record<string, unknown>>): CreateHealthCheckRequest {
  const checkInterval = boundedInteger(body.checkInterval, "Check interval", 5, 86_400);
  const timeoutMs = boundedInteger(body.timeoutMs, "Timeout", 100, 60_000);
  if (timeoutMs > checkInterval * 1_000) {
    throw new Error("Timeout must not exceed the check interval");
  }
  return {
    checkType: healthCheckType(body.checkType),
    checkUrl: boundedRequiredText(body.checkUrl, "Health check target", 2_000),
    checkInterval,
    timeoutMs,
    retryCount: boundedInteger(body.retryCount, "Retry count", 0, 10),
  };
}

function healthCheckType(value: unknown): 1 | 2 | 3 {
  const parsed = Number(value);
  if (parsed === 1 || parsed === 2 || parsed === 3) return parsed;
  throw new Error("Health check type is invalid");
}

function environment(value: unknown): "development" | "test" | "staging" | "production" | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  if (value === "development" || value === "test" || value === "staging" || value === "production") {
    return value;
  }
  throw new Error("Environment is invalid");
}

function boundedRequiredText(value: unknown, label: string, maximum: number): string {
  const text = boundedOptionalText(value, label, maximum, false);
  if (!text) throw new Error(`${label} is required`);
  return text;
}

function boundedOptionalText(
  value: unknown,
  label: string,
  maximum: number,
  allowEmpty: boolean,
): string | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") throw new Error(`${label} is invalid`);
  const text = value.trim();
  if ((!allowEmpty && !text) || text.length > maximum || /[\u0000-\u001f\u007f]/.test(text)) {
    throw new Error(`${label} is invalid`);
  }
  return text;
}

function optionalBoolean(value: unknown, label: string): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "boolean") throw new Error(`${label} is invalid`);
  return value;
}

function boundedInteger(value: unknown, label: string, minimum: number, maximum: number): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new Error(`${label} must be between ${minimum} and ${maximum}`);
  }
  return parsed;
}

const MAX_ENV_VALUE_BYTES = 64 * 1024;
