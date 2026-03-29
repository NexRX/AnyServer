/**
 * Console — Pure rendering component for the server console.
 *
 * This component is responsible ONLY for:
 * - Rendering log lines, separators, and phase progress
 * - Managing scroll behavior (auto-scroll, scroll-to-bottom)
 * - Handling command input, history, and submission
 * - Displaying connection status indicators
 *
 * It does NOT manage any WebSocket connections. All data flows in via props
 * from the `useServerConsole` hook, which is owned by the parent page
 * (ServerDetail). This separation eliminates the class of bugs caused by
 * coupling WebSocket lifecycle to component mount/unmount cycles.
 *
 * Previous architecture (buggy):
 *   Console owns WebSocket → unmount kills WS → stale close handlers →
 *   onDisconnect fires after unmount → banner flashes → refetch →
 *   remount → new WS → cycle repeats
 *
 * New architecture (robust):
 *   ServerDetail owns useServerConsole hook → hook owns WebSocket →
 *   Console receives data via props → mount/unmount has no effect on WS →
 *   no stale handlers → no banner flashing
 */

import {
  type Component,
  createSignal,
  createEffect,
  onCleanup,
  For,
  Show,
} from "solid-js";
import { sendCommand } from "../api/client";
import type { ServerStatus, PhaseProgress } from "../types/bindings";
import type { ConsoleLine } from "../hooks/useServerConsole";
import type { ConnectionState } from "../utils/ReconnectingWebSocket";
import { AutoScrollController } from "../utils/autoScroll";
import StatusTooltip from "./StatusTooltip";

// ─── Constants ──────────────────────────────────────────────────────────────

const KIND_LABELS: Record<string, string> = {
  start: "Pre-start",
  install: "Install",
  update: "Update",
  uninstall: "Uninstall",
};

// ─── Props ──────────────────────────────────────────────────────────────────

interface Props {
  /** Server ID — used for sending commands via HTTP. */
  serverId: string;

  /** Console log lines to render. Provided by useServerConsole. */
  lines: ConsoleLine[];

  /** Current connection state. Provided by useServerConsole. */
  connectionState: ConnectionState;

  /** Current server status string (e.g. "running", "stopped"). */
  serverStatus: string | null;

  /** Current phase progress (install/update/etc pipeline). */
  phaseProgress: PhaseProgress | null;

  /** Callback to clear console lines (calls hook's clearLines). */
  onClear: () => void;
}

// ─── Component ──────────────────────────────────────────────────────────────

