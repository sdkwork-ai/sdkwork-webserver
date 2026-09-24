export type WebserverPcSurface = "app-console" | "backend-admin";

/**
 * Resource keys the Web Server workspace can route to. `apps`, `domains`, and
 * `certificates` are bridged from sdkwork-deployments, and `cloud-accounts` from
 * sdkwork-iam (the IAM module owns cloud providers and their credentials, so the
 * workspace only mounts its console capability package); the rest are rendered
 * either by an owning package's surface or by the admin registry. The retired
 * `applications` / `configuration` / `source-versions` / `deployments` /
 * `sites` / `application-*` keys went with the local application lifecycle.
 *
 * `nginx` / `servers` / `servers-explorer` / `webserver-config` are **declared
 * but not mounted**. Their capability packages still ship these module
 * declarations, so the keys have to stay resolvable for those packages to
 * type-check; what went away is every consumer of them in this host. The edge is
 * operated as a cluster, so one machine's nginx runtime, a hand-kept inventory of
 * machines, and node-scoped browsing or online editing over a deployment tree are
 * the cluster plane's business now (`cluster-hosts` / `cluster-instances` and
 * their liveness) rather than a menu of their own. No menu entry, no
 * `resourceRenderers` entry, no admin-registry source, no column plan or
 * preferred field order, no icon branch, and no i18n label remains for them — a
 * re-added entry with no page behind it is the failure this shape prevents.
 *
 * `diagnostics` is **declared but not mounted** too, for a different reason: it
 * was never a page of its own but a view the admin registry built from the
 * nginx status call, and the operations surface stopped mounting it, so its
 * registry source, its `ResourceIcon` branch, and its `resource.diagnostics.*`
 * and `action.diagnostics.reload` i18n all went with it. Only the key stays, so
 * the capability package that still declares it keeps type-checking; nothing
 * routes to it. Retiring that package is what would let the key go.
 */
export type WebserverResourceKey =
  | "apps"
  | "domains"
  | "certificates"
  | "cloud-accounts"
  | "nginx"
  | "servers"
  | "servers-explorer"
  | "webserver-config"
  // Declared but not mounted — see the note on this type. Kept only so the
  // capability package that still declares the key keeps type-checking.
  | "diagnostics"
  | "audit"
  | "skills"
  | "mcp"
  | "plugins"
  | "plugin-categories"
  // The virtual machine (VM) instances this account provisioned. A per-user
  // resource owned by sdkwork-sandbox and read over its own app-api face; the
  // tenant-wide inventory is a different plane and is not this key.
  | "sandbox-instances"
  | "storage-providers"
  | "storage-kinds"
  | "storage-buckets"
  | "storage-bindings"
  | "cluster-overview"
  | "cluster-clusters"
  | "cluster-hosts"
  | "cluster-instances"
  | "cluster-events"
  // `dashboard` is the leading overview of the workspace and stays unclaimed by
  // any sidebar section; `traffic-usage` is the filterable reading the
  // `dataStatistics` section groups. Both read the Web Server's own traffic
  // contract, and which tenant scope answers is decided server-side.
  | "dashboard"
  | "traffic-usage";

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
