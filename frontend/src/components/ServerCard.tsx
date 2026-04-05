import {
  type Component,
  createSignal,
  createEffect,
  onCleanup,
  Show,
} from "solid-js";
import { A } from "@solidjs/router";
import { startServer, stopServer, getServerStats } from "../api/client";
import { formatBytes } from "../utils/format";
import StatusTooltip from "./StatusTooltip";
import RestartBanner from "./RestartBanner";
import type {
  ServerWithStatus,
  ServerStatus,
  ServerResourceStats,
  UpdateCheckResult,
} from "../types/bindings";

interface Props {
  server: ServerWithStatus;
  onAction: () => void;
  updateResult?: UpdateCheckResult | null;
}

const statusConfig: Record<
  ServerStatus,
  { color: string; glow: string; label: string }
> = {
  running: {
    color: "#22c55e",
    glow: "rgba(34, 197, 94, 0.4)",
    label: "Running",
  },
  starting: {
    color: "#eab308",
    glow: "rgba(234, 179, 8, 0.4)",
    label: "Starting",
  },
  stopping: {
    color: "#f97316",
    glow: "rgba(249, 115, 22, 0.4)",
    label: "Stopping",
  },
  crashed: {
    color: "#ef4444",
    glow: "rgba(239, 68, 68, 0.4)",
    label: "Crashed",
  },
  stopped: {
    color: "#6b7280",
    glow: "rgba(107, 114, 128, 0.2)",
    label: "Stopped",
  },
  installing: {
    color: "#8b5cf6",
    glow: "rgba(139, 92, 246, 0.4)",
    label: "Installing",
  },
  updating: {
    color: "#8b5cf6",
    glow: "rgba(139, 92, 246, 0.4)",
    label: "Updating",
  },
  uninstalling: {
    color: "#ef4444",
    glow: "rgba(239, 68, 68, 0.4)",
    label: "Uninstalling",
  },
};

