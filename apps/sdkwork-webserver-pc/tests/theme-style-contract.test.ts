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
    expect(workspaceStyles).toMatch(
      /\.table-frame\s*\{[^}]*background:\s*var\(--sdk-color-surface-panel\)/s,
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

  it("keeps application row operations compact, fixed, and theme-aware", () => {
    expect(stylesheet).toMatch(
      /\.row-actions-cell\s*\{[^}]*position:\s*sticky[^}]*background:\s*var\(--sdk-color-surface-panel\)/s,
    );
    expect(stylesheet).toMatch(
      /\.row-action-button\s*\{[^}]*width:\s*30px[^}]*height:\s*30px/s,
    );
    expect(stylesheet).toMatch(
      /\.row-action-button-danger:not\(:disabled\):hover\s*\{[^}]*background:\s*var\(--webserver-color-danger-surface\)/s,
    );
    expect(stylesheet).toMatch(
      /\.row-actions-column, \.row-actions-cell\s*\{[^}]*width:\s*236px[^}]*min-width:\s*236px/s,
    );
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
    expect(stylesheet).toMatch(
      /\.table-frame\s*\{[^}]*min-height:\s*0[^}]*overflow:\s*auto[^}]*overscroll-behavior:\s*contain/s,
    );
    expect(stylesheet).toMatch(
      /\.workspace > \*\s*\{[^}]*height:\s*100%/s,
    );
    expect(stylesheet).toMatch(
      /\.skills-console-surface,[\s\S]*\.plugins-admin-surface\s*\{[^}]*height:\s*100%[^}]*display:\s*flex[^}]*overflow:\s*hidden/s,
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
});
