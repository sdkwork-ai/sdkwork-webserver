import { createBaseHttpClient, type AuthTokenManager, type BaseHttpClient } from "@sdkwork/sdk-common";

/**
 * VM instance transport.
 *
 * `sandbox_instance` is the per-user virtual machine (VM) registry owned by
 * sdkwork-sandbox and served by that module's app-api face:
 *
 *   GET    {base}/app/v3/api/sandbox/sandbox_instances
 *   POST   {base}/app/v3/api/sandbox/sandbox_instances
 *   GET    {base}/app/v3/api/sandbox/sandbox_instances/{sandboxInstanceId}
 *   PATCH  {base}/app/v3/api/sandbox/sandbox_instances/{sandboxInstanceId}
 *   DELETE {base}/app/v3/api/sandbox/sandbox_instances/{sandboxInstanceId}
 *
 * This is the one place in the console allowed to own a transport
 * (`verify-repo` forbids a capability package from constructing one, and the
 * generated-SDK import boundary is what keeps that rule enforceable), so the
 * Sandbox capability package consumes the factory below exactly the way the
 * plugins capability consumes `createDriveAppClient`.
 *
 * Why the transport is hand-written rather than a generated SDK: sdkwork-sandbox
 * declares its SDK family inactive for Phase 0 (`sdks/README.md`: "Verification:
 * inactive in Phase 0; activate only after `apis/` authority and generation
 * ownership are approved"), so there is no `@sdkwork/sandbox-app-sdk` to compose
 * against. The four operations are small, fully specified by the route crate's
 * payloads, and exercised end-to-end; generating a family for them would activate
 * a machinery the owning repository has deliberately not switched on yet. When
 * that family is activated, this client is the single call site to swap.
 *
 * `createBaseHttpClient` is the shared platform transport: it speaks the
 * SDKWork response envelope (`{ code, data, traceId }` — it unwraps `data` and
 * raises on a non-success `code`), attaches the IAM dual token from the manager
 * (`Access-Token` plus `Authorization: Bearer`), and strips client-projected
 * identity headers before sending (`API_SPEC.md` section 10.2: tenant and user
 * are derived server-side from the authenticated principal, never from a
 * request header).
 */

/** Route collection path, relative to the injected app-api base URL. */
export const SANDBOX_INSTANCES_PATH = "/app/v3/api/sandbox/sandbox_instances";

/** Lifecycle states the provisioning service admits. */
export type SandboxInstanceState =
  | "requested"
  | "active"
  | "suspended"
  | "terminated"
  | "failed";

/** Resource shapes a request may ask for. */
export type SandboxInstanceProfile =
  | "standard"
  | "memory_optimized"
  | "compute_optimized";

/** Runtime capabilities an instance may require. */
export type SandboxRuntimeCapability =
  | "build"
  | "browser"
  | "environment"
  | "filesystem"
  | "git"
  | "mcp_transport"
  | "port_forward"
  | "terminal";

/** Minimum isolation the instance must be placed under. */
export type SandboxIsolationAssurance =
  | "host_user"
  | "container"
  | "user_space_kernel"
  | "micro_vm"
  | "dedicated_vm";

export const SANDBOX_INSTANCE_STATES: readonly SandboxInstanceState[] = [
  "requested",
  "active",
  "suspended",
  "terminated",
  "failed",
];

export const SANDBOX_INSTANCE_PROFILES: readonly SandboxInstanceProfile[] = [
  "standard",
  "memory_optimized",
  "compute_optimized",
];

export const SANDBOX_RUNTIME_CAPABILITIES: readonly SandboxRuntimeCapability[] = [
  "build",
  "browser",
  "environment",
  "filesystem",
  "git",
  "mcp_transport",
  "port_forward",
  "terminal",
];

export const SANDBOX_ISOLATION_ASSURANCES: readonly SandboxIsolationAssurance[] = [
  "host_user",
  "container",
  "user_space_kernel",
  "micro_vm",
  "dedicated_vm",
];

/** States an instance can never leave, so no transition may be requested. */
export const SANDBOX_TERMINAL_STATES: readonly SandboxInstanceState[] = ["terminated", "failed"];

/**
 * One VM instance, exactly as the app-api serializes it (`camelCase`, no
 * rename layer).
 *
 * `sandboxVersion` is a `string` on purpose: it is an `int64` on the wire
 * (`API_SPEC.md` section 13.6) and a JavaScript number would round it, which
 * would either lose the optimistic-concurrency comparison or, worse, win it by
 * accident. It is carried through the console untouched and returned on update
 * so the server can reject a stale write.
 */
export interface SandboxInstance {
  sandboxInstanceId: string;
  sandboxInstanceOwnerId: string;
  sandboxInstanceName: string;
  sandboxInstanceState: SandboxInstanceState;
  sandboxInstanceProfile: SandboxInstanceProfile;
  sandboxInstanceBaseImage: string;
  sandboxInstanceVcpuCount: number;
  sandboxInstanceMemoryMb: number;
  sandboxInstanceDiskMb: number;
  sandboxInstanceRequiredCapabilities: readonly SandboxRuntimeCapability[];
  sandboxInstanceMinimumAssurance: SandboxIsolationAssurance;
  sandboxInstanceAutoStart: boolean;
  sandboxInstanceExpiresAt?: string;
  sandboxWorkspaceId?: string;
  sandboxVersion: string;
  createdAt?: string;
  updatedAt?: string;
}

/** The `pageInfo` half of an offset page (`PAGINATION_SPEC`). */
export interface SandboxInstancePageInfo {
  mode?: "cursor" | "offset";
  page?: number;
  pageSize?: number;
  /** `int64` on the wire: a decimal string, never a rounded number. */
  totalItems?: string;
  totalPages?: number;
  nextCursor?: string;
  hasMore?: boolean;
}

