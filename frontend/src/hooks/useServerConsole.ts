/**
 * useServerConsole — High-level SolidJS hook for server console WebSocket.
 *
 * This hook owns the entire lifecycle of a per-server WebSocket connection
 * and exposes reactive signals for everything the Console component and
 * its parent (ServerDetail) need. It replaces:
 *
 * - The WebSocket connection/reconnection logic that was in Console.tsx
 * - The message dispatch logic that was in Console.tsx
 * - The `onDisconnect`/`onReconnect` callback pattern between Console → ServerDetail
 * - The `wsConnected` signal + debounce timer in ServerDetail
 *
 * By lifting connection management into a hook, the Console component becomes
 * a pure rendering component that cannot cause stale-handler bugs, and the
 * parent page gets direct access to connection state without intermediary
 * callbacks.
 *
 * Usage (in ServerDetail.tsx):
 *
 *   const console = useServerConsole(() => params.id);
 *
 *   // Pass to Console for rendering:
 *   <Console
 *     lines={console.lines()}
 *     connectionState={console.connectionState()}
 *     serverStatus={console.serverStatus()}
 *     phaseProgress={console.phaseProgress()}
 *     serverId={params.id}
 *   />
 *
 *   // Use in the page directly:
 *   <ConnectionBanner state={console.connectionState()} />
 *
 *   // Read runtime/progress for the page header:
 *   const runtime = console.runtime();
 */

import { createSignal, type Accessor, createEffect, on, batch } from "solid-js";
import { getWsTicket } from "../api/auth";
import { useWebSocket } from "./useWebSocket";
import type {
  ConnectionState,
  ReconnectSchedule,
} from "../utils/ReconnectingWebSocket";
import type {
  WsMessage,
  ServerRuntime,
  PhaseProgress,
  PhaseLogLine,
  StopProgress,
  LogLine,
} from "../types/bindings";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface ConsoleLine {
  timestamp: string;
  line: string;
  stream: "stdout" | "stderr";
  phase?: "start" | "install" | "update" | "uninstall";
  stepName?: string;
  kind?: "separator";
}

export interface UseServerConsoleOptions {
  /**
   * Called when the WebSocket reconnects after a disconnect.
   * Typically used to trigger a data refetch so the parent page's
   * HTTP-fetched data stays in sync with the live WebSocket state.
   */
  onReconnect?: () => void;
}

export interface UseServerConsoleReturn {
  // ── Connection ──
  /** Reactive connection state (idle | connecting | connected | ...). */
  connectionState: Accessor<ConnectionState>;
  /** Shorthand: connectionState() === "connected". */
  isConnected: Accessor<boolean>;
  /**
   * Reactive reconnect schedule info. Non-null when a reconnect attempt is
   * pending, null when connected or idle. Useful for displaying a countdown
   * timer in the ConnectionBanner.
   */
  reconnectInfo: Accessor<ReconnectSchedule | null>;

  // ── Console data ──
  /** Accumulated console lines (capped at MAX_DISPLAY_LINES). */
  lines: Accessor<ConsoleLine[]>;
  /** Last known server status string (e.g. "running", "stopped"). */
  serverStatus: Accessor<string | null>;
  /** Current phase progress (install/update/uninstall/start pipeline). */
  phaseProgress: Accessor<PhaseProgress | null>;

  // ── Data for the parent page ──
  /** Full ServerRuntime object from the last StatusChange message. */
  runtime: Accessor<ServerRuntime | null>;
  /** Stop progress for graceful shutdown UI. */
  stopProgress: Accessor<{ progress: StopProgress; receivedAt: number } | null>;

  // ── Actions ──
  /** Clear all accumulated console lines. */
  clearLines: () => void;
  /** Force an immediate reconnect (e.g. after a server reset). */
  reconnectNow: () => void;
}

// ─── Constants ──────────────────────────────────────────────────────────────

const MAX_DISPLAY_LINES = 800;

const KIND_LABELS: Record<string, string> = {
  start: "Pre-start",
  install: "Install",
  update: "Update",
  uninstall: "Uninstall",
};