const Console: Component<Props> = (props) => {
  // ── Local UI state (not related to WebSocket) ──
  const [input, setInput] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [commandHistory, setCommandHistory] = createSignal<string[]>([]);
  const [historyIndex, setHistoryIndex] = createSignal(-1);
  const [autoScrollEnabled, setAutoScrollEnabled] = createSignal(true);

  const scrollCtrl = new AutoScrollController();

  let outputRef: HTMLDivElement | undefined;
  let bottomRef: HTMLDivElement | undefined;
  let pointerDragging = false;

  // ── Auto-scroll ──

  const syncAutoScroll = () => {
    setAutoScrollEnabled(scrollCtrl.enabled);
  };

  const checkUserScrollPosition = () => {
    if (!outputRef) return;
    const { scrollTop, scrollHeight, clientHeight } = outputRef;
    scrollCtrl.handleUserScroll(scrollTop, scrollHeight, clientHeight);
    syncAutoScroll();
  };

  const handleWheel = () => {
    requestAnimationFrame(checkUserScrollPosition);
  };

  const handlePointerDown = () => {
    pointerDragging = true;
  };

  const handleScrollDuringDrag = () => {
    if (!pointerDragging) return;
    checkUserScrollPosition();
  };

  const handlePointerUp = () => {
    if (!pointerDragging) return;
    pointerDragging = false;
    requestAnimationFrame(checkUserScrollPosition);
  };

  if (typeof document !== "undefined") {
    document.addEventListener("pointerup", handlePointerUp);
  }
  onCleanup(() => {
    if (typeof document !== "undefined") {
      document.removeEventListener("pointerup", handlePointerUp);
    }
  });

  // Auto-scroll when lines change.
  createEffect(() => {
    // Track the lines array — this effect re-runs when lines change.
    const _lines = props.lines;
    if (_lines.length > 0) {
      requestAnimationFrame(() => {
        scrollCtrl.scrollToBottom(outputRef ?? null);
        syncAutoScroll();
      });
    }
  });

  // ── Command input ──

  const handleSend = async () => {
    const cmd = input().trim();
    if (!cmd || sending()) return;

    setSending(true);
    try {
      await sendCommand(props.serverId, cmd);
      setCommandHistory((prev) => {
        const next = [...prev.filter((c) => c !== cmd), cmd];
        return next.length > 50 ? next.slice(-50) : next;
      });
      setHistoryIndex(-1);
      setInput("");
    } catch (e: unknown) {
      console.error("[Console] Failed to send command:", e);
    } finally {
      setSending(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      const history = commandHistory();
      if (history.length === 0) return;
      const newIndex =
        historyIndex() === -1
          ? history.length - 1
          : Math.max(0, historyIndex() - 1);
      setHistoryIndex(newIndex);
      setInput(history[newIndex]);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      const history = commandHistory();
      if (historyIndex() === -1) return;
      const newIndex = historyIndex() + 1;
      if (newIndex >= history.length) {
        setHistoryIndex(-1);
        setInput("");
      } else {
        setHistoryIndex(newIndex);
        setInput(history[newIndex]);
      }
    }
  };

  // ── Formatting helpers ──

  const formatTimestamp = (ts: string): string => {
    try {
      return new Date(ts).toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
    } catch {
      return "";
    }
  };

  const phaseLabel = (): string | null => {
    const p = props.phaseProgress;
    if (!p) return null;
    const kind = KIND_LABELS[p.phase] ?? p.phase;
    if (p.status === "running") {
      const total = p.steps.length;
      const done = p.steps.filter(
        (s) =>
          s.status === "completed" ||
          s.status === "failed" ||
          s.status === "skipped",
      ).length;
      const current = p.steps.find((s) => s.status === "running");
      if (current) {
        return `${kind}: step ${done + 1}/${total} — ${current.step_name}`;
      }
      return `${kind}: ${done}/${total} steps`;
    }
    if (p.status === "completed") return `${kind}: completed`;
    if (p.status === "failed") return `${kind}: failed`;
    return null;
  };

  const isPhaseActive = () => {
    const p = props.phaseProgress;
    return p != null && p.status === "running";
  };

  const connectionInfo = (): {
    label: string;
    color: string;
    pulse: boolean;
  } => {
    switch (props.connectionState) {
      case "connected":
        return { label: "Connected", color: "#22c55e", pulse: false };
      case "connecting":
        return { label: "Connecting…", color: "#eab308", pulse: true };
      case "reconnecting":
        return { label: "Reconnecting…", color: "#eab308", pulse: true };
      case "disconnected":
        return { label: "Disconnected", color: "#ef4444", pulse: false };
      case "idle":
        return { label: "Idle", color: "#6b7280", pulse: false };
      case "closed":
        return { label: "Closed", color: "#6b7280", pulse: false };
    }
  };

  const emptyMessage = (): string => {
    const state = props.connectionState;
    if (state === "connecting" || state === "reconnecting")
      return "Connecting to server console…";
    if (state === "disconnected") return "Unable to connect. Retrying…";
    if (state === "idle") return "Initializing…";
    const p = props.phaseProgress;
    if (p && p.status === "running") {
      return "Waiting for pipeline output…";
    }
    return "No output yet. The console will display log output once the server is running.";
  };

  // ── Render ──

  return (
    <div class="console">
      <div class="console-toolbar">
        <div class="console-status">
          <span
            class="status-dot"
            classList={{ "status-dot-pulse": connectionInfo().pulse }}
            style={{
              background: connectionInfo().color,
              width: "8px",
              height: "8px",
              "border-radius": "50%",
              display: "inline-block",
            }}
          />
          <span style={{ "font-size": "0.8rem", color: "#9ca3af" }}>
            {connectionInfo().label}
          </span>
          <Show when={props.serverStatus}>
            {(status) => (
              <>
                <span
                  class={`status-badge status-${status()}`}
                  style={{ "margin-left": "0.5rem" }}
                >
                  {status()}
                </span>
                <StatusTooltip status={status() as ServerStatus} />
              </>
            )}
          </Show>
          <Show when={phaseLabel()}>
            {(label) => (
              <span
                class={`status-badge ${isPhaseActive() ? "status-starting" : props.phaseProgress?.status === "completed" ? "status-running" : "status-crashed"}`}
                style={{ "margin-left": "0.5rem" }}
              >
                {label()}
              </span>
            )}
          </Show>
        </div>
        <div style={{ display: "flex", gap: "0.4rem" }}>
          <button class="btn btn-sm" onClick={props.onClear}>
            Clear Console
          </button>
          <button
            class="btn btn-sm"
            onClick={() => {
              scrollCtrl.userScrollToBottom(bottomRef ?? null);
              syncAutoScroll();
            }}
            disabled={autoScrollEnabled()}
          >
            Scroll to Bottom
          </button>
        </div>
      </div>

      <Show when={isPhaseActive() && props.phaseProgress}>
        {(progress) => {
          const total = () => progress().steps.length;
          const done = () =>
            progress().steps.filter(
              (s) =>
                s.status === "completed" ||
                s.status === "failed" ||
                s.status === "skipped",
            ).length;
          const pct = () =>
            total() > 0 ? Math.round((done() / total()) * 100) : 0;

          return (
            <div class="console-progress-bar">
              <span>
                Step {done()}/{total()}
              </span>
              <div class="console-progress-track">
                <div
                  class="console-progress-fill"
                  style={{ width: `${pct()}%` }}
                />
              </div>
              <span>{pct()}%</span>
            </div>
          );
        }}
      </Show>

      <div
        class="console-output"
        ref={outputRef}
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
        onScroll={handleScrollDuringDrag}
      >
        <Show
          when={props.lines.length > 0}
          fallback={<div class="console-empty">{emptyMessage()}</div>}
        >
          <For each={props.lines}>
            {(line) => (
              <Show
                when={line.kind !== "separator"}
                fallback={
                  <div
                    class={`console-separator ${line.stream === "stderr" ? "console-separator-error" : ""}`}
                  >
                    <span class="console-separator-line" />
                    <span class="console-separator-label">{line.line}</span>
                    <span class="console-separator-line" />
                  </div>
                }
              >
                <div
                  class={`console-line stream-${line.stream}`}
                  classList={{ "phase-line": !!line.phase }}
                >
                  <span class="timestamp">
                    {formatTimestamp(line.timestamp)}
                  </span>
                  <Show when={line.phase && line.stepName}>
                    <span class="phase-tag">
                      [{line.phase}:{line.stepName}]
                    </span>
                  </Show>
                  <span class="text">{line.line}</span>
                </div>
              </Show>
            )}
          </For>
        </Show>
        <div ref={bottomRef} />
      </div>

      <div class="console-input">
        <span class="console-prompt">&gt;</span>
        <input
          type="text"
          value={input()}
          onInput={(e) => {
            setInput(e.currentTarget.value);
            setHistoryIndex(-1);
          }}
          onKeyDown={handleKeyDown}
          placeholder="Type a command and press Enter…"
          disabled={sending()}
        />
        <button
          class="btn btn-primary"
          onClick={handleSend}
          disabled={sending() || !input().trim()}
        >
          {sending() ? "…" : "Send"}
        </button>
      </div>
    </div>
  );
};

export default Console;
