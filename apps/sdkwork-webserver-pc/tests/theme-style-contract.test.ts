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
      /\.data-surface\s*\{[^}]*flex:\s*1 1 auto[^}]*display:\s*flex[^}]*flex-direction:\s*column[^}]*overflow:\s*hidden/s,
    );
    // The frame used to be a two-row grid (`grid-template-rows: minmax(0, 1fr)
    // auto`). That model only held while the table was the frame's sole child —
    // the `1fr` first track went to whichever child came first, which is how two
    // console pages ended up abandoning the class for a frame of their own and a
    // third added a scoped `display: flex` override. The grid is the defect this
    // assertion retires, so it must not come back: a page that wants to put a
    // toolbar in the frame should not have to re-derive that.
    expect(stylesheet).not.toMatch(/\.data-surface\s*\{[^}]*grid-template-rows/s);
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

  it("draws the list panel frame once and lets the table fill the pane", () => {
    // Each list page wraps the framework `DataTable` in `.data-surface`, and both
    // of them paint the same border + radius + panel background + `--sdk-shadow-sm`.
    // `.data-surface` carries no padding, so the composite's frame landed 1px
    // inside the host's and the page showed a doubled 2px outline over two
    // overlapping rounded corners. Measured in a real browser against the product's
    // own compiled stylesheets: host 987.69x691.16 against composite 985.69x192,
    // identical `borderColor`, identical 6px radius.
    //
    // The same rules carry the height chain. Only the host `flex` / `min-height`
    // below is old; without the rest an empty listing collapsed to a 192px box in a
    // 691px panel, and a twenty-row listing had its 1459.61px viewport clipped by
    // the composite's `overflow: hidden` — ten rows and the pagination footer were
    // not mis-sized, they were unreachable. `flex: 1 1 auto` + `min-height: 0` on
    // the composite and on the framework's own scrolling viewport is what turns that
    // clipping into a scrollable table, so these three selectors are one contract
    // rather than three independent tricks.
    expect(workspaceStyles).toMatch(
      /\.data-surface\s*>\s*\[data-slot="data-table"\]\s*\{[^}]*flex:\s*1 1 auto[^}]*min-height:\s*0/s,
    );
    expect(workspaceStyles).toMatch(
      /\.data-surface\s+\[data-sdk-region="data-table-surface"\]\s*\{[^}]*display:\s*flex[^}]*flex-direction:\s*column[^}]*min-height:\s*0[^}]*border:\s*0[^}]*border-radius:\s*0[^}]*background:\s*transparent[^}]*box-shadow:\s*none/s,
    );
    expect(workspaceStyles).toMatch(
      /\.data-surface\s+\[data-slot="table-viewport"\]\s*\{[^}]*flex:\s*1 1 auto[^}]*min-height:\s*0/s,
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

  it("keeps the plugin filter card out of the table's frame and inside half the pane", () => {
    // The plugin page renders its filter bar as a card (border, 10px radius,
    // panel background). As the first child of `.data-surface` — which carries no
    // padding — that card's border landed 1px inside the frame's own, the same
    // doubled outline the table surface used to paint: measured in a real browser
    // at 985.69x573 on (265.16, 187.84) against the frame's 987.69x689.16 on
    // (264.16, 186.84), identical border colour. It also spent its whole chip
    // matrix out of the table's height, leaving a twenty-row table 102px of a
    // 689px pane. The page now keeps the card above the frame like every other
    // console list page; the cap is what keeps the table's half of the pane when
    // the chip matrix is long, and the bar scrolls past it rather than hiding a
    // chip. The sibling relationship itself is asserted in
    // `plugins-list-frame.test.tsx`, which renders the page.
    expect(stylesheet).toMatch(
      /\.plugin-filter-bar\s*\{[^}]*max-height:\s*50%[^}]*overflow-y:\s*auto/s,
    );
  });

  it("gives the native select popup a themed surface from the base layer", () => {
    // The native `<select>` list is painted by the browser, outside this sheet's
    // cascade. It resolves its surface from the select's own `background-color`
    // when that is opaque, otherwise from the option's, otherwise from the UA
    // light canvas — and `color-scheme: dark` on the root does not repaint it.
    // Tailwind's preflight sets `background-color: transparent` on every `select`,
    // so anything that never received an opaque surface — an unstyled `<select>`
    // in a table cell, one dressed with `bg-transparent`, a drawer form field —
    // opened a `#ffffff` list whose option text was still `#fafafa`. Measured in a
    // real browser: those popups sampled `#ffffff` before and `#18181b` after.
    //
    // The rule has to sit in `base`: the scoped `option` rules above (and every
    // unlayered rule, e.g. the deployments dialogs' own `--pda-*` surface) must
    // still win, so this only fills the gap the preflight left. It is not a
    // hardcoded palette — the pair of theme tokens is the assertion.
    expect(workspaceStyles).toMatch(
      /@layer\s+base\s*\{\s*select\s+option,\s*select\s+optgroup\s*\{[^}]*color:\s*var\(--sdk-color-text-primary\)[^}]*background:\s*var\(--sdk-color-surface-panel\)/s,
    );
  });

  it("only ever paints the scrim token behind a surface, never under a word", () => {
    // `--sdk-color-surface-overlay` is a scrim, not a surface: the theme defines
    // it as `rgba(9, 9, 11, 0.45)` in *both* modes (sdkwork-ui
    // `src/styles/sdkwork-ui.css`), because its one job is to darken whatever is
    // behind a dialog. So it is right behind a full-viewport backdrop and wrong
    // behind text. The statistics metric row's "not assembled here" chip took it
    // as a fill, composited to #8a8a8c over the muted card, and then carried
    // `--sdk-color-text-secondary` (#3f3f46) at 3.04:1 — under the 4.5:1 floor
    // for 11px text, and under it in light mode, the mode this product ships.
    //
    // The invariant is mechanical, which is why it is a gate and not a review
    // note: every rule naming the token must also be a fixed full-viewport
    // backdrop. Read off the real stylesheets rather than restated.
    const backdrops: string[] = [];
    const offenders: string[] = [];
    for (const sheet of ["src/index.css", "src/deploy-surface.css"]) {
      const css = readFileSync(resolve(root, sheet), "utf8");
      for (const [, selector, body] of css.matchAll(
        /([^{}]+)\{([^{}]*--sdk-color-surface-overlay[^{}]*)\}/g,
      )) {
        const rule = `${sheet}: ${selector.trim().split(/\r?\n/).pop()?.trim()}`;
        if (/position:\s*fixed/.test(body) && /inset:\s*0/.test(body)) {
          backdrops.push(rule);
        } else {
          offenders.push(rule);
        }
      }
    }

    expect(offenders).toEqual([]);
    // A gate that matches nothing passes for the wrong reason. The three known
    // scrims are the two dialog backdrops (`src/index.css`, `deploy-surface.css`)
    // and the delivery drawer, so a rename that emptied this loop must fail
    // here rather than let the assertion above go quietly vacuous.
    expect(backdrops).toHaveLength(3);
  });
});
