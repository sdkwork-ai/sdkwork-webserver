import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createSandboxAppClient,
  SANDBOX_INSTANCES_PATH,
  type SandboxAppClient,
  type SandboxInstance,
} from "@sdkwork/webserver-pc-console-core";

/**
 * Wire contract of the hand-written Sandbox transport.
 *
 * The console UI tests mock the client object, so nothing above this file sees
 * the actual query string. These tests exercise the real `createBaseHttpClient`
 * transport against a stubbed `fetch` and pin the wire the app-api authority
 * declares: query vocabulary is `lower_snake_case`
 * (`PAGINATION_SPEC.md` section 0 — `pageSize` as a GET alias is a contract
 * violation), paths are the canonical collection/item pair, and the platform
 * envelope is unwrapped in the success case and raised in the failure case.
 */

const INSTANCE: SandboxInstance = {
  sandboxInstanceId: "sbi-1",
  sandboxInstanceOwnerId: "usr-1",
  sandboxInstanceName: "agent-vm-01",
  sandboxInstanceState: "requested",
  sandboxInstanceProfile: "standard",
  sandboxInstanceBaseImage: "sdkwork/sandbox-runtime:latest",
  sandboxInstanceVcpuCount: 2,
  sandboxInstanceMemoryMb: 4_096,
  sandboxInstanceDiskMb: 20_480,
  sandboxInstanceRequiredCapabilities: ["terminal"],
  sandboxInstanceMinimumAssurance: "container",
  sandboxInstanceAutoStart: false,
  sandboxVersion: "3",
};

type RecordedRequest = { url: string; init: RequestInit };

let requests: RecordedRequest[];

function respondWith(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function envelope(data: unknown): unknown {
  return { code: 0, data, traceId: "wire-test-trace" };
}

function client(): SandboxAppClient {
  return createSandboxAppClient("http://sandbox.test", {
    isValid: () => true,
    getAccessToken: () => "access-token-value",
    getAuthToken: () => "auth-token-value",
  } as never);
}

function lastUrl(): string {
  const last = requests.at(-1);
  if (!last) throw new Error("no request captured");
  return last.url;
}

beforeEach(() => {
  requests = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      requests.push({ url: String(input), init: init ?? {} });
      const method = (init?.method ?? "GET").toUpperCase();
      // GET collection unwraps to a page; commands unwrap to `data.item`.
      const data = method === "GET" && !String(input).includes("/sbi%2F1")
        ? { items: [INSTANCE], pageInfo: { mode: "offset", page: 1, pageSize: 20, totalItems: "1" } }
        : { item: INSTANCE };
      return respondWith(envelope(data));
    }),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("sandbox app client wire contract", () => {
  it("lists with snake_case query vocabulary and no owner alias", async () => {
    await client().list({ page: 2, pageSize: 50, sandboxInstanceState: "active" });

    const url = new URL(lastUrl());
    expect(url.pathname).toBe(SANDBOX_INSTANCES_PATH);
    // `PAGINATION_SPEC.md` section 0: `page_size` is the only page-size query
    // parameter; a `pageSize` alias here would be a wire violation, and an
    // owner parameter would widen the face beyond the verified caller.
    expect(Object.fromEntries(url.searchParams)).toEqual({
      page: "2",
      page_size: "50",
      sandbox_instance_state: "active",
    });
  });

  it("omits absent filters instead of sending empty values", async () => {
    await client().list({ page: 1 });

    const url = new URL(lastUrl());
    expect(Object.fromEntries(url.searchParams)).toEqual({ page: "1" });
  });

  it("creates with the camelCase body and drops undefined keys", async () => {
    await client().create({
      sandboxInstanceName: "agent-vm-01",
      sandboxInstanceProfile: "standard",
      sandboxInstanceBaseImage: "sdkwork/sandbox-runtime:latest",
      sandboxInstanceVcpuCount: 2,
      sandboxInstanceMemoryMb: 4_096,
      sandboxInstanceDiskMb: 20_480,
      sandboxInstanceMinimumAssurance: "container",
      sandboxInstanceAutoStart: undefined,
      sandboxInstanceExpiresAt: undefined,
      sandboxWorkspaceId: undefined,
    });

    const last = requests.at(-1);
    if (!last) throw new Error("no request captured");
    expect(last.init.method).toBe("POST");
    const body = JSON.parse(String(last.init.body)) as Record<string, unknown>;
    expect(body).toEqual({
      sandboxInstanceName: "agent-vm-01",
      sandboxInstanceProfile: "standard",
      sandboxInstanceBaseImage: "sdkwork/sandbox-runtime:latest",
      sandboxInstanceVcpuCount: 2,
      sandboxInstanceMemoryMb: 4_096,
      sandboxInstanceDiskMb: 20_480,
      sandboxInstanceMinimumAssurance: "container",
    });
    // The owner is server-derived (API_SPEC §10.2): it must never ride the body.
    expect(body).not.toHaveProperty("sandboxInstanceOwnerId");
    expect(body).not.toHaveProperty("tenantId");
  });

  it("addresses one instance through the encoded item path", async () => {
    await client().retrieve("sbi/1");
    expect(lastUrl()).toBe(
      `http://sandbox.test${SANDBOX_INSTANCES_PATH}/sbi%2F1`,
    );

    await client().update("sbi/1", { sandboxInstanceName: "renamed" });
    const patched = requests.at(-1);
    if (!patched) throw new Error("no request captured");
    expect(patched.init.method).toBe("PATCH");

    await client().remove("sbi/1");
    const deleted = requests.at(-1);
    if (!deleted) throw new Error("no request captured");
    expect(deleted.init.method).toBe("DELETE");
  });

  it("unwraps the success envelope and raises on a failure code", async () => {
    const listed = await client().list();
    expect(listed.items).toEqual([INSTANCE]);
    expect(listed.pageInfo?.totalItems).toBe("1");

    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        respondWith(
          {
            code: 40101,
            detail: "authentication required",
            traceId: "t",
          },
          401,
        ),
      ),
    );
    await expect(client().list()).rejects.toThrow();
  });
});
