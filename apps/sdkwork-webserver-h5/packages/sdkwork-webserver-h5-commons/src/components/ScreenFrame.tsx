import type { ReactNode } from "react";

import { cx } from "../cx";

export interface ScreenFrameProps {
  /** Localized eyebrow label rendered above the title. */
  eyebrow?: string;
  title: string;
  description?: string;
  /** Right-aligned header action slot. */
  action?: ReactNode;
  children: ReactNode;
}

/**
 * Mobile screen chrome: a sticky header plus a scrollable body. Every H5
 * feature screen renders inside one so the console keeps a single layout
 * contract on small viewports.
 */
export function ScreenFrame({ action, children, description, eyebrow, title }: ScreenFrameProps) {
  return (
    <section className={cx("h5-screen")}>
      <header className={cx("h5-screen__header")}>
        <div className={cx("h5-screen__heading")}>
          {eyebrow ? <p className={cx("h5-screen__eyebrow")}>{eyebrow}</p> : null}
          <h1 className={cx("h5-screen__title")}>{title}</h1>
          {description ? <p className={cx("h5-screen__lede")}>{description}</p> : null}
        </div>
        {action ? <div className={cx("h5-screen__action")}>{action}</div> : null}
      </header>
      <div className={cx("h5-screen__body")}>{children}</div>
    </section>
  );
}
