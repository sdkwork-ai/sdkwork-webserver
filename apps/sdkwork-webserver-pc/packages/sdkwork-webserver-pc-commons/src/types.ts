export type WebserverPcSurface = "app-console" | "backend-admin";

export type WebserverActionErrorCode =
  | "application-draft-media-failed"
  | "application-draft-source-failed"
  | "application-draft-deployment-failed"
  | "deployment-source-stored";

export class WebserverActionError extends Error {
  constructor(
    readonly code: WebserverActionErrorCode,
    readonly details: Readonly<Record<string, string | number>> = {},
    options?: ErrorOptions,
  ) {
    super(code, options);
    this.name = "WebserverActionError";
  }
}

export type WebserverResourceKey =
  | "applications"
  | "configuration"
  | "source-versions"
  | "domains"
  | "certificates"
  | "deployments"
  | "sites"
  | "application-source-versions"
  | "application-deployments"
  | "nginx"
  | "servers"
  | "servers-explorer"
  | "diagnostics"
  | "audit"
  | "skills"
  | "mcp"
  | "plugins";

export interface WebserverModuleEntry {
  description: string;
  label: string;
  order: number;
  permission: string;
  resource: WebserverResourceKey;
}

export interface WebserverPcModuleDefinition {
  entries: readonly WebserverModuleEntry[];
  id: string;
  label: string;
  surface: WebserverPcSurface;
}

export interface WebserverPageInfo {
  hasMore: boolean;
  /** `offset` (page/page_size) or `cursor` (opaque keyset) per PAGINATION_SPEC;
   *  absent values normalize to `offset` for legacy in-memory fixtures. */
  mode?: "cursor" | "offset";
  page: number;
  pageSize: number;
  total?: number;
  /** Opaque continuation token for cursor-paginated resources (PAGINATION_SPEC). */
  nextCursor?: string;
}

export interface WebserverResourcePage {
  items: readonly Record<string, unknown>[];
  pageInfo: WebserverPageInfo;
}

export interface WebserverResourceQuery {
  /** Opaque keyset continuation token; cursor mode replaces page for the request (PAGINATION_SPEC). */
  cursor?: string;
  filters?: Readonly<Record<string, string>>;
  page: number;
  pageSize: number;
  scopeId?: string;
  search?: string;
}

export interface WebserverResourceFilter {
  fieldOptions?: readonly WebserverResourceFieldOptionValue[];
  id: string;
  type: "date" | "select" | "text";
}

export type ApplicationDeploymentSourceMode = "archive" | "directory" | "git";

export interface ApplicationDeploymentSourceDefaults {
  mode?: ApplicationDeploymentSourceMode;
  repository?: string;
}

export interface WebserverResourceActionContext {
  applicationSubmission?: import("./application-media.ts").ApplicationSubmissionInput;
  body: Record<string, unknown>;
  file?: File;
  files?: readonly File[];
  idempotencyKey?: string;
  onProgress?(progress: number): void;
  selectedItem?: Record<string, unknown>;
  signal?: AbortSignal;
  sourceInputMode?: ApplicationDeploymentSourceMode;
  sourceRepository?: string;
  scopeId?: string;
}

export interface WebserverResourceFieldOption {
  label: string;
  relatedValues?: Readonly<Record<string, number | string>>;
  value: number | string;
}

export type WebserverResourceFieldOptionValue =
  | number
  | string
  | WebserverResourceFieldOption;

export type WebserverResourceFieldOptions = Readonly<
  Record<string, readonly WebserverResourceFieldOptionValue[]>
>;

export interface WebserverResourceFieldOptionPageContext extends WebserverResourceActionContext {
  page: number;
  pageSize: number;
}

export interface WebserverResourceFieldOptionPage {
  options: readonly WebserverResourceFieldOptionValue[];
  pageInfo: WebserverPageInfo;
}

export interface WebserverResourceAction {
  acceptedFileTypes?: string;
  applicationSubmission?: "create" | "update";
  availableWhen?(context: WebserverResourceActionContext): boolean;
  bodyTemplate: Record<string, unknown>;
  dangerous?: boolean;
  dismissibleWhileBusy?: boolean;
  execute(context: WebserverResourceActionContext): Promise<unknown>;
  fieldOptions?: WebserverResourceFieldOptions;
  fieldSelectionLimits?: Readonly<Record<string, number>>;
  id: string;
  label: string;
  loadFieldOptionPage?(
    field: string,
    context: WebserverResourceFieldOptionPageContext,
  ): Promise<WebserverResourceFieldOptionPage>;
  loadFieldOptions?(context: WebserverResourceActionContext): Promise<WebserverResourceFieldOptions>;
  loadSourceInputDefaults?(
    context: WebserverResourceActionContext,
  ): Promise<ApplicationDeploymentSourceDefaults>;
  multipleFields?: readonly string[];
  paginatedFields?: readonly string[];
  permission?: string;
  readOnlyFields?: readonly string[];
  requiredFields?: readonly string[];
  resultFields?: readonly string[];
  requiresConfirmation?: boolean;
  requiresFile?: boolean;
  requiresScope?: boolean;
  requiresSelection?: boolean;
  sourceInput?: "archive-directory-or-git";
}

export interface WebserverResourceDataSource {
  actions: readonly WebserverResourceAction[];
  filters?: readonly WebserverResourceFilter[];
  load(query: WebserverResourceQuery): Promise<WebserverResourcePage>;
  requiresScope?: boolean;
  scopeKind?: "application";
}

export type WebserverResourceRegistry = Partial<Record<WebserverResourceKey, WebserverResourceDataSource>>;