const ServerCard: Component<Props> = (props) => {
  const srv = () => props.server.server;
  const cfg = () =>
    statusConfig[props.server.runtime.status] ?? statusConfig.stopped;

  const [stats, setStats] = createSignal<ServerResourceStats | null>(null);
  const [actionPending, setActionPending] = createSignal(false);

  const fetchStats = async () => {
    try {
      const result = await getServerStats(srv().id);
      setStats(result);
    } catch {
      /* ignore */
    }
  };

  // Only poll resource stats for servers in an active state.
  // Stopped/crashed servers don't need constant stats fetching.
  const isActiveStatus = () => {
    const s = props.server.runtime.status;
    return (
      s === "running" ||
      s === "starting" ||
      s === "stopping" ||
      s === "installing" ||
      s === "updating" ||
      s === "uninstalling"
    );
  };

  let statsInterval: ReturnType<typeof setInterval> | null = null;

  // Fetch once on mount so even stopped servers show disk usage.
  fetchStats();

  createEffect(() => {
    // Clear any previous interval when status changes.
    if (statsInterval) {
      clearInterval(statsInterval);
      statsInterval = null;
    }

    if (isActiveStatus()) {
      statsInterval = setInterval(fetchStats, 5000);
    }
  });

  onCleanup(() => {
    if (statsInterval) {
      clearInterval(statsInterval);
      statsInterval = null;
    }
  });

  const handleStart = async (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (actionPending()) return;
    setActionPending(true);
    try {
      await startServer(srv().id);
    } catch (err) {
      console.error("Failed to start server:", err);
    } finally {
      setActionPending(false);
    }
    props.onAction();
  };

  const handleStop = async (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (actionPending()) return;
    setActionPending(true);
    try {
      await stopServer(srv().id);
    } catch (err) {
      console.error("Failed to stop server:", err);
    } finally {
      setActionPending(false);
    }
    props.onAction();
  };

  const isRunning = () =>
    props.server.runtime.status === "running" ||
    props.server.runtime.status === "starting";

  const isStopped = () =>
    props.server.runtime.status === "stopped" ||
    props.server.runtime.status === "crashed";

  const isAnimatedStatus = () =>
    props.server.runtime.status === "running" ||
    props.server.runtime.status === "starting" ||
    props.server.runtime.status === "stopping" ||
    props.server.runtime.status === "installing" ||
    props.server.runtime.status === "updating" ||
    props.server.runtime.status === "uninstalling";

  /** CPU percentage clamped 0..100 for the mini bar */
  const cpuPct = () => {
    const s = stats();
    if (!s || s.cpu_percent === null) return null;
    return Math.min(100, Math.max(0, s.cpu_percent));
  };

  /** Memory in human string */
  const memStr = () => {
    const s = stats();
    if (!s || s.memory_rss_bytes === null) return null;
    return formatBytes(s.memory_rss_bytes!);
  };

  /** Disk in human string */
  const diskStr = () => {
    const s = stats();
    if (!s) return null;
    return formatBytes(s.disk_usage_bytes);
  };

  return (
    <A
      href={`/server/${srv().id}`}
      class="server-card"
      style={{
        "--card-accent": cfg().color,
        "--card-glow": cfg().glow,
      }}
    >
      {/* Accent stripe along the left */}
      <div class="server-card-accent" aria-hidden="true" />

      <div class="server-card-body">
        <div class="server-card-header">
          <span
            class="status-dot"
            classList={{ "status-dot-pulse": isAnimatedStatus() }}
            style={{
              background: cfg().color,
              "box-shadow": `0 0 6px ${cfg().glow}`,
            }}
          />
          <h3>{srv().config.name}</h3>
          <Show when={props.updateResult?.update_available}>
            <span
              class="card-update-badge"
              title={`Update available: ${props.updateResult?.installed_version_display ?? props.updateResult?.installed_version ?? "?"} → ${props.updateResult?.latest_version_display ?? props.updateResult?.latest_version ?? "?"}`}
            >
              ⬆ Update
            </span>
          </Show>
        </div>

        <div class="server-card-meta">
          <span class={`status-badge status-${props.server.runtime.status}`}>
            {props.server.runtime.status}
          </span>
          <StatusTooltip status={props.server.runtime.status} />
          <span class="binary" title={srv().config.binary}>
            {srv().config.binary}
          </span>
        </div>

        <div class="server-card-details">
          {srv().config.auto_restart && <span class="tag">auto-restart</span>}
          {srv().config.auto_start && <span class="tag">auto-start</span>}
          {props.server.runtime.restart_count > 0 && (
            <span class="tag">
              restarts: {props.server.runtime.restart_count}
            </span>
          )}
        </div>

        {/* Resource mini-bars — only shown when we have stats */}
        <Show when={stats()}>
          <div class="server-card-resources">
            <Show when={cpuPct() !== null}>
              <div
                class="resource-mini"
                title={`CPU: ${cpuPct()!.toFixed(1)}%`}
              >
                <span class="resource-mini-label">CPU</span>
                <div class="resource-mini-track">
                  <div
                    class="resource-mini-fill"
                    classList={{
                      "resource-mini-ok": cpuPct()! < 70,
                      "resource-mini-warn": cpuPct()! >= 70 && cpuPct()! < 90,
                      "resource-mini-crit": cpuPct()! >= 90,
                    }}
                    style={{ width: `${cpuPct()}%` }}
                  />
                </div>
                <span class="resource-mini-value">{cpuPct()!.toFixed(0)}%</span>
              </div>
            </Show>
            <Show when={memStr()}>
              <div class="resource-mini" title={`Memory (RSS): ${memStr()}`}>
                <span class="resource-mini-label">MEM</span>
                <span class="resource-mini-text">{memStr()}</span>
              </div>
            </Show>
            <Show when={diskStr()}>
              <div class="resource-mini" title={`Disk usage: ${diskStr()}`}>
                <span class="resource-mini-label">DISK</span>
                <span class="resource-mini-text">{diskStr()}</span>
              </div>
            </Show>
          </div>
        </Show>

        {/* Restart banner when crashed with pending restart */}
        <Show
          when={
            props.server.runtime.status === "crashed" &&
            props.server.runtime.next_restart_at
          }
        >
          <RestartBanner
            serverId={srv().id}
            runtime={props.server.runtime}
            maxAttempts={srv().config.max_restart_attempts}
            restartDelaySecs={srv().config.restart_delay_secs}
            onCancelled={props.onAction}
            compact={true}
          />
        </Show>

        <div class="server-card-actions">
          {isStopped() ? (
            <button
              class="btn btn-sm btn-success"
              onClick={handleStart}
              disabled={actionPending()}
              aria-label={`Start ${srv().config.name}`}
            >
              {actionPending() ? <span class="btn-spinner" /> : "▶"} Start
            </button>
          ) : isRunning() ? (
            <button
              class="btn btn-sm btn-danger"
              onClick={handleStop}
              disabled={actionPending()}
              aria-label={`Stop ${srv().config.name}`}
            >
              {actionPending() ? <span class="btn-spinner" /> : "■"} Stop
            </button>
          ) : (
            <button class="btn btn-sm" disabled>
              {props.server.runtime.status}…
            </button>
          )}
        </div>
      </div>
    </A>
  );
};

export default ServerCard;
