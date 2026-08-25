import type { ApplicationSubmissionInput } from "./application-media.ts";
import type { ApplicationDeploymentSourceMode, WebserverResourceActionContext } from "./types.ts";

function optionalText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

export function hasApplicationSourceInput(
  context: Pick<WebserverResourceActionContext, "file" | "files" | "sourceInputMode" | "sourceRepository">,
): boolean {
  if (context.sourceInputMode === "git") {
    return Boolean(context.sourceRepository?.trim());
  }
  return Boolean(context.files?.length || context.file);
}

export function hasApplicationListingInput(
  context: Pick<WebserverResourceActionContext, "applicationSubmission" | "body">,
): boolean {
  const submission = context.applicationSubmission;
  if (!submission) return false;
  if (
    submission.iconMode === "upload"
    || submission.coverMode === "upload"
    || submission.previewsMode === "replace"
  ) {
    return true;
  }
  const listingFields = [
    "shortDescription",
    "fullDescription",
    "releaseNotes",
    "category",
    "keywords",
    "supportUrl",
    "privacyPolicyUrl",
    "officialWebsiteUrl",
  ] as const;
  return listingFields.some((field) => optionalText(context.body[field]));
}

export function shouldCreateApplicationListing(
  context: Pick<WebserverResourceActionContext, "applicationSubmission" | "body" | "wizardSkips">,
): boolean {
  if (context.wizardSkips?.media) return false;
  return hasApplicationListingInput(context);
}

export function shouldCreateApplicationSource(
  context: Pick<
    WebserverResourceActionContext,
    "file" | "files" | "sourceInputMode" | "sourceRepository" | "wizardSkips"
  >,
): boolean {
  if (context.wizardSkips?.source) return false;
  return hasApplicationSourceInput(context);
}

export function applicationHasSourceVersion(item: Record<string, unknown> | undefined): boolean {
  return item?.hasSourceVersion === true;
}

export function defaultApplicationSubmissionForCreate(): ApplicationSubmissionInput {
  return {
    coverMode: "remove",
    iconMode: "default",
    previewFiles: [],
    previewsMode: "remove",
  };
}

export function summarizeApplicationSourceInput(
  sourceInputMode: ApplicationDeploymentSourceMode,
  files: readonly File[],
  sourceRepository: string,
): string {
  if (sourceInputMode === "git") return sourceRepository.trim();
  if (files.length === 0) return "";
  if (sourceInputMode === "archive") return files[0]?.name ?? "";
  return `${files.length} files`;
}