const STATUS_SEPARATOR_LABELS: Record<string, string> = {
  running: "▶ Server is running",
  starting: "⏳ Server is starting…",
  stopped: "■ Server stopped",
  crashed: "✗ Server crashed",
  stopping: "⏳ Server is stopping…",
};

const INTERESTING_STATUSES = new Set([
  "running",
  "stopped",
  "crashed",
  "starting",
]);

// ─── URL Factory ────────────────────────────────────────────────────────────

/**
 * Build a WebSocket URL factory for a given server ID.
 * Each invocation fetches a fresh one-time authentication ticket.
 */
function makeUrlFactory(serverId: string): () => Promise<string> {
  return async () => {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    const path = `/api/servers/${encodeURIComponent(serverId)}/ws`;
    const { ticket } = await getWsTicket(path);
    return `${proto}//${location.host}${path}?ticket=${encodeURIComponent(ticket)}`;
  };
}

// ─── Hook ───────────────────────────────────────────────────────────────────

export function useServerConsole(
  serverId: Accessor<string>,
  options?: UseServerConsoleOptions,
): UseServerConsoleReturn {
  // ── Signals ──
  const [lines, setLines] = createSignal<ConsoleLine[]>([]);
  const [serverStatus, setServerStatus] = createSignal<string | null>(null);
  const [phaseProgress, setPhaseProgress] = createSignal<PhaseProgress | null>(
    null,
  );
  const [runtime, setRuntime] = createSignal<ServerRuntime | null>(null);
  const [stopProgress, setStopProgress] = createSignal<{
    progress: StopProgress;
    receivedAt: number;
  } | null>(null);

  // ── Mutable tracking (not reactive — only used in message handler) ──
  let prevStatus: string | null = null;
  let lastAnnouncedPhaseId: string | null = null;

  // ── Sequence-based deduplication ──
  // `lastSeq` tracks the highest sequence number we've seen so far.
  // After a WebSocket reconnect the backend replays its log buffer which
  // may overlap with lines we already have. Lines with seq <= lastSeq are
  // duplicates and are silently dropped.
  let lastSeq = -1;

  // ── Line accumulation ──

  const appendLines = (newLines: ConsoleLine[]) => {
    setLines((prev) => {
      const next = [...prev, ...newLines];
      return next.length > MAX_DISPLAY_LINES
        ? next.slice(-MAX_DISPLAY_LINES)
        : next;
    });
  };

  const maybeInjectSeparator = (newStatus: string) => {
    if (
      prevStatus &&
      prevStatus !== newStatus &&
      INTERESTING_STATUSES.has(newStatus)
    ) {
      const label =
        STATUS_SEPARATOR_LABELS[newStatus] ?? `Status: ${newStatus}`;
      appendLines([
        {
          timestamp: new Date().toISOString(),
          line: label,
          stream: newStatus === "crashed" ? "stderr" : "stdout",
          kind: "separator",
        },
      ]);
    }
    prevStatus = newStatus;
  };

  const phaseLogToLine = (pl: PhaseLogLine): ConsoleLine => ({
    timestamp: pl.timestamp,
    line: pl.line,
    stream: pl.stream,
    phase: pl.phase as "start" | "install" | "update" | "uninstall",
    stepName: pl.step_name,
  });

  // ── Message handler ──

  const handleMessage = (raw: unknown) => {
    let msg: WsMessage;
    try {
      msg = typeof raw === "string" ? JSON.parse(raw) : (raw as WsMessage);
    } catch (e) {
      console.error("[useServerConsole] Failed to parse message:", e, raw);
      return;
    }

    switch (msg.type) {
      case "Log": {
        const logData = msg.data as LogLine;
        const seq = logData.seq;

        // Deduplicate: skip lines we've already seen (replay after reconnect).
        if (seq <= lastSeq) {
          break;
        }

        // Gap detection: if we skipped sequence numbers, insert a marker.
        if (lastSeq >= 0 && seq > lastSeq + 1) {
          const gap = seq - lastSeq - 1;
          appendLines([
            {
              timestamp: new Date().toISOString(),
              line: `--- ${gap} line(s) may be missing ---`,
              stream: "stderr",
              kind: "separator",
            },
          ]);
        }

        lastSeq = seq;
        appendLines([
          {
            timestamp: logData.timestamp,
            line: logData.line,
            stream: logData.stream,
          },
        ]);
        break;
      }

      case "StatusChange":
        batch(() => {
          maybeInjectSeparator(msg.data.status);
          setServerStatus(msg.data.status);
          setRuntime(msg.data);
          // Clear stop progress when no longer stopping.
          if (msg.data.status !== "stopping") {
            setStopProgress(null);
          }
        });
        break;

      case "PhaseLog":
        appendLines([phaseLogToLine(msg.data)]);
        break;

      case "PhaseProgress": {
        setPhaseProgress(msg.data);

        const progress = msg.data;
        if (progress.status === "completed" || progress.status === "failed") {
          const phaseId = `${progress.server_id}:${progress.phase}:${progress.status}:${progress.completed_at ?? ""}`;
          if (phaseId !== lastAnnouncedPhaseId) {
            lastAnnouncedPhaseId = phaseId;
            const icon = progress.status === "completed" ? "✓" : "✗";
            const label = KIND_LABELS[progress.phase] ?? progress.phase;
            appendLines([
              {
                timestamp: progress.completed_at ?? new Date().toISOString(),
                line: `${icon} ${label} pipeline ${progress.status}.`,
                stream: progress.status === "completed" ? "stdout" : "stderr",
                phase: progress.phase as
                  | "start"
                  | "install"
                  | "update"
                  | "uninstall",
              },
            ]);
          }
        }
        break;
      }

      case "StopProgress":
        setStopProgress({ progress: msg.data, receivedAt: Date.now() });
        break;
    }
  };

  // ── Reconnect handler ──
  // On reconnect, preserve existing console history and only clear transient
  // runtime/progress/status so fresh websocket messages repopulate state.
  // Insert a visual reconnection indicator so the user knows there was a
  // disruption (and that some output may be missing).
  const handleReconnect = () => {
    batch(() => {
      setServerStatus(null);
      setRuntime(null);
      setPhaseProgress(null);
      setStopProgress(null);
      prevStatus = null;
      lastAnnouncedPhaseId = null;
      // Reset sequence counter so log lines from a restarted backend
      // (which start from seq 0) are not silently dropped by dedup.
      lastSeq = -1;
    });

    // Visual reconnection marker in the console.
    appendLines([
      {
        timestamp: new Date().toISOString(),
        line: `⚠ Connection lost. Reconnected at ${new Date().toLocaleTimeString()} — some output may be missing`,
        stream: "stderr",
        kind: "separator",
      },
    ]);

    options?.onReconnect?.();
  };

  // ── WebSocket connection ──
  // The URL accessor produces a new URL factory whenever serverId changes,
  // which causes useWebSocket to tear down and recreate the connection.
  const ws = useWebSocket({
    url: () => makeUrlFactory(serverId()),
    onMessage: handleMessage,
    onReconnect: handleReconnect,
    label: "ServerConsole",
  });

  // ── Reset local state when serverId changes ──
  // (useWebSocket already handles reconnection; we just need to clear data)
  createEffect(
    on(serverId, () => {
      batch(() => {
        setLines([]);
        setServerStatus(null);
        setPhaseProgress(null);
        setRuntime(null);
        setStopProgress(null);
        prevStatus = null;
        lastAnnouncedPhaseId = null;
        lastSeq = -1;
      });
    }),
  );

  // ── Public API ──

  const clearLines = () => {
    setLines([]);
    setPhaseProgress(null);
    lastAnnouncedPhaseId = null;
    prevStatus = null;
    lastSeq = -1;
  };

  return {
    connectionState: ws.state,
    isConnected: ws.isConnected,
    reconnectInfo: ws.reconnectInfo,
    lines,
    serverStatus,
    phaseProgress,
    runtime,
    stopProgress,
    clearLines,
    reconnectNow: ws.reconnectNow,
  };
}
