import { cx } from "../cx";

export type StateBlockTone = "empty" | "error" | "loading";

export interface StateBlockProps {
  tone: StateBlockTone;
  message: string;
  /** Optional retry affordance, rendered only when a callback is supplied. */
  onRetry?: () => void;
  retryLabel?: string;
}

/**
 * Single placeholder for the three non-content states a mobile list can be in.
 * Keeping them in one component stops each feature screen from inventing its
 * own loading/empty/error markup.
 */
export function StateBlock({ message, onRetry, retryLabel, tone }: StateBlockProps) {
  return (
    <div
      className={cx("h5-state", `h5-state--${tone}`)}
      role={tone === "error" ? "alert" : "status"}
    >
      <p className={cx("h5-state__message")}>{message}</p>
      {onRetry && retryLabel ? (
        <button className={cx("h5-state__retry")} type="button" onClick={onRetry}>
          {retryLabel}
        </button>
      ) : null}
    </div>
  );
}
