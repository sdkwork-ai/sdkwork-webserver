import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const stylesheet = readFileSync(resolve(root, "src/index.css"), "utf8");
const authStylesStart = stylesheet.indexOf(".webserver-auth-page {");
const workspaceStyles = stylesheet.slice(0, authStylesStart);

describe("webserver workspace theme styles", () => {
  it("does not mix theme text colors with fixed white surfaces", () => {
    const fixedWhiteBackgrounds = Array.from(
      workspaceStyles.matchAll(/background(?:-color)?\s*:\s*(?:white|#fff(?:fff)?)(?=\s*;)/gi),
      (match) => match[0],
    );

    expect(authStylesStart).toBeGreaterThan(0);
    expect(fixedWhiteBackgrounds).toEqual([]);
  });

  it("keeps shared workspace components on semantic theme tokens", () => {
    // The resource tables moved to the framework `DataTable`, which owns its own
    // surface chrome (border/background/shadow) in the composite. The host keeps
    // the semantic `--sdk-color-surface-panel` token on `.data-surface`, the frame
    // that wraps the table; `.table-frame` no longer exists here.
    expect(workspaceStyles).toMatch(
      /\.data-surface\s*\{[^}]*background:\s*var\(--sdk-color-surface-panel\)/s,
    );
    expect(workspaceStyles).toMatch(
      /\.dialog\s*\{[^}]*color:\s*var\(--sdk-color-text-primary\)[^}]*background:\s*var\(--sdk-color-surface-panel\)/s,
    );
    expect(workspaceStyles).toMatch(
      /\.form-grid input[^\{]*\{[^}]*background:\s*var\(--sdk-color-surface-panel-muted\)/s,
    );
    expect(workspaceStyles).toMatch(
      /\.command-button\s*\{[^}]*color:\s*var\(--webserver-color-on-accent\)[^}]*background:\s*var\(--webserver-color-command-background\)/s,
    );
    expect(workspaceStyles).toContain('html[data-sdk-color-mode="dark"] .webserver-pc-theme');
  });

  it("keeps application create and edit forms in the shared accessible left-side drawer", () => {
    expect(stylesheet).toMatch(
      /\.application-creation-drawer-backdrop\s*\{[^}]*display:\s*block[^}]*padding:\s*0/s,
    );
    expect(stylesheet).toMatch(
      /\.dialog\.application-creation-dialog\.application-creation-drawer\s*\{[^}]*inset:\s*0 auto 0 0[^}]*height:\s*100dvh[^}]*border-radius:\s*0/s,
    );
    expect(stylesheet).toMatch(
      /\.application-edit-drawer-content\s*\{[^}]*grid-template-rows:\s*minmax\(0, 1fr\) auto[^}]*overflow:\s*hidden/s,
    );
    expect(stylesheet).toMatch(
      /@media \(prefers-reduced-motion:\s*reduce\)[\s\S]*\.dialog\.application-creation-dialog\.application-creation-drawer\s*\{[^}]*animation:\s*none/s,
    );
    expect(stylesheet).toMatch(
      /\.application-creation-dialog \.application-release-fields label\s*\{[^}]*grid-template-columns:\s*1fr/s,
    );
    expect(stylesheet).toMatch(
      /\.application-creation-dialog \.application-review-grid\s*\{[^}]*grid-template-columns:\s*repeat\(2,/s,
    );
    expect(stylesheet).toMatch(
      /\.application-creation-dialog \.application-review-grid > div\.wide\s*\{[^}]*grid-column:\s*1 \/ -1/s,
    );
  });

  it("delegates row operations chrome to the framework DataTable", () => {
    // Row operations used to be host-owned: `.row-actions-column` / `.row-actions-cell`
    // / `.row-action-button` painted a hand-rolled sticky action cell. The tables now
    // pass `rowActions` to the framework `DataTable`, which owns that chrome, so the
    // stale selectors must stay gone rather than linger as dead rules.
    for (const dead of [
      "row-actions-column",
      "row-actions-cell",
      "row-action-button",
    ]) {
      expect(stylesheet).not.toContain(dead);
    }
  });

  it("keeps code updates in a focused centered modal", () => {
    expect(stylesheet).toMatch(
      /\.dialog\.source-update-dialog\s*\{[^}]*width:\s*min\(680px, 100%\)/s,
    );
    expect(stylesheet).toMatch(
      /\.source-update-dialog \.source-picker\s*\{[^}]*border-top:\s*1px solid var\(--sdk-color-border-default\)/s,
    );
  });

  it("maximizes every resource table within the available workspace height", () => {
    expect(stylesheet).toMatch(
      /\.app-layout\s*\{[^}]*height:\s*100dvh[^}]*grid-template-rows:[^}]*minmax\(0, 1fr\)[^}]*overflow:\s*hidden/s,
    );
    expect(stylesheet).toMatch(
      /\.resource-page\s*\{[^}]*height:\s*100%[^}]*display:\s*flex[^}]*flex-direction:\s*column[^}]*overflow:\s*hidden/s,
    );
    expect(stylesheet).toMatch(
      /\.data-surface\s*\{[^}]*flex:\s*1 1 auto[^}]*grid-template-rows:\s*minmax\(0, 1fr\) auto[^}]*overflow:\s*hidden/s,
    );
    // The scrolling viewport now belongs to the framework `Table` primitive
    // (`relative w-full overflow-auto`, `data-slot="table-viewport"`), so the host
    // only has to guarantee the frame gives it a bounded height to scroll within.
    expect(workspaceStyles).toMatch(
      /\.data-surface\s*\{[^}]*min-height:\s*0[^}]*flex:\s*1 1 auto/s,
    );
    expect(workspaceStyles).toMatch(
      /\.workspace > \*\s*\{[^}]*height:\s*100%/s,
    );
    expect(stylesheet).toMatch(
      /\.skills-console-surface,[\s\S]*\.storage-center-surface\s*\{[^}]*height:\s*100%[^}]*display:\s*flex[^}]*overflow:\s*hidden/s,
    );
    expect(stylesheet).toMatch(
      /\.plugin-source-toggle\s*\{[^}]*width:\s*max-content/s,
    );
    expect(stylesheet).toMatch(
      /\.skills-console-primary\s*\{[^}]*width:\s*max-content/s,
    );
    expect(stylesheet).toMatch(
      /\.skills-console-empty\s*\{[^}]*display:\s*flex[^}]*align-items:\s*center[^}]*justify-content:\s*center/s,
    );
  });

  it("gives the host-bridged cloud account page the same content gutter as the other embeds", () => {
    // `.workspace` is a bare grid cell — the rule above pins its `height: 100%`
    // child — and it sets no padding, so every host-bridged page carries its own
    // gutter. The cloud account page shipped without one, which in a real browser
    // measured as inset { top: 0, left: 0, right: 0 } against the workspace content
    // box: the section rule sat against the sidebar and the account table's right
    // border was cut off at the viewport edge.
    //
    // It deliberately takes the gutter *without* joining the two-scroll-region
    // group above: the page is a document (a listing plus a detail dialog), so it
    // scrolls as a whole. Before this rule the page was taller than the pane, and
    // the pane is `overflow: hidden` with no scrollable child, so the credentials
    // form at the bottom of it could not be reached.
    expect(stylesheet).toMatch(
      /\.cloud-account-surface\s*\{[^}]*overflow-y:\s*auto[^}]*padding:\s*18px clamp\(16px, 2\.2vw, 30px\) 24px/s,
    );
    expect(stylesheet).toMatch(
      /\.cloud-account-surface\s*\{[^}]*background:\s*var\(--sdk-color-surface-panel\)/s,
    );

    // A gutter rule with no consumer is dead CSS, and a wrapper class with no rule
    // is an invisible page — the defect was exactly the second one, so the pairing
    // is what this guards. (The geometry itself is verified in a real browser; this
    // is the part a build can fail on.)
    const surface = readFileSync(
      resolve(root, "packages/sdkwork-webserver-pc-console-cloud-account/src/CloudAccountManagementSurface.tsx"),
      "utf8",
    );
    expect(surface).toContain('className="cloud-account-surface"');
  });
});
