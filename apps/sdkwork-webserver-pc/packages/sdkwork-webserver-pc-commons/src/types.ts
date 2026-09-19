export type WebserverPcSurface = "app-console" | "backend-admin";

/**
 * Resource keys the Web Server workspace can route to. `apps`, `domains`, and
 * `certificates` are bridged from sdkwork-deployments; the rest are rendered
 * either by an owning package's surface or by the admin registry. The retired
 * `applications` / `configuration` / `source-versions` / `deployments` /
 * `sites` / `application-*` keys went with the local application lifecycle.
 */
export type WebserverResourceKey =
  | "apps"
  | "domains"
  | "certificates"
  | "nginx"
  | "servers"
  | "servers-explorer"
  | "webserver-config"
  | "diagnostics"
  | "audit"
  | "skills"
  | "mcp"
  | "plugins"
  | "storage-providers"
  | "storage-kinds"
  | "storage-buckets"
  | "storage-bindings"
  | "cluster-overview"
  | "cluster-clusters"
  | "cluster-hosts"
  | "cluster-instances"
  | "cluster-events";

export interface WebserverModuleEntry {
  description: string;
  label: string;
  order: number;
  /**
   * Route segment under the surface base path. Defaults to `resource`; set it
   * when a resource belongs to a module sub-path, e.g. `storage/providers`
   * makes the route `/admin/storage/providers` while the resource key stays
   * `storage-providers`.
   */
  path?: string;
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
  search?: string;
}

export interface WebserverResourceFilter {
  fieldOptions?: readonly WebserverResourceFieldOptionValue[];
  id: string;
  type: "date" | "select" | "text";
}

/**
 * Everything a resource action receives when it runs. The dialog owns the body,
 * the optional upload, the idempotency key, the progress channel, and the
 * selected row; the action owns what to do with them.
 */
export interface WebserverResourceActionContext {
  body: Record<string, unknown>;
  file?: File;
  idempotencyKey?: string;
  onProgress?(progress: number): void;
  selectedItem?: Record<string, unknown>;
  signal?: AbortSignal;
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
  availableWhen?(context: WebserverResourceActionContext): boolean;
  bodyTemplate: Record<string, unknown>;
  dangerous?: boolean;
  dismissibleWhileBusy?: boolean;
  execute(context: WebserverResourceActionContext): Promise<unknown>;
  fieldOptions?: WebserverResourceFieldOptions;
  fieldSelectionLimits?: Readonly<Record<string, number>>;
  id: string;
  label: string;
  /**
   * Server-side option paging for fields whose option set is too large to
   * inline. Pair with `paginatedFields` to name the fields it applies to.
   */
  loadFieldOptionPage?(
    field: string,
    context: WebserverResourceFieldOptionPageContext,
  ): Promise<WebserverResourceFieldOptionPage>;
  loadFieldOptions?(context: WebserverResourceActionContext): Promise<WebserverResourceFieldOptions>;
  multipleFields?: readonly string[];
  paginatedFields?: readonly string[];
  permission?: string;
  readOnlyFields?: readonly string[];
  requiredFields?: readonly string[];
  resolveActionLabelKey?(context: Pick<WebserverResourceActionContext, "selectedItem">): string | undefined;
  resultFields?: readonly string[];
  requiresConfirmation?: boolean;
  requiresFile?: boolean;
  requiresSelection?: boolean;
}

export interface WebserverResourceDataSource {
  actions: readonly WebserverResourceAction[];
  filters?: readonly WebserverResourceFilter[];
  load(query: WebserverResourceQuery): Promise<WebserverResourcePage>;
}

export type WebserverResourceRegistry = Partial<Record<WebserverResourceKey, WebserverResourceDataSource>>;
