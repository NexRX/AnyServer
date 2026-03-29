import { type Component, Show } from "solid-js";
import type { StopPhase, StopProgress } from "../../types/bindings";
import {
  computeShutdownPercent,
  computeGraceRemaining,
  computeGracePercent,
  formatShutdownCountdown,
} from "../../utils/format";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface StopProgressEntry {
  progress: StopProgress;
  receivedAt: number;
}

export interface ShutdownPanelProps {
  /** The stop progress entry from the WebSocket. */
  entry: StopProgressEntry;
  /** A reactive tick counter (incremented every second) to drive countdown updates. */
  tick: number;
  /** Whether the cancel-stop action is currently in flight. */
  cancellingStop: boolean;
  /** Called when the user clicks "Cancel Shutdown". */
  onCancelStop: () => void;
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function stopPhaseLabel(phase: StopPhase): string {
  switch (phase) {
    case "sending_stop_command":
      return "Sending stop command…";
    case "running_stop_steps":
      return "Running stop steps…";
    case "waiting_for_exit":
      return "Waiting for process to exit…";
    case "sending_sigkill":
      return "Sending SIGKILL…";
    case "cancelled":
      return "Shutdown cancelled";
    default:
      return "Stopping…";
  }
}

function isCancellable(phase: StopPhase | undefined): boolean {
  return phase !== undefined && phase !== "sending_sigkill" && phase !== "cancelled";
}

// ─── Component ──────────────────────────────────────────────────────────────

const ShutdownPanel: Component<ShutdownPanelProps> = (props) => {
  const phase = () => props.entry.progress.phase;
  const stepInfo = () => props.entry.progress.step_info;
  const graceSecs = () => props.entry.progress.grace_secs;

  const elapsed = () => {
    // Access tick to subscribe to the reactive timer
    void props.tick;
    const localDelta =
      Math.max(0, Date.now() - props.entry.receivedAt) / 1000;
    return props.entry.progress.elapsed_secs + localDelta;
  };

  const graceRemaining = () => {
    void props.tick;
    return computeGraceRemaining(
      graceSecs(),
      props.entry.receivedAt,
      Date.now(),
    );
  };

  const pct = () => {
    void props.tick;
    const p = phase();
    if (p === "running_stop_steps") {
      return computeShutdownPercent(
        props.entry.progress.elapsed_secs,
        props.entry.progress.timeout_secs,
        props.entry.receivedAt,
        Date.now(),
      );
    }
    if (p === "waiting_for_exit") {
      return computeGracePercent(
        graceSecs(),
        props.entry.receivedAt,
        Date.now(),
      );
    }
    if (p === "sending_sigkill") return 100;

    return computeShutdownPercent(
      props.entry.progress.elapsed_secs,
      props.entry.progress.timeout_secs,
      props.entry.receivedAt,
      Date.now(),
    );
  };

  const headerRight = (): string => {
    const p = phase();
    if (p === "sending_sigkill") return "Force killing…";
    if (p === "waiting_for_exit") {
      return `${formatShutdownCountdown(graceRemaining())} remaining`;
    }
    if (p === "running_stop_steps") {
      return `${formatShutdownCountdown(elapsed())} elapsed`;
    }
    return `${formatShutdownCountdown(elapsed())} elapsed`;
  };

  return (
    <div class="shutdown-panel">
      <div class="shutdown-panel-header">
        <span class="shutdown-phase-label">
          {stopPhaseLabel(phase())}
        </span>
        <span class="shutdown-countdown">
          {headerRight()}
        </span>
      </div>

      <Show when={phase() === "running_stop_steps" && stepInfo()}>
        {(info) => (
          <div class="shutdown-step-info">
            <span class="shutdown-step-counter">
              Step {info().index + 1}/{info().total}
            </span>
            <span class="shutdown-step-name">
              {info().name}
            </span>
            <Show when={info().step_timeout_secs != null}>
              <span class="shutdown-step-timeout">
                (up to {info().step_timeout_secs}s)
              </span>
            </Show>
          </div>
        )}
      </Show>

      <div class="shutdown-progress-bar-track">
        <div
          class={`shutdown-progress-bar-fill${phase() === "sending_sigkill" ? " sigkill" : ""}`}
          style={{ width: `${pct()}%` }}
        />
      </div>

      <Show when={isCancellable(phase())}>
        <button
          class="btn btn-warning shutdown-cancel-btn"
          onClick={props.onCancelStop}
          disabled={props.cancellingStop}
        >
          {props.cancellingStop ? (
            <span class="btn-spinner" />
          ) : (
            "✕"
          )}{" "}
          Cancel Shutdown
        </button>
      </Show>
    </div>
  );
};

export default ShutdownPanel;
