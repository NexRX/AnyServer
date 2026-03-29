import { type Component, createSignal, onCleanup, Show } from "solid-js";
import { cancelRestart } from "../api/client";
import type { ServerRuntime } from "../types/bindings";

interface Props {
  serverId: string;
  runtime: ServerRuntime;
  maxAttempts: number;
  restartDelaySecs: number;
  onCancelled?: () => void;
  compact?: boolean;
}

const RestartBanner: Component<Props> = (props) => {
  const [cancelling, setCancelling] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [tick, setTick] = createSignal(0);

  // Update tick every 100ms for smooth progress animation
  const tickInterval = setInterval(() => setTick((t) => t + 1), 100);
  onCleanup(() => clearInterval(tickInterval));

  const remainingSeconds = () => {
    tick(); // Force reactivity
    if (!props.runtime.next_restart_at) return 0;
    const now = Date.now();
    const target = new Date(props.runtime.next_restart_at).getTime();
    return Math.max(0, (target - now) / 1000);
  };

  const progressPercent = () => {
    const remaining = remainingSeconds();
    const total = props.restartDelaySecs;
    if (total <= 0 || remaining <= 0) return 100;
    const elapsed = total - remaining;
    return Math.min(100, Math.max(0, (elapsed / total) * 100));
  };

  const handleCancel = async () => {
    if (cancelling()) return;
    setCancelling(true);
    setError(null);

    try {
      await cancelRestart(props.serverId);
      props.onCancelled?.();
    } catch (err) {
      console.error("Failed to cancel restart:", err);
      setError(err instanceof Error ? err.message : "Failed to cancel restart");
    } finally {
      setCancelling(false);
    }
  };

  const formatTime = (seconds: number): string => {
    if (seconds < 1) return "0s";
    const s = Math.ceil(seconds);
    if (s < 60) return `${s}s`;
    const mins = Math.floor(s / 60);
    const secs = s % 60;
    return `${mins}m ${secs}s`;
  };

  return (
    <Show when={props.runtime.next_restart_at}>
      <div
        class={props.compact ? "restart-banner-compact" : "restart-banner"}
        style={{
          display: "flex",
          "align-items": "center",
          gap: props.compact ? "0.5rem" : "1rem",
          padding: props.compact ? "0.5rem 0.75rem" : "1rem 1.25rem",
          background: "linear-gradient(135deg, #ef4444 0%, #dc2626 100%)",
          border: "1px solid rgba(239, 68, 68, 0.3)",
          "border-radius": props.compact ? "6px" : "8px",
          "box-shadow": props.compact
            ? "0 2px 8px rgba(239, 68, 68, 0.2)"
            : "0 4px 12px rgba(239, 68, 68, 0.3)",
          "margin-bottom": props.compact ? "0.5rem" : "1rem",
          position: "relative",
          overflow: "hidden",
        }}
      >
        {/* Progress bar background */}
        <div
          style={{
            position: "absolute",
            top: "0",
            left: "0",
            bottom: "0",
            width: `${progressPercent()}%`,
            background:
              "linear-gradient(90deg, rgba(220, 38, 38, 0.4) 0%, rgba(239, 68, 68, 0.2) 100%)",
            transition: "width 0.1s linear",
            "z-index": "0",
          }}
        />

        {/* Content */}
        <div
          style={{
            position: "relative",
            "z-index": "1",
            display: "flex",
            "align-items": "center",
            gap: props.compact ? "0.5rem" : "1rem",
            flex: "1",
          }}
        >
          {/* Circular progress indicator */}
          <div
            style={{
              position: "relative",
              width: props.compact ? "32px" : "40px",
              height: props.compact ? "32px" : "40px",
              "flex-shrink": "0",
            }}
          >
            <svg
              width="100%"
              height="100%"
              viewBox="0 0 40 40"
              style={{
                transform: "rotate(-90deg)",
              }}
            >
              {/* Background circle */}
              <circle
                cx="20"
                cy="20"
                r="16"
                fill="none"
                stroke="rgba(255, 255, 255, 0.2)"
                stroke-width="3"
              />
              {/* Progress circle */}
              <circle
                cx="20"
                cy="20"
                r="16"
                fill="none"
                stroke="white"
                stroke-width="3"
                stroke-dasharray={`${2 * Math.PI * 16}`}
                stroke-dashoffset={`${2 * Math.PI * 16 * (1 - progressPercent() / 100)}`}
                stroke-linecap="round"
                style={{
                  transition: "stroke-dashoffset 0.1s linear",
                }}
              />
            </svg>
            <div
              style={{
                position: "absolute",
                top: "50%",
                left: "50%",
                transform: "translate(-50%, -50%)",
                "font-size": props.compact ? "10px" : "11px",
                "font-weight": "600",
                color: "white",
              }}
            >
              ↻
            </div>
          </div>

          {/* Text content */}
          <div style={{ flex: "1", "min-width": "0" }}>
            <div
              style={{
                "font-weight": "600",
                color: "white",
                "font-size": props.compact ? "0.875rem" : "0.9375rem",
                "margin-bottom": props.compact ? "0.125rem" : "0.25rem",
              }}
            >
              Auto-restart pending
            </div>
            <div
              style={{
                "font-size": props.compact ? "0.75rem" : "0.8125rem",
                color: "rgba(255, 255, 255, 0.9)",
                display: "flex",
                "align-items": "center",
                gap: "0.5rem",
                "flex-wrap": "wrap",
              }}
            >
              <span>
                Attempt {props.runtime.restart_count + 1} of {props.maxAttempts}
              </span>
              <span style={{ color: "rgba(255, 255, 255, 0.6)" }}>•</span>
              <span>Restarting in {formatTime(remainingSeconds())}</span>
            </div>
            <Show when={error()}>
              <div
                style={{
                  "font-size": "0.75rem",
                  color: "rgba(255, 255, 255, 0.95)",
                  "margin-top": "0.25rem",
                  "font-weight": "500",
                }}
              >
                ⚠ {error()}
              </div>
            </Show>
          </div>

          {/* Cancel button */}
          <button
            class="btn btn-sm"
            onClick={handleCancel}
            disabled={cancelling()}
            style={{
              background: "rgba(255, 255, 255, 0.95)",
              color: "#dc2626",
              border: "none",
              "font-weight": "600",
              "font-size": props.compact ? "0.75rem" : "0.8125rem",
              padding: props.compact ? "0.375rem 0.75rem" : "0.5rem 1rem",
              "white-space": "nowrap",
              "flex-shrink": "0",
            }}
            title="Cancel auto-restart and keep server in crashed state"
          >
            {cancelling() ? (
              <>
                <span class="btn-spinner" style={{ color: "#dc2626" }} />
                Cancelling...
              </>
            ) : (
              <>✕ Cancel</>
            )}
          </button>
        </div>
      </div>
    </Show>
  );
};

export default RestartBanner;
