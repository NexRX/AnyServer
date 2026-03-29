import { type Component, Show } from "solid-js";

export interface LoaderProps {
  message?: string;
  compact?: boolean;
  /** Show a skeleton placeholder grid instead of the spinner */
  skeleton?: "cards" | "lines";
  /** Number of skeleton items to show */
  skeletonCount?: number;
}

const Loader: Component<LoaderProps> = (props) => {
  const count = () =>
    props.skeletonCount ?? (props.skeleton === "cards" ? 6 : 4);

  const displayMessage = () => props.message ?? "Loading";

  return (
    <Show
      when={!props.skeleton}
      fallback={
        <div class={`skeleton-container skeleton-${props.skeleton}`}>
          {Array.from({ length: count() }, (_, i) => (
            <div
              class="skeleton-item"
              style={{ "animation-delay": `${i * 0.08}s` }}
            >
              <Show when={props.skeleton === "cards"}>
                <div class="skeleton-bar skeleton-bar-short" />
                <div class="skeleton-bar skeleton-bar-medium" />
                <div class="skeleton-bar skeleton-bar-long" />
                <div class="skeleton-bar skeleton-bar-medium" />
              </Show>
              <Show when={props.skeleton === "lines"}>
                <div class="skeleton-bar skeleton-bar-full" />
              </Show>
            </div>
          ))}
        </div>
      }
    >
      <div
        class={`loading${props.compact ? " loading-compact" : ""}`}
        role="status"
        aria-live="polite"
      >
        <div class="loader-spinner-row">
          <div class="loader-spinner" aria-hidden="true">
            <svg viewBox="0 0 50 50" class="loader-svg">
              <circle
                class="loader-track"
                cx="25"
                cy="25"
                r="20"
                fill="none"
                stroke-width="4"
              />
              <circle
                class="loader-arc"
                cx="25"
                cy="25"
                r="20"
                fill="none"
                stroke-width="4"
                stroke-linecap="round"
              />
            </svg>
          </div>
          <span class="loader-message">{displayMessage()}</span>
        </div>
      </div>
    </Show>
  );
};

export default Loader;
