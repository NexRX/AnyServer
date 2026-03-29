import { type Component, Show } from "solid-js";
import type { UpdateCheckResult } from "../../types/bindings";

export interface UpdateCheckBannerProps {
  /** The most recent update check result, or null if not yet checked. */
  result: UpdateCheckResult | null;
  /** Whether an update check is currently in progress. */
  checking: boolean;
  /** Called when the user clicks "Check for updates" or "Check again". */
  onCheck: (force?: boolean) => void;
}

const UpdateCheckBanner: Component<UpdateCheckBannerProps> = (props) => {
  const hasUpdate = () => props.result?.update_available ?? false;

  return (
    <div
      class="update-check-banner"
      classList={{
        "update-check-banner--available": hasUpdate(),
      }}
    >
      <Show
        when={props.result}
        fallback={
          <span class="update-check-banner-text update-check-banner-text--muted">
            Update checking is configured.{" "}
            <button
              class="btn btn-sm update-check-banner-btn"
              disabled={props.checking}
              onClick={() => props.onCheck()}
            >
              {props.checking ? "Checking…" : "Check for updates"}
            </button>
          </span>
        }
      >
        {(result) => (
          <>
            <Show when={result().error}>
              <span class="update-check-banner-text update-check-banner-text--error">
                ⚠ Check failed: {result().error}
              </span>
            </Show>
            <Show when={!result().error && result().update_available}>
              <span class="update-check-banner-text update-check-banner-text--warn">
                ⬆ Update available:{" "}
                <strong>{result().installed_version ?? "?"}</strong>
                {" → "}
                <strong>{result().latest_version ?? "?"}</strong>
              </span>
            </Show>
            <Show when={!result().error && !result().update_available}>
              <span class="update-check-banner-text update-check-banner-text--ok">
                ✓ Up to date
                <Show when={result().installed_version}>
                  {" "}
                  ({result().installed_version})
                </Show>
              </span>
            </Show>
            <button
              class="btn btn-sm update-check-banner-btn"
              disabled={props.checking}
              onClick={() => props.onCheck(true)}
            >
              {props.checking ? "Checking…" : "Check again"}
            </button>
          </>
        )}
      </Show>
    </div>
  );
};

export default UpdateCheckBanner;