export interface SandboxInstancePage {
  items: readonly SandboxInstance[];
  pageInfo?: SandboxInstancePageInfo;
}

/** Query accepted by the collection listing. */
export interface SandboxInstanceListQuery {
  /** 1-based; the server clamps to `>= 1`. */
  page?: number;
  /** The server clamps to `1..200`. */
  pageSize?: number;
  /** Narrows the tenant-wide listing to one owner. */
  sandboxInstanceOwnerId?: string;
  sandboxInstanceState?: SandboxInstanceState;
}

/** Body accepted by `POST` — the owner and tenant are never client-supplied. */
export interface CreateSandboxInstanceInput {
  sandboxInstanceName: string;
  sandboxInstanceProfile: SandboxInstanceProfile;
  sandboxInstanceBaseImage: string;
  sandboxInstanceVcpuCount: number;
  sandboxInstanceMemoryMb: number;
  sandboxInstanceDiskMb: number;
  sandboxInstanceRequiredCapabilities?: readonly SandboxRuntimeCapability[];
  sandboxInstanceMinimumAssurance: SandboxIsolationAssurance;
  sandboxInstanceAutoStart?: boolean;
  sandboxInstanceExpiresAt?: string;
  sandboxWorkspaceId?: string;
}

/**
 * Body accepted by `PATCH`.
 *
 * Every key is optional and absent means "leave the stored value alone" — which
 * is why `sandboxInstanceExpiresAt` is three-way: the key omitted keeps the
 * expiry, `null` clears it, and a timestamp sets it. The server distinguishes
 * those three cases explicitly, so collapsing "absent" and "null" here would
 * silently make clearing an expiry impossible.
 */
export interface UpdateSandboxInstanceInput {
  sandboxInstanceName?: string;
  sandboxInstanceProfile?: SandboxInstanceProfile;
  sandboxInstanceVcpuCount?: number;
  sandboxInstanceMemoryMb?: number;
  sandboxInstanceDiskMb?: number;
  sandboxInstanceAutoStart?: boolean;
  sandboxInstanceExpiresAt?: string | null;
  sandboxWorkspaceId?: string;
  sandboxInstanceState?: SandboxInstanceState;
}

/** Result of a delete: the module reports the identity it retired. */
export interface DeletedSandboxInstance {
  sandboxInstanceId: string;
  deleted: boolean;
}

/** One resource as the envelope wraps it. */
interface SandboxItemEnvelope<T> {
  item: T;
}

function dropUndefined<T extends object>(source: T): Record<string, unknown> {
  const body: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(source)) {
    if (value !== undefined) body[key] = value;
  }
  return body;
}

export class SandboxAppClient {
  /**
   * The shared request boundary, exposed so the host can register it with the
   * IAM session-auth boundary (`attachSdkClientBoundaries`): that integration
   * wraps `http.request`, which is exactly how a 401 from this plane clears the
   * session like a 401 from any generated client. Without it a revoked session
   * would surface as a wall of failed reads instead of a sign-out.
   */
  readonly http: BaseHttpClient;

  constructor(baseUrl: string, tokenManager: AuthTokenManager) {
    this.http = createBaseHttpClient({ baseUrl, tokenManager });
  }

  async list(query: SandboxInstanceListQuery = {}): Promise<SandboxInstancePage> {
    const params: Record<string, string> = {};
    if (query.page !== undefined) params.page = String(query.page);
    if (query.pageSize !== undefined) params.pageSize = String(query.pageSize);
    if (query.sandboxInstanceOwnerId) params.sandboxInstanceOwnerId = query.sandboxInstanceOwnerId;
    if (query.sandboxInstanceState) params.sandboxInstanceState = query.sandboxInstanceState;
    return this.http.get<SandboxInstancePage>(SANDBOX_INSTANCES_PATH, params);
  }

  async create(body: CreateSandboxInstanceInput): Promise<SandboxInstance> {
    const created = await this.http.post<SandboxItemEnvelope<SandboxInstance>>(
      SANDBOX_INSTANCES_PATH,
      dropUndefined(body),
    );
    return created.item;
  }

  async retrieve(sandboxInstanceId: string): Promise<SandboxInstance> {
    const found = await this.http.get<SandboxItemEnvelope<SandboxInstance>>(
      sandboxInstancePath(sandboxInstanceId),
    );
    return found.item;
  }

  async update(
    sandboxInstanceId: string,
    body: UpdateSandboxInstanceInput,
  ): Promise<SandboxInstance> {
    const updated = await this.http.patch<SandboxItemEnvelope<SandboxInstance>>(
      sandboxInstancePath(sandboxInstanceId),
      dropUndefined(body),
    );
    return updated.item;
  }

  async remove(sandboxInstanceId: string): Promise<DeletedSandboxInstance> {
    const deleted = await this.http.delete<SandboxItemEnvelope<DeletedSandboxInstance>>(
      sandboxInstancePath(sandboxInstanceId),
    );
    return deleted.item;
  }
}

/**
 * One instance's path.
 *
 * The identifier is percent-encoded: it is a server-issued opaque handle, and a
 * value that reached this function with a slash in it must not be able to
 * address a different route.
 */
function sandboxInstancePath(sandboxInstanceId: string): string {
  return `${SANDBOX_INSTANCES_PATH}/${encodeURIComponent(sandboxInstanceId)}`;
}

export type SandboxAppClientLike = SandboxAppClient;

export function createSandboxAppClient(
  appApiBaseUrl: string,
  tokenManager: AuthTokenManager,
): SandboxAppClient {
  return new SandboxAppClient(appApiBaseUrl, tokenManager);
}
