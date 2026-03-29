import { type Component, createSignal, Show, onCleanup } from "solid-js";
import type { ServerStatus } from "../types/bindings";

/**
 * Description and metadata for each server status value.
 */
const STATUS_INFO: Record<
  ServerStatus,
  { label: string; icon: string; description: string }
> = {
  stopped: {
    label: "Stopped",
    icon: "⏹",
    description:
      "The server process is not running. It can be started manually or will auto-start on boot if configured.",
  },
  starting: {
    label: "Starting",
    icon: "⏳",
    description:
      "The server process is being launched. It will transition to Running once the process is confirmed alive.",
  },
  running: {
    label: "Running",
    icon: "✅",
    description:
      "The server process is active and healthy. Console output is being streamed in real time.",
  },
  stopping: {
    label: "Stopping",
    icon: "⏸",
    description:
      "The server is shutting down gracefully. A stop command or SIGTERM has been sent and we're waiting for the process to exit.",
  },
  crashed: {
    label: "Crashed",
    icon: "💥",
    description:
      "The server process exited unexpectedly with a non-zero exit code. It will auto-restart if that option is enabled.",
  },
  installing: {
    label: "Installing",
    icon: "📦",
    description:
      "An install pipeline is running — downloading files, extracting archives, or running setup commands.",
  },
  updating: {
    label: "Updating",
    icon: "🔄",
    description:
      "An update pipeline is running — fetching the latest version and applying patches or replacements.",
  },
  uninstalling: {
    label: "Uninstalling",
    icon: "🗑",
    description:
      "An uninstall pipeline is running — cleaning up server files and removing installed artifacts.",
  },
};

const ALL_STATUSES: ServerStatus[] = [
  "stopped",
  "starting",
  "running",
  "stopping",
  "crashed",
  "installing",
  "updating",
  "uninstalling",
];

interface Props {
  /** The server's current status value */
  status: ServerStatus;
}

const StatusTooltip: Component<Props> = (props) => {
  const [open, setOpen] = createSignal(false);
  let containerRef: HTMLDivElement | undefined;

  const currentInfo = () => STATUS_INFO[props.status] ?? STATUS_INFO.stopped;

  const toggle = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setOpen((prev) => !prev);
  };

  // Close on outside click
  const handleClickOutside = (e: MouseEvent) => {
    if (containerRef && !containerRef.contains(e.target as Node)) {
      setOpen(false);
    }
  };

  // Close on escape
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && open()) {
      setOpen(false);
    }
  };

  if (typeof document !== "undefined") {
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    onCleanup(() => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    });
  }

  return (
    <div class="status-tooltip-container" ref={containerRef}>
      <button
        class="status-tooltip-trigger"
        onClick={toggle}
        aria-label="Status help — click to learn about server statuses"
        aria-expanded={open()}
        aria-haspopup="true"
        type="button"
      >
        ?
      </button>

      <Show when={open()}>
        <div class="status-tooltip-popover" role="tooltip">
          {/* Arrow */}
          <div class="status-tooltip-arrow" />

          {/* Current status highlight */}
          <div class="status-tooltip-current">
            <div class="status-tooltip-current-header">
              <span class="status-tooltip-current-icon">
                {currentInfo().icon}
              </span>
              <span class="status-tooltip-current-label">
                {currentInfo().label}
              </span>
              <span class="status-tooltip-current-tag">current</span>
            </div>
            <p class="status-tooltip-current-desc">
              {currentInfo().description}
            </p>
          </div>

          <div class="status-tooltip-divider" />

          {/* All statuses */}
          <div class="status-tooltip-list-header">All possible statuses</div>
          <ul class="status-tooltip-list">
            {ALL_STATUSES.map((s) => {
              const info = STATUS_INFO[s];
              const isCurrent = () => s === props.status;
              return (
                <li
                  class="status-tooltip-list-item"
                  classList={{ "status-tooltip-list-item--active": isCurrent() }}
                >
                  <span class="status-tooltip-list-icon">{info.icon}</span>
                  <span class="status-tooltip-list-label">{info.label}</span>
                  <span class="status-tooltip-list-desc">
                    {info.description}
                  </span>
                </li>
              );
            })}
          </ul>
        </div>
      </Show>
    </div>
  );
};

export default StatusTooltip;
