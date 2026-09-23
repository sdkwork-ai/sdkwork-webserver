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
  //
  // ## The second exemption: pages that *mirror* a Deployments page
  //
  // `webserver-pc-admin-delivery` carries the tenant-level Domains and Certificates
  // ledgers. Those two pages are not a host-owned resource page with its own design;
  // they are the same entity the tenant console shows, at a different ownership
  // level, read by the same operator in the same session. Their whole brief is to
  // be indistinguishable from the console pair — and the console pair is an
  // authored `table.domain-table` owned by `sdkwork-deployments`, whose
  // `operations-column` / `row-actions` / `table-action` hooks the mirror
  // stylesheet in `src/deploy-surface.css` defines *only* for that markup.
  //
  // So the composite cannot express these two pages: `DataTable` emits its own
  // table shell and has no hook that produces `.domain-table`, which means routing
  // them through it is precisely what made them look like a different product. The
  // same thing is already true of the admin Applications page, which is a re-export
  // of the console page — this gate never saw its authored table because that table
  // lives in the deployments repository.
  //
  // The exemption is therefore narrow and self-checking: exact paths, and the test
  // asserts each entry still exists *and* still authors a table, so a stale
  // exemption turns red instead of quietly widening the hole.
  const mirrored = [
    resolve(root, "packages/sdkwork-webserver-pc-admin-delivery/src/ServedDomainAdminSurface.tsx"),
    resolve(root, "packages/sdkwork-webserver-pc-admin-delivery/src/ServedCertificateAdminSurface.tsx"),
  ];
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
      .filter((path) => !mirrored.includes(path))
      .filter((path) => authored(readFileSync(path, "utf8")));
    expect(offenders).toEqual([]);
  });

  it("keeps every mirrored-page exemption justified", () => {
    // A mirror exemption that outlives the table it was granted for is a hole with
    // no occupant: the page would be free to hand-roll a table the composite could
    // have rendered. Re-check both halves here.
    for (const path of mirrored) {
      const source = readFileSync(path, "utf8");
      const authored = /<table[\s>]/.test(
        source
          .replace(/\/\*[\s\S]*?\*\//g, "")
          .replace(/(^|[^:])\/\/[^\n]*/g, "$1")
          .replace(/\{[^{}]*\}/g, ""),
      );
      expect(authored, `${path} no longer authors a table`).toBe(true);
      // And it must still be a *mirror*: the rendered scope class is what puts it
      // in the Deployments stylesheet, which is the only reason its table has the
      // console's look instead of the composite's.
      expect(source).toContain('className="deploy-surface"');
    }
  });
});
