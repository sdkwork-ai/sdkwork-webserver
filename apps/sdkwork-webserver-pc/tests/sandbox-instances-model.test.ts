import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import {
  SANDBOX_INSTANCES_PATH,
  SANDBOX_INSTANCE_STATES,
  SANDBOX_ISOLATION_ASSURANCES,
  SANDBOX_RUNTIME_CAPABILITIES,
} from "../packages/sdkwork-webserver-pc-console-core/src/sandbox-client.ts";
import {
  createSandboxInstanceDraft,
  isSandboxDeletable,
  isSandboxTerminalState,
  sandboxDraftFromInstance,
  sandboxProfileBounds,
  sandboxStateTransitions,
  sandboxTotalItems,
  toCreateSandboxInstanceInput,
  toSandboxExpiresAtLocal,
  toSandboxExpiresAtUtc,
  toUpdateSandboxInstanceInput,
  toggleSandboxCapability,
  validateSandboxInstanceDraft,
  SANDBOX_CAPABILITY_ORDER,
  SANDBOX_DEFAULT_ASSURANCE,
  SANDBOX_DEFAULT_BASE_IMAGE,
  SANDBOX_DEFAULT_PROFILE,
  SANDBOX_INSTANCE_LIMITS,
  SANDBOX_PROFILE_BOUNDS,
  SANDBOX_PROFILE_ORDER,
} from "../packages/sdkwork-webserver-pc-console-sandbox/src/sandbox-instances-model.ts";

/**
 * The VM instances model echoes the provisioning service's rules so the page
 * can reject a request before spending a round trip on it. An echo that drifts is
 * worse than no echo at all — it would refuse a shape the server accepts, or
 * accept one it refuses and turn a form-level mistake into a 400.
 *
 * So the second half of this file is a differential oracle: it reads the Rust
 * authority in `sdkwork-sandbox` and asserts every bound, every state transition,
 * and the route path against it. Those are the numbers that actually have to
 * agree; the unit tests above them only pin the mapping behaviour.
 */

const here = dirname(fileURLToPath(import.meta.url));
/** `D:\sdkwork-space` — the directory holding every sibling repository. */
const workspaceRoot = resolve(here, "../../../../");

function source(relativePath: string): string {
  return readFileSync(resolve(workspaceRoot, relativePath), "utf8");
}

const serviceSource = source("sdkwork-sandbox/crates/sdkwork-intelligence-sandbox-service/src/instance.rs");
const routePaths = source("sdkwork-sandbox/crates/sdkwork-routes-sandbox-app-api/src/paths.rs");
const spiSource = source("sdkwork-sandbox/crates/sdkwork-sandbox-provider-spi/src/capability.rs");

