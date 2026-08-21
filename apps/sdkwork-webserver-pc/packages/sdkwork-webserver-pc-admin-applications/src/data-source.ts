import type { WebserverAdminSdkClient } from "@sdkwork/webserver-pc-admin-core";
import {
  normalizeWebserverPage,
  normalizeApplicationGitRepositoryUrl,
  applicationStoreListing,
  resolveApplicationStoreListing,
  WebserverActionError,
  type ApplicationMediaStorage,
  type ApplicationSourceStorage,
  type ApplicationStoreListingInput,
  type PreparedApplicationSource,
  type StoredApplicationSource,
  type WebserverResourceAction,
  type WebserverResourceActionContext,
  type WebserverResourceDataSource,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";

type DeploymentCreateRequest = Parameters<
  WebserverAdminSdkClient["applicationDeployment"]["applications"]["deployments"]["create"]
>[1];
type SourceVersionCreateRequest = Parameters<
  WebserverAdminSdkClient["applicationSourceVersion"]["applications"]["sourceVersions"]["create"]
>[1];
type ApplicationUpdateRequest = Parameters<WebserverAdminSdkClient["application"]["update"]>[1];

export function createWebserverAdminApplicationRegistry(
  client: WebserverAdminSdkClient,
  sourceStorage: ApplicationSourceStorage,
  mediaStorage: ApplicationMediaStorage,
): WebserverResourceRegistry {
  return {
    applications: source(
      (query) => client.application.list({ page: query.page, pageSize: query.pageSize, keyword: query.search }),
      [
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
          (context) => createApplicationWithInitialVersion(client, sourceStorage, mediaStorage, context),
          {
            applicationSubmission: "create",
            fieldOptions: {
              applicationType: ["WEB", "API"],
              siteType: [1, 2, 3, 4, 5, 6],
              environment: ["production", "staging", "test", "development"],
            },
            permission: "web.sites.write",
            requiredFields: ["name", "versionTag"],
            sourceInput: "archive-directory-or-git",
          },
        ),
        action(
          "update",
          "Update application",
          {
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
          },
          (context) => updateApplicationListing(client, mediaStorage, context),
          { applicationSubmission: "update", requiresSelection: true, permission: "web.sites.write" },
        ),
        action(
          "update-source",
          "Update code",
          { versionTag: "" },
          (context) => storeApplicationSourceVersion(client, sourceStorage, context),
          {
            loadSourceInputDefaults: async (context) => {
              const versions = await client.applicationSourceVersion.applications.sourceVersions.list(
                selectedId(context),
                { pageSize: 1 },
              );
              const latest = versions.items[0];
              return latest?.sourceType === "GIT" && latest.sourceRef?.trim()
                ? { mode: "git", repository: latest.sourceRef }
                : {};
            },
            permission: "web.sites.write",
            requiredFields: ["versionTag"],
            requiresSelection: true,
            sourceInput: "archive-directory-or-git",
          },
        ),
        action(
          "publish",
          "Publish application",
          {
            deployType: 1,
            sourceVersionId: "",
            environment: "production",
            versionTag: "",
          },
          (context) => deployApplication(client, context),
          {
            requiresConfirmation: true,
            requiresSelection: true,
            fieldOptions: {
              deployType: [1],
              sourceVersionId: [],
              environment: ["production", "staging", "test", "development"],
            },
            loadFieldOptions: async (context) => {
              const versions = await client.applicationSourceVersion.applications.sourceVersions.list(
                selectedId(context),
                { pageSize: 100 },
              );
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
            permission: "web.sites.write",
            readOnlyFields: ["versionTag"],
            requiredFields: ["sourceVersionId", "versionTag"],
          },
        ),
        action(
          "activate",
          "Activate application",
          {},
          (context) => client.application.activate(selectedId(context), idempotencyParams(context)),
          {
            availableWhen: ({ selectedItem }) => Number(selectedItem?.status) !== 1,
            requiresSelection: true,
            permission: "web.sites.write",
          },
        ),
        action(
          "pause",
          "Disable application",
          {},
          (context) => client.application.pause(selectedId(context), idempotencyParams(context)),
          {
            availableWhen: ({ selectedItem }) => Number(selectedItem?.status) === 1,
            dangerous: true,
            requiresSelection: true,
            permission: "web.sites.write",
          },
        ),
        action(
          "delete",
          "Delete application",
          {},
          (context) => client.application.delete(selectedId(context), idempotencyParams(context)),
          {
            availableWhen: ({ selectedItem }) => Number(selectedItem?.status) !== 1,
            dangerous: true,
            requiresSelection: true,
            permission: "web.sites.write",
          },
        ),
      ],
    ),
    "application-source-versions": applicationSource(
      (query) => client.applicationSourceVersion.applications.sourceVersions.list(
        requiredApplicationId(query.scopeId),
        { cursor: query.cursor, pageSize: query.pageSize },
      ),
      [
        action(
          "create",
          "Save source version",
          { versionTag: "" },
          (context) => storeApplicationSourceVersion(client, sourceStorage, context),
          {
            permission: "web.sites.write",
            requiredFields: ["versionTag"],
            requiresScope: true,
            sourceInput: "archive-directory-or-git",
          },
        ),
      ],
    ),
    "application-deployments": applicationSource(
      (query) => client.applicationDeployment.applications.deployments.list(requiredApplicationId(query.scopeId), { cursor: query.cursor, pageSize: query.pageSize }),
      [
        action(
          "deploy",
          "Create deployment command",
          {
            deployType: 1,
            sourceVersionId: "",
            environment: "production",
            versionTag: "",
          },
          (context) => deployApplication(client, context),
          {
            requiresConfirmation: true,
            requiresScope: true,
            fieldOptions: {
              deployType: [1],
              sourceVersionId: [],
              environment: ["production", "staging", "test", "development"],
            },
            loadFieldOptions: async (context) => {
              const versions = await client.applicationSourceVersion.applications.sourceVersions.list(
                requiredApplicationId(context.scopeId),
                { pageSize: 100 },
              );
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
            permission: "web.sites.write",
            requiredFields: ["sourceVersionId", "versionTag"],
          },
        ),
        action(
          "rollback",
          "Restore this version",
          {},
          (context) => client.applicationDeployment.applications.deployments.rollback(
            requiredApplicationId(context.scopeId),
            selectedId(context),
            idempotencyParams(context),
          ),
          {
            availableWhen: ({ selectedItem }) => Number(selectedItem?.status) === 2,
            requiresConfirmation: true,
            requiresScope: true,
            requiresSelection: true,
            permission: "web.sites.write",
          },
        ),
      ],
    ),
  };
}

function source(
  load: WebserverResourceDataSource["load"] extends (query: infer Query) => Promise<unknown> ? (query: Query) => Promise<unknown> : never,
  actions: readonly WebserverResourceAction[],
): WebserverResourceDataSource {
  return { actions, async load(query) { return normalizeWebserverPage(await load(query)); } };
}

function applicationSource(load: Parameters<typeof source>[0], actions: readonly WebserverResourceAction[]): WebserverResourceDataSource {
  return { ...source(load, actions), requiresScope: true, scopeKind: "application" };
}

function action(
  id: string,
  label: string,
  bodyTemplate: Record<string, unknown>,
  execute: WebserverResourceAction["execute"],
  options: Omit<WebserverResourceAction, "bodyTemplate" | "execute" | "id" | "label"> = {},
): WebserverResourceAction {
  return { id, label, bodyTemplate, execute, ...options };
}

function requiredApplicationId(value: string | undefined): string {
  if (!value?.trim()) throw new Error("Application ID is required");
  return value.trim();
}

function selectedId(context: WebserverResourceActionContext): string {
  const value = context.selectedItem?.id;
  if (typeof value !== "string" && typeof value !== "number") throw new Error("Selected resource ID is unavailable");
  return String(value);
}

function idempotencyParams(context: WebserverResourceActionContext): { idempotencyKey: string } {
  const idempotencyKey = context.idempotencyKey?.trim();
  if (!idempotencyKey) throw new Error("Idempotency key is required");
  return { idempotencyKey };
}

async function createApplicationWithInitialVersion(
  client: WebserverAdminSdkClient,
  sourceStorage: ApplicationSourceStorage,
  mediaStorage: ApplicationMediaStorage,
  context: WebserverResourceActionContext,
): Promise<unknown> {
  const resolvedApplicationType = applicationType(context.body.applicationType);
  const resolvedSiteType = siteType(context.body.siteType);
  const applicationRequest = {
    name: requiredText(context.body.name, "Application name"),
    description: optionalText(context.body.description),
    appKind: appKindFromCarrier(resolvedApplicationType, resolvedSiteType),
    runtimeConfig: deploymentConfiguration(context.body),
  };
  const idempotency = idempotencyParams(context);
  const prepared = context.sourceInputMode === "git"
    ? undefined
    : await prepareSource(sourceStorage, context, 0, 14);
  const application = await client.application.create(applicationRequest, idempotency);
  const applicationId = application.id?.trim();
  if (!applicationId) throw new Error("The created application did not return an ID");
  context.onProgress?.(16);
  let storeListing: ApplicationStoreListingInput;
  try {
    storeListing = await resolveApplicationStoreListing({
      applicationId,
      applicationName: applicationRequest.name,
      body: context.body,
      mediaStorage,
      onProgress: (progress) => context.onProgress?.(scaleProgress(progress, 16, 46)),
      signal: context.signal,
      submission: requiredApplicationSubmission(context),
    });
    await client.application.update(
      applicationId,
      { storeListing: sdkStoreListing(storeListing) },
      idempotency,
    );
    context.onProgress?.(48);
  } catch (error) {
    throw new WebserverActionError(
      "application-draft-media-failed",
      { applicationId },
      { cause: error },
    );
  }
  let sourceVersionId: string;
  if (prepared) {
    try {
      const stored = await storeSource(sourceStorage, applicationId, prepared, context, 48, 86);
      const sourceVersion = await client.applicationSourceVersion.applications.sourceVersions.create(
        applicationId,
        sourceVersionRequest(context, stored, prepared.inputMode),
        idempotency,
      );
      sourceVersionId = sourceVersion.id;
      context.onProgress?.(92);
    } catch (error) {
      throw new WebserverActionError(
        "application-draft-source-failed",
        { applicationId },
        { cause: error },
      );
    }
  } else {
    try {
      const sourceVersion = await client.applicationSourceVersion.applications.sourceVersions.gitImport.create(
        applicationId,
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
        { applicationId },
        { cause: error },
      );
    }
  }
  try {
    const deployment = await client.applicationDeployment.applications.deployments.create(
      applicationId,
      deploymentRequest(context, sourceVersionId),
      idempotency,
    );
    context.onProgress?.(100);
    return { ...deployment, applicationId };
  } catch (error) {
    throw new WebserverActionError(
      "application-draft-deployment-failed",
      { applicationId },
      { cause: error },
    );
  }
}

async function updateApplicationListing(
  client: WebserverAdminSdkClient,
  mediaStorage: ApplicationMediaStorage,
  context: WebserverResourceActionContext,
): Promise<unknown> {
  const applicationId = selectedId(context);
  const applicationName = requiredText(context.body.name, "Application name");
  const storeListing = await resolveApplicationStoreListing({
    applicationId,
    applicationName,
    body: context.body,
    current: applicationStoreListing(context.selectedItem?.storeListing),
    mediaStorage,
    onProgress: context.onProgress,
    signal: context.signal,
    submission: requiredApplicationSubmission(context),
  });
  context.onProgress?.(96);
  const result = await client.application.update(
    applicationId,
    updateApplicationRequest(context.body, storeListing),
    idempotencyParams(context),
  );
  context.onProgress?.(100);
  return result;
}

async function deployApplication(
  client: WebserverAdminSdkClient,
  context: WebserverResourceActionContext,
): Promise<unknown> {
  const applicationId = context.scopeId?.trim()
    ? requiredApplicationId(context.scopeId)
    : selectedId(context);
  const idempotency = idempotencyParams(context);
  const request = deploymentRequest(
    context,
    requiredText(context.body.sourceVersionId, "Source version"),
  );
  let deployment: unknown;
  try {
    deployment = await client.applicationDeployment.applications.deployments.create(
      applicationId,
      request,
      idempotency,
    );
  } catch (error) {
    throw new WebserverActionError("deployment-source-stored", {}, { cause: error });
  }
  context.onProgress?.(100);
  return deployment;
}

async function storeApplicationSourceVersion(
  client: WebserverAdminSdkClient,
  sourceStorage: ApplicationSourceStorage,
  context: WebserverResourceActionContext,
): Promise<unknown> {
  const applicationId = context.scopeId?.trim()
    ? requiredApplicationId(context.scopeId)
    : selectedId(context);
  const idempotency = idempotencyParams(context);
  if (context.sourceInputMode === "git") {
    context.onProgress?.(8);
    const sourceVersion = await client.applicationSourceVersion.applications.sourceVersions.gitImport.create(
      applicationId,
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
  const stored = await storeSource(sourceStorage, applicationId, prepared, context, 24, 88);
  const sourceVersion = await client.applicationSourceVersion.applications.sourceVersions.create(
    applicationId,
    sourceVersionRequest(context, stored, prepared.inputMode),
    idempotency,
  );
  context.onProgress?.(100);
  return sourceVersion;
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

function sourceVersionRequest(
  context: WebserverResourceActionContext,
  stored: StoredApplicationSource,
  inputMode: "archive" | "directory",
): SourceVersionCreateRequest {
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
): DeploymentCreateRequest {
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
  throw new Error("Deployment method is invalid");
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
  throw new Error("Deployment environment is invalid");
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
  if (parsed === 1 || parsed === 2 || parsed === 3 || parsed === 4 || parsed === 5 || parsed === 6) {
    return parsed;
  }
  throw new Error("Runtime type is invalid");
}

function requiredText(value: unknown, label: string): string {
  const text = optionalText(value);
  if (!text) throw new Error(`${label} is required`);
  return text;
}

function optionalText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function updateApplicationRequest(
  body: Readonly<Record<string, unknown>>,
  storeListing: ApplicationStoreListingInput,
): ApplicationUpdateRequest {
  const name = boundedOptionalText(body.name, "Application name", 100, false);
  const description = boundedOptionalText(body.description, "Description", 500, true);
  return { name, description, storeListing: sdkStoreListing(storeListing) };
}

function sdkStoreListing(
  storeListing: ApplicationStoreListingInput,
): NonNullable<ApplicationUpdateRequest["storeListing"]> {
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

function sslProvider(value: unknown): "letsencrypt" | "custom" | "none" | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  if (value === "letsencrypt" || value === "custom" || value === "none") return value;
  throw new Error("TLS provider is invalid");
}
