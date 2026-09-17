import type { ReactNode } from "react";
import { NavLink } from "react-router-dom";

import { cx } from "@sdkwork/webserver-h5-commons";

import type { WebserverH5NavigationEntry } from "../navigation/routeRegistry.ts";

/**
 * Mobile chrome every screen renders inside: a brand app bar, the screen body,
 * and a bottom tab bar built from the navigation model.
 *
 * The shell resolves **no** copy of its own — `brand`, `navigationLabel`, and
 * every tab label arrive as props the application root reads from the message
 * catalog, so localization stays a root concern. The screen title belongs to the
 * screen itself (`ScreenFrame`), which keeps one app bar and one screen header
 * instead of two competing ones. Styling uses the `h5-*` class layer defined by
 * the H5 application root, the same convention
 * `@sdkwork/webserver-h5-commons` follows.
 */
export interface WebserverH5AppShellProps {
  readonly brand: string;
  /** Accessible name of the tab-bar landmark. */
  readonly navigationLabel: string;
  readonly navigation: readonly WebserverH5NavigationEntry[];
  /** Resolves a navigation entry's catalog key into display copy. */
  readonly resolveLabel: (entry: WebserverH5NavigationEntry) => string;
  readonly children: ReactNode;
}

export function WebserverH5AppShell({
  brand,
  children,
  navigation,
  navigationLabel,
  resolveLabel,
}: WebserverH5AppShellProps) {
  return (
    <div className={cx("h5-app")}>
      <header className={cx("h5-app__header")}>
        <p className={cx("h5-app__brand")}>{brand}</p>
      </header>
      <main className={cx("h5-app__main")}>{children}</main>
      {navigation.length > 0 ? (
        <nav aria-label={navigationLabel} className={cx("h5-app__nav")}>
          <ul className={cx("h5-app__nav-list")}>
            {navigation.map((entry) => (
              <li key={entry.id} className={cx("h5-app__nav-item")}>
                <NavLink
                  to={entry.path}
                  className={({ isActive }) =>
                    cx("h5-app__nav-link", isActive && "h5-app__nav-link--active")
                  }
                >
                  {resolveLabel(entry)}
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
      ) : null}
    </div>
  );
}