/** `MemoryOptimized` → `memory_optimized`. */
function snakeCase(variant: string): string {
  return variant.replaceAll(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
}

/** `pub const NAME: u32 = 262_144;` → 262144. */
function rustConstant(name: string): number {
  const match = serviceSource.match(new RegExp(`pub const ${name}: \\w+ = ([0-9_]+);`));
  expect(match, `${name} must be declared by the provisioning service`).not.toBeNull();
  return Number.parseInt(match![1].replaceAll("_", ""), 10);
}

/** Resolves either a literal (`16_384`) or a constant name (`MAX_..._MB`). */
function rustNumber(token: string): number {
  const trimmed = token.trim();
  if (/^[0-9_]+$/.test(trimmed)) return Number.parseInt(trimmed.replaceAll("_", ""), 10);
  return rustConstant(trimmed);
}

describe("sandbox instance state machine", () => {
  it("offers exactly the transitions the service accepts", () => {
    expect(sandboxStateTransitions("requested")).toEqual(["active", "suspended", "failed"]);
    expect(sandboxStateTransitions("active")).toEqual(["suspended", "terminated", "failed"]);
    expect(sandboxStateTransitions("suspended")).toEqual(["active", "terminated", "failed"]);
    expect(sandboxStateTransitions("terminated")).toEqual([]);
    expect(sandboxStateTransitions("failed")).toEqual([]);
  });

  it("never offers a self-transition", () => {
    for (const state of SANDBOX_INSTANCE_STATES) {
      expect(sandboxStateTransitions(state)).not.toContain(state);
    }
  });

  it("marks only terminated and failed as terminal", () => {
    expect(SANDBOX_INSTANCE_STATES.filter(isSandboxTerminalState)).toEqual(["terminated", "failed"]);
  });

  it("refuses deletion only while the instance is live", () => {
    // `active` is the one state a single DELETE must not be able to remove.
    expect(SANDBOX_INSTANCE_STATES.filter(isSandboxDeletable))
      .toEqual(["requested", "suspended", "terminated", "failed"]);
  });
});

describe("sandbox instance profile envelopes", () => {
  it("keeps the three envelopes the service declares", () => {
    expect(SANDBOX_PROFILE_BOUNDS).toEqual({
      compute_optimized: { maxDiskMb: 1_048_576, maxMemoryMb: 65_536, maxVcpuCount: 64 },
      memory_optimized: { maxDiskMb: 204_800, maxMemoryMb: 262_144, maxVcpuCount: 16 },
      standard: { maxDiskMb: 102_400, maxMemoryMb: 16_384, maxVcpuCount: 8 },
    });
  });

  it("orders the profile picker by the transport vocabulary, not by key order", () => {
    expect(SANDBOX_PROFILE_ORDER).toEqual(["standard", "memory_optimized", "compute_optimized"]);
  });
});

describe("sandbox instance draft validation", () => {
  const createDraft = createSandboxInstanceDraft;

  it("accepts the default draft and seeds it with service-legal defaults", () => {
    const draft = createDraft({ name: "vm-1" });
    expect(validateSandboxInstanceDraft(draft, { requireBaseImage: true })).toEqual({});
    expect(draft).toMatchObject({
      assurance: SANDBOX_DEFAULT_ASSURANCE,
      autoStart: false,
      baseImage: SANDBOX_DEFAULT_BASE_IMAGE,
      expiresAtLocal: "",
      profile: SANDBOX_DEFAULT_PROFILE,
      requiredCapabilities: [],
      workspaceId: "",
    });
    // A draft that is legal at the envelope floor must stay legal: `standard`'s
    // ceiling is the narrowest of the three, so these defaults cannot be out of
    // range on any profile.
    for (const profile of SANDBOX_PROFILE_ORDER) {
      expect(validateSandboxInstanceDraft({ ...draft, profile }, { requireBaseImage: true })).toEqual({});
    }
  });

  it("requires a name on every submit path", () => {
    expect(validateSandboxInstanceDraft(createDraft(), { requireBaseImage: true }).name)
      .toBe("name.required");
    expect(validateSandboxInstanceDraft(createDraft({ name: "   " }), { requireBaseImage: true }).name)
      .toBe("name.required");
    expect(
      validateSandboxInstanceDraft(createDraft({ name: "x".repeat(129) }), { requireBaseImage: true }).name,
    ).toBe("name.tooLong");
    // Exactly at the bound is accepted: the service compares `>`.
    expect(
      validateSandboxInstanceDraft(createDraft({ name: "x".repeat(128) }), { requireBaseImage: true }).name,
    ).toBeUndefined();
  });

  it("requires a base image only where the wire has the field", () => {
    // `PATCH` carries no `sandboxInstanceBaseImage`, so an edit must not be
    // blocked by a base image it can never send.
    const blank = createDraft({ name: "vm-1", baseImage: "" });
    expect(validateSandboxInstanceDraft(blank, { requireBaseImage: true }).baseImage)
      .toBe("baseImage.required");
    expect(validateSandboxInstanceDraft(blank, { requireBaseImage: false }).baseImage)
      .toBeUndefined();
  });

  it("rejects a shape outside the selected profile envelope", () => {
    const tooBigForStandard = createDraft({
      name: "vm-1",
      profile: "standard",
      vcpuCount: "9",
      memoryMb: "16385",
      diskMb: "102401",
    });
    const standardVerdict = validateSandboxInstanceDraft(tooBigForStandard, { requireBaseImage: true });
    expect(standardVerdict.vcpuCount).toBe("vcpu.outOfProfile");
    expect(standardVerdict.memoryMb).toBe("memory.outOfProfile");
    expect(standardVerdict.diskMb).toBe("disk.outOfProfile");

    // The same shape is legal under the wider envelope: the check is the profile,
    // not a global ceiling.
    expect(
      validateSandboxInstanceDraft({ ...tooBigForStandard, profile: "compute_optimized" }, { requireBaseImage: true }),
    ).toEqual({});
  });

  it("rejects the absolute floor and unparsable numbers", () => {
    const verdict = validateSandboxInstanceDraft(
      createDraft({ name: "vm-1", vcpuCount: "0", memoryMb: "255", diskMb: "1023" }),
      { requireBaseImage: true },
    );
    expect(verdict.vcpuCount).toBe("vcpu.outOfProfile");
    expect(verdict.memoryMb).toBe("memory.outOfProfile");
    expect(verdict.diskMb).toBe("disk.outOfProfile");

    const blank = validateSandboxInstanceDraft(
      createDraft({ name: "vm-1", vcpuCount: "", memoryMb: "  ", diskMb: "2.5" }),
      { requireBaseImage: true },
    );
    // An empty box is "not filled in", not 0 — and 2.5 is not a MiB count.
    expect(blank.vcpuCount).toBe("vcpu.notANumber");
    expect(blank.memoryMb).toBe("memory.notANumber");
    expect(blank.diskMb).toBe("disk.notANumber");
  });

  it("rejects an unparsable expiry but accepts an empty one", () => {
    expect(
      validateSandboxInstanceDraft(createDraft({ name: "vm-1", expiresAtLocal: "not-a-date" }), { requireBaseImage: true }).expiresAtLocal,
    ).toBe("expiresAt.invalid");
    expect(
      validateSandboxInstanceDraft(createDraft({ name: "vm-1", expiresAtLocal: "" }), { requireBaseImage: true }).expiresAtLocal,
    ).toBeUndefined();
  });
});

describe("sandbox instance expiry conversion", () => {
  it("emits the exact RFC 3339 form the service accepts", () => {
    const utc = toSandboxExpiresAtUtc("2026-10-01T09:30");
    expect(utc).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
    // Only the millisecond form satisfies the service's validator, so the
    // zero-second form is never produced.
    expect(utc).toBe(new Date("2026-10-01T09:30").toISOString());
  });

  it("returns undefined for a blank or unparsable box rather than an invalid string", () => {
    expect(toSandboxExpiresAtUtc("")).toBeUndefined();
    expect(toSandboxExpiresAtUtc("   ")).toBeUndefined();
    expect(toSandboxExpiresAtUtc("nope")).toBeUndefined();
  });

  it("round-trips through the local datetime-local representation", () => {
    const local = "2026-10-01T09:30";
    expect(toSandboxExpiresAtLocal(toSandboxExpiresAtUtc(local))).toBe(local);
    expect(toSandboxExpiresAtLocal(undefined)).toBe("");
    expect(toSandboxExpiresAtLocal("not-a-date")).toBe("");
  });
});

describe("sandbox instance command mapping", () => {
  const base = { name: "vm-1", baseImage: "img:1", vcpuCount: "2", memoryMb: "4096", diskMb: "20480" };

  it("omits the expiry key on create when the box is empty", () => {
    const body = toCreateSandboxInstanceInput(createSandboxInstanceDraft(base));
    expect(Object.hasOwn(body, "sandboxInstanceExpiresAt")).toBe(false);
    expect(Object.hasOwn(body, "sandboxWorkspaceId")).toBe(false);
    expect(body).toMatchObject({
      sandboxInstanceAutoStart: false,
      sandboxInstanceBaseImage: "img:1",
      sandboxInstanceDiskMb: 20_480,
      sandboxInstanceMemoryMb: 4_096,
      sandboxInstanceMinimumAssurance: SANDBOX_DEFAULT_ASSURANCE,
      sandboxInstanceName: "vm-1",
      sandboxInstanceProfile: SANDBOX_DEFAULT_PROFILE,
      sandboxInstanceRequiredCapabilities: [],
      sandboxInstanceVcpuCount: 2,
    });
  });

  it("sends a set expiry on create", () => {
    const body = toCreateSandboxInstanceInput(
      createSandboxInstanceDraft({ ...base, expiresAtLocal: "2026-10-01T09:30" }),
    );
    expect(body.sandboxInstanceExpiresAt).toBe(toSandboxExpiresAtUtc("2026-10-01T09:30"));
  });

  it("carries the optional workspace only when it is filled in", () => {
    expect(
      Object.hasOwn(toCreateSandboxInstanceInput(createSandboxInstanceDraft({ ...base, workspaceId: "  " })), "sandboxWorkspaceId"),
    ).toBe(false);
    expect(
      toCreateSandboxInstanceInput(createSandboxInstanceDraft({ ...base, workspaceId: " ws-1 " })).sandboxWorkspaceId,
    ).toBe("ws-1");
  });

  it("clears the expiry on update rather than omitting the key", () => {
    // The three-way contract: absent leaves the stored value, `null` clears it.
    // Collapsing the two would make a cleared expiry un-clearable.
    const cleared = toUpdateSandboxInstanceInput(createSandboxInstanceDraft({ ...base, expiresAtLocal: "" }));
    expect(Object.hasOwn(cleared, "sandboxInstanceExpiresAt")).toBe(true);
    expect(cleared.sandboxInstanceExpiresAt).toBeNull();

    const set = toUpdateSandboxInstanceInput(
      createSandboxInstanceDraft({ ...base, expiresAtLocal: "2026-10-01T09:30" }),
    );
    expect(set.sandboxInstanceExpiresAt).toBe(toSandboxExpiresAtUtc("2026-10-01T09:30"));
  });

  it("omits the state key unless a transition was asked for", () => {
    const untouched = toUpdateSandboxInstanceInput(createSandboxInstanceDraft(base));
    expect(Object.hasOwn(untouched, "sandboxInstanceState")).toBe(false);

    const moved = toUpdateSandboxInstanceInput(createSandboxInstanceDraft(base), { state: "suspended" });
    expect(moved.sandboxInstanceState).toBe("suspended");
  });

  it("refuses to build a command from an unvalidated draft", () => {
    const broken = createSandboxInstanceDraft({ ...base, memoryMb: "" });
    expect(() => toCreateSandboxInstanceInput(broken)).toThrow(/numeric fields are required/);
    expect(() => toUpdateSandboxInstanceInput(broken)).toThrow(/numeric fields are required/);
  });

  it("seeds an edit draft from the row's stored values", () => {
    const draft = sandboxDraftFromInstance({
      sandboxInstanceId: "sbi-1",
      sandboxInstanceOwnerId: "usr-1",
      sandboxInstanceName: "vm-1",
      sandboxInstanceState: "active",
      sandboxInstanceProfile: "memory_optimized",
      sandboxInstanceBaseImage: "img:2",
      sandboxInstanceVcpuCount: 4,
      sandboxInstanceMemoryMb: 32_768,
      sandboxInstanceDiskMb: 51_200,
      sandboxInstanceRequiredCapabilities: ["git", "terminal"],
      sandboxInstanceMinimumAssurance: "micro_vm",
      sandboxInstanceAutoStart: true,
      sandboxInstanceExpiresAt: "2026-10-01T01:30:00.000Z",
      sandboxWorkspaceId: "ws-1",
      sandboxVersion: "7",
    });
    expect(draft).toMatchObject({
      assurance: "micro_vm",
      autoStart: true,
      baseImage: "img:2",
      diskMb: "51200",
      memoryMb: "32768",
      name: "vm-1",
      profile: "memory_optimized",
      requiredCapabilities: ["git", "terminal"],
      vcpuCount: "4",
      workspaceId: "ws-1",
    });
    expect(draft.expiresAtLocal).toBe(toSandboxExpiresAtLocal("2026-10-01T01:30:00.000Z"));
    // A 51200 MiB disk with memory_optimized must re-validate cleanly.
    expect(validateSandboxInstanceDraft(draft, { requireBaseImage: false })).toEqual({});
  });

  it("keeps capabilities in canonical order and toggles them off", () => {
    const withGit = toggleSandboxCapability([], "git");
    expect(withGit).toEqual(["git"]);
    const withBoth = toggleSandboxCapability(withGit, "terminal");
    // Canonical order is terminal before git, regardless of click order.
    expect(withBoth).toEqual(["terminal", "git"]);
    expect(toggleSandboxCapability(withBoth, "git")).toEqual(["terminal"]);
  });
});

describe("sandbox instance pagination", () => {
  it("parses the int64 string without producing NaN", () => {
    expect(sandboxTotalItems({ totalItems: "42" })).toBe(42);
    expect(sandboxTotalItems({ totalItems: "900719925474099" })).toBe(900_719_925_474_099);
    expect(sandboxTotalItems({ totalItems: "not-a-number" })).toBe(0);
    expect(sandboxTotalItems({ totalItems: "" })).toBe(0);
    expect(sandboxTotalItems({})).toBe(0);
    expect(sandboxTotalItems(undefined)).toBe(0);
  });
});

describe("sandbox model stays a faithful echo of the Rust authority", () => {
  it("routes at the path the route crate declares", () => {
    const collection = routePaths.match(/pub const SANDBOX_INSTANCES: &str = "([^"]+)";/);
    const single = routePaths.match(/pub const SANDBOX_INSTANCE: &str = "([^"]+)";/);
    expect(collection, "SANDBOX_INSTANCES must be declared").not.toBeNull();
    expect(single, "SANDBOX_INSTANCE must be declared").not.toBeNull();
    expect(SANDBOX_INSTANCES_PATH).toBe(collection![1]);
    // The item route is the collection path plus the encoded identifier segment;
    // the client builds it, so the two declarations have to line up.
    expect(`${SANDBOX_INSTANCES_PATH}/{sandboxInstanceId}`).toBe(single![1]);
  });

  it("restates the absolute column bounds", () => {
    expect(SANDBOX_INSTANCE_LIMITS).toEqual({
      baseImageMaxLength: rustConstant("MAX_SANDBOX_INSTANCE_BASE_IMAGE_LENGTH"),
      capabilitiesMax: rustConstant("MAX_SANDBOX_INSTANCE_REQUIRED_CAPABILITIES"),
      diskMaxMb: rustConstant("MAX_SANDBOX_INSTANCE_DISK_MB"),
      diskMinMb: rustConstant("MIN_SANDBOX_INSTANCE_DISK_MB"),
      memoryMaxMb: rustConstant("MAX_SANDBOX_INSTANCE_MEMORY_MB"),
      memoryMinMb: rustConstant("MIN_SANDBOX_INSTANCE_MEMORY_MB"),
      nameMaxLength: rustConstant("MAX_SANDBOX_INSTANCE_NAME_LENGTH"),
      vcpuMaxCount: rustConstant("MAX_SANDBOX_INSTANCE_VCPU_COUNT"),
      vcpuMinCount: rustConstant("MIN_SANDBOX_INSTANCE_VCPU_COUNT"),
    });
  });

  it("restates each profile envelope arm of `bounds()`", () => {
    for (const [profile, variant] of [
      ["standard", "Standard"],
      ["memory_optimized", "MemoryOptimized"],
      ["compute_optimized", "ComputeOptimized"],
    ] as const) {
      const arm = serviceSource.match(
        new RegExp(`Self::${variant}\\s*=>\\s*SandboxInstanceResourceBounds\\s*\\{([\\s\\S]*?)\\}`),
      );
      expect(arm, `the ${variant} arm of bounds() must be declared`).not.toBeNull();
      const field = (name: string): number => {
        const match = arm![1].match(new RegExp(`${name}:\\s*([A-Za-z0-9_]+),`));
        expect(match, `${variant}.${name} must be declared`).not.toBeNull();
        return rustNumber(match![1]);
      };
      expect(sandboxProfileBounds(profile)).toEqual({
        maxDiskMb: field("max_disk_mb"),
        maxMemoryMb: field("max_memory_mb"),
        maxVcpuCount: field("max_vcpu_count"),
      });
    }
  });

  it("restates every arm of `can_transition_to`", () => {
    const block = serviceSource.match(/\(self, target\),([\s\S]*?)\n\s*\)/);
    expect(block, "the can_transition_to matrix must be extractable").not.toBeNull();
    const pairs = [...block![1].matchAll(/\(Self::(\w+),\s*Self::(\w+)\)/g)]
      .map(([, from, to]) => `${snakeCase(from)}>${snakeCase(to)}`);
    expect(pairs.length, "the matrix must declare at least one transition").toBeGreaterThan(0);
    // Every declared pair must be offered, and nothing else: a transition the
    // page offers but the service refuses is a control that answers 409.
    for (const from of SANDBOX_INSTANCE_STATES) {
      const offered = sandboxStateTransitions(from).map((to) => `${from}>${to}`).sort();
      const declared = pairs.filter((pair) => pair.startsWith(`${from}>`)).sort();
      expect(offered, `transitions out of ${from}`).toEqual(declared);
    }
  });

  it("restates the capability and assurance vocabularies, in the service's order", () => {
    // The declaration order of `IsolationAssurance` IS the security ladder — the
    // derived `Ord` is what `satisfies_sandbox_requirements` compares — so the
    // picker has to offer it weakest-first and must not be re-sorted.
    const assuranceLadder = spiSource.match(/pub enum IsolationAssurance \{([\s\S]*?)\}/);
    expect(assuranceLadder, "IsolationAssurance must be declared").not.toBeNull();
    const declaredLadder = [...assuranceLadder![1].matchAll(/^\s*(\w+),/gm)].map(([, v]) => snakeCase(v));
    expect([...SANDBOX_ISOLATION_ASSURANCES]).toEqual(declaredLadder);
    expect(declaredLadder.length).toBeGreaterThanOrEqual(2);

    // Capabilities are a set on the wire, so only the membership is contractual —
    // but the picker renders them in a fixed order of its own and every offered
    // value must be one the service can parse.
    const capabilityTable = spiSource.match(
      /pub enum RuntimeCapability \{([\s\S]*?)\}\n/,
    );
    expect(capabilityTable, "RuntimeCapability must be declared").not.toBeNull();
    const declaredCapabilities = [...capabilityTable![1].matchAll(/^\s*(\w+),/gm)].map(([, v]) => snakeCase(v));
    expect([...SANDBOX_RUNTIME_CAPABILITIES].sort()).toEqual([...declaredCapabilities].sort());
    expect([...SANDBOX_CAPABILITY_ORDER].sort()).toEqual([...declaredCapabilities].sort());
    // The picker's order is a deliberate permutation, not the declaration order:
    // it leads with the capabilities an operator recognises as "what this VM is
    // for". A silent re-sort to declaration order would still be a valid set, so
    // the order itself is pinned.
    expect([...SANDBOX_CAPABILITY_ORDER]).toEqual([
      "terminal",
      "filesystem",
      "git",
      "build",
      "browser",
      "port_forward",
      "mcp_transport",
      "environment",
    ]);
    expect(SANDBOX_ISOLATION_ASSURANCES).toContain(SANDBOX_DEFAULT_ASSURANCE);
  });

  it("restates the terminal-state set", () => {
    expect(serviceSource).toMatch(/matches!\(self, Self::Terminated \| Self::Failed\)/);
    expect(serviceSource).toMatch(/!matches!\(self, Self::Active\)/);
  });
});
