import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
const root = resolve(import.meta.dirname, "..");
function files(directory: string): string[] { return readdirSync(directory).filter((name) => name !== "node_modules").flatMap((name) => { const path = resolve(directory, name); return statSync(path).isDirectory() ? files(path) : path.endsWith(".ts") || path.endsWith(".tsx") ? [path] : []; }); }
describe("surface SDK boundaries", () => {
  it("keeps backend SDK imports out of console packages", () => { const offenders = files(resolve(root, "packages")).filter((path) => path.includes("-console-") && readFileSync(path, "utf8").includes("@sdkwork/webserver-backend-sdk")); expect(offenders).toEqual([]); });
  // The `deploy_app` entity has one owner (`sdkwork-deployments`), so the
  // deployments App SDK is a console-delivery concern: a backend-admin package
  // must reach it through that bridge, never by constructing a client of its
  // own (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7).
  it("keeps deployments App SDK imports out of admin packages", () => { const offenders = files(resolve(root, "packages")).filter((path) => path.includes("-admin-") && readFileSync(path, "utf8").includes("@sdkwork/deployments-app-sdk")); expect(offenders).toEqual([]); });
  // The retired webserver-owned application surface must not come back: this
  // root reads `deploy_app` through the deployments App SDK, and re-adding the
  // webserver family would restore the duplicate authority this root removed.
  it("never re-introduces the webserver-owned application SDK", () => { const offenders = files(resolve(root, "packages")).filter((path) => readFileSync(path, "utf8").includes("@sdkwork/webserver-app-sdk")); expect(offenders).toEqual([]); });
  it("does not use raw HTTP in authored UI packages", () => { const offenders = files(resolve(root, "packages")).filter((path) => /\bfetch\s*\(/.test(readFileSync(path, "utf8"))); expect(offenders).toEqual([]); });
  // The app-console and backend-admin faces render the same canonical
  // sdkwork-deployments pages, and the bridge that turns base URLs plus a token
  // manager into those pages' clients is surface-neutral — it has no
  // admin/console divergence. So a second copy of such a bridge is a defect,
  // not a parallel: the console-delivery package owns every deployments bridge
  // and the admin faces re-export them. `check-frontend-composition` cannot see
  // this (it validates role direction and SDK imports, not duplicate
  // implementations), so the guard lives here.
  it("keeps exactly one implementation per bridged deployments surface", () => {
    const adapters = files(resolve(root, "packages")).filter((path) => /Deploy(?:Apps|Domain)(?:Management|Admin)Surface\.tsx$/.test(path));
    expect(adapters.filter((path) => path.includes("-admin-"))).toEqual([]);
    expect(adapters.filter((path) => path.includes("sdkwork-webserver-pc-console-delivery"))).toHaveLength(adapters.length);
    const adminIndex = readFileSync(resolve(root, "packages/sdkwork-webserver-pc-admin-apps/src/index.ts"), "utf8");
    expect(adminIndex).toContain("DeployAppsManagementSurface as DeployAppsAdminSurface");
    expect(adminIndex).toContain("@sdkwork/webserver-pc-console-delivery");
  });
  // Console and admin tables are rendered by the framework `DataTable`, which owns
  // pagination, sorting, sticky headers, selection, and row actions
  // (`framework-governance.md`: dense table chrome belongs on the composite, not on
  // hand-rolled markup). Authored `<table>` therefore only belongs in content
  // surfaces that genuinely have no data semantics. `DocumentationContent` is the
  // one such case — a two-row static comparison with prose cells, no sorting, no
  // pagination, no selection — so it is allow-listed by path rather than by a
  // blanket exemption that would silently re-admit a hand-rolled data table.
  it("renders data tables through the framework DataTable", () => {
    const contentOnly = resolve(root, "packages/sdkwork-webserver-pc-documentation");
    // Strip comments and string literals first: prose that merely *mentions* a
    // `<table>` (e.g. the comment above the workspace's `DataTable` explaining why
    // it replaced one) is not authored markup, and matching it would make the gate
    // fail for documenting the very rule it enforces.
    const authored = (source: string) =>
      /<table[\s>]/.test(
        source
          .replace(/\/\*[\s\S]*?\*\//g, "")
          .replace(/(^|[^:])\/\/[^\n]*/g, "$1")
          .replace(/\{[^{}]*\}/g, ""),
      );
    const offenders = files(resolve(root, "packages"))
      .filter((path) => !path.startsWith(contentOnly))
      .filter((path) => authored(readFileSync(path, "utf8")));
    expect(offenders).toEqual([]);
  });
});
