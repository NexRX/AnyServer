import {
  type Component,
  createSignal,
  onCleanup,
  onMount,
  Show,
  For,
} from "solid-js";
import Loader from "../components/Loader";
import { getSystemHealth, getSteamCmdStatus } from "../api/client";
import {
  formatBytes as _formatBytes,
  formatUptime as _formatUptime,
  thresholdClass,
} from "../utils/format";
import type {
  SystemHealth as SystemHealthType,
  CpuMetrics,
  MemoryMetrics,
  DiskMetrics,
  NetworkMetrics,
  SteamCmdStatusResponse,
} from "../types/bindings";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  return _formatBytes(bytes);
}

function formatUptime(seconds: number): string {
  return _formatUptime(seconds);
}

const ProgressBar: Component<{
  percent: number;
  label?: string;
  sublabel?: string;
}> = (props) => {
  const clamped = () => Math.max(0, Math.min(100, props.percent));
  const cls = () => thresholdClass(clamped());

  return (
    <div class="health-bar-wrapper">
      <div class="health-bar-labels">
        <Show when={props.label}>
          <span class="health-bar-label">{props.label}</span>
        </Show>
        <span class="health-bar-value">{clamped().toFixed(1)}%</span>
      </div>
      <div class="health-bar-track">
        <div
          class={`health-bar-fill health-bar-${cls()}`}
          style={{ width: `${clamped()}%` }}
        />
      </div>
      <Show when={props.sublabel}>
        <span class="health-bar-sublabel">{props.sublabel}</span>
      </Show>
    </div>
  );
};

const CpuSection: Component<{ cpu: CpuMetrics }> = (props) => {
  return (
    <div class="health-card">
      <div class="health-card-header">
        <span class="health-card-icon">🖥</span>
        <h3>CPU</h3>
        <span class="health-card-badge">{props.cpu.core_count} cores</span>
      </div>
      <div class="health-card-body">
        <ProgressBar
          percent={props.cpu.overall_percent}
          label="Overall"
          sublabel={`Load: ${props.cpu.load_avg_1.toFixed(2)} / ${props.cpu.load_avg_5.toFixed(2)} / ${props.cpu.load_avg_15.toFixed(2)}`}
        />
        <div class="health-core-grid">
          <For each={props.cpu.per_core_percent}>
            {(pct, i) => (
              <div class="health-core-item">
                <span class="health-core-label">Core {i()}</span>
                <div class="health-bar-track health-bar-track-sm">
                  <div
                    class={`health-bar-fill health-bar-${thresholdClass(pct)}`}
                    style={{ width: `${Math.min(100, pct)}%` }}
                  />
                </div>
                <span class="health-core-pct">{pct.toFixed(0)}%</span>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
};

const MemorySection: Component<{ memory: MemoryMetrics }> = (props) => {
  const usedPercent = () =>
    props.memory.total_bytes > 0
      ? (props.memory.used_bytes / props.memory.total_bytes) * 100
      : 0;
  const swapPercent = () =>
    props.memory.swap_total_bytes > 0
      ? (props.memory.swap_used_bytes / props.memory.swap_total_bytes) * 100
      : 0;

  return (
    <div class="health-card">
      <div class="health-card-header">
        <span class="health-card-icon">🧠</span>
        <h3>Memory</h3>
      </div>
      <div class="health-card-body">
        <ProgressBar
          percent={usedPercent()}
          label="RAM"
          sublabel={`${formatBytes(props.memory.used_bytes)} / ${formatBytes(props.memory.total_bytes)} (${formatBytes(props.memory.available_bytes)} available)`}
        />
        <Show when={props.memory.swap_total_bytes > 0}>
          <ProgressBar
            percent={swapPercent()}
            label="Swap"
            sublabel={`${formatBytes(props.memory.swap_used_bytes)} / ${formatBytes(props.memory.swap_total_bytes)}`}
          />
        </Show>
      </div>
    </div>
  );
};

const DiskSection: Component<{ disks: DiskMetrics[] }> = (props) => {
  return (
    <div class="health-card">
      <div class="health-card-header">
        <span class="health-card-icon">💾</span>
        <h3>Disk</h3>
        <span class="health-card-badge">
          {props.disks.length} volume{props.disks.length !== 1 ? "s" : ""}
        </span>
      </div>
      <div class="health-card-body">
        <For each={props.disks}>
          {(disk) => {
            const usedPercent = () =>
              disk.total_bytes > 0
                ? (disk.used_bytes / disk.total_bytes) * 100
                : 0;
            return (
              <ProgressBar
                percent={usedPercent()}
                label={disk.mount_point}
                sublabel={`${formatBytes(disk.used_bytes)} / ${formatBytes(disk.total_bytes)} free: ${formatBytes(disk.free_bytes)} (${disk.filesystem})`}
              />
            );
          }}
        </For>
      </div>
    </div>
  );
};

const NetworkSection: Component<{ networks: NetworkMetrics[] }> = (props) => {
  return (
    <div class="health-card">
      <div class="health-card-header">
        <span class="health-card-icon">🌐</span>
        <h3>Network</h3>
      </div>
      <div class="health-card-body">
        <div class="health-network-list">
          <div class="health-network-row health-network-header">
            <span class="health-network-col-iface">Interface</span>
            <span class="health-network-col-rx">Received</span>
            <span class="health-network-col-tx">Transmitted</span>
          </div>
          <For each={props.networks}>
            {(net) => (
              <div class="health-network-row">
                <span class="health-network-col-iface">{net.interface}</span>
                <span class="health-network-col-rx">
                  ↓ {formatBytes(net.rx_bytes)}
                </span>
                <span class="health-network-col-tx">
                  ↑ {formatBytes(net.tx_bytes)}
                </span>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
};

const SteamCmdSection: Component<{ status: SteamCmdStatusResponse }> = (
  props,
) => {
  return (
    <div class="health-card">
      <div class="health-card-header">
        <span class="health-card-icon">🎮</span>
        <h3>SteamCMD</h3>
        <Show when={props.status.available}>
          <span
            class="health-card-badge"
            style={{ background: "rgba(34, 197, 94, 0.15)", color: "#22c55e" }}
          >
            Available
          </span>
        </Show>
        <Show when={!props.status.available}>
          <span
            class="health-card-badge"
            style={{ background: "rgba(239, 68, 68, 0.15)", color: "#f87171" }}
          >
            Not Installed
          </span>
        </Show>
      </div>
      <div class="health-card-body">
        <Show
          when={props.status.available}
          fallback={
            <div class="steamcmd-status-unavailable">
              <p
                style={{
                  color: "#f87171",
                  "margin-bottom": "0.5rem",
                  "font-size": "0.9rem",
                }}
              >
                {props.status.message ?? "SteamCMD was not found on PATH."}
              </p>
              <p
                style={{
                  color: "#9ca3af",
                  "font-size": "0.85rem",
                  "margin-bottom": "0.75rem",
                }}
              >
                Templates that use SteamCMD (e.g., Valheim Dedicated Server)
                will not be able to install or update until SteamCMD is
                available.
              </p>
              <dl
                style={{
                  margin: "0",
                  "font-size": "0.85rem",
                  color: "#9ca3af",
                }}
              >
                <dt
                  style={{ "font-weight": "600", "margin-bottom": "0.25rem" }}
                >
                  Installation options:
                </dt>
                <dd style={{ margin: "0 0 0.15rem 1rem" }}>
                  • Debian/Ubuntu: <code>sudo apt install steamcmd</code>
                </dd>
                <dd style={{ margin: "0 0 0.15rem 1rem" }}>
                  • Arch Linux: <code>yay -S steamcmd</code> (AUR)
                </dd>
                <dd style={{ margin: "0 0 0.15rem 1rem" }}>
                  • NixOS: <code>nix-env -iA nixpkgs.steamcmd</code>
                </dd>
                <dd style={{ margin: "0 0 0.15rem 1rem" }}>
                  • Docker: add <code>steamcmd</code> to your Dockerfile
                </dd>
              </dl>
              <a
                href="https://developer.valvesoftware.com/wiki/SteamCMD#Downloading_SteamCMD"
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  color: "var(--accent)",
                  "font-size": "0.85rem",
                  "margin-top": "0.5rem",
                  display: "inline-block",
                }}
              >
                SteamCMD installation guide →
              </a>
            </div>
          }
        >
          <dl style={{ margin: "0", "font-size": "0.9rem" }}>
            <div
              style={{
                display: "flex",
                gap: "0.5rem",
                "margin-bottom": "0.35rem",
              }}
            >
              <dt style={{ color: "#9ca3af", "min-width": "3.5rem" }}>Path:</dt>
              <dd style={{ margin: "0" }}>
                <code>{props.status.path}</code>
              </dd>
            </div>
            <Show when={props.status.message}>
              <div style={{ display: "flex", gap: "0.5rem" }}>
                <dt style={{ color: "#9ca3af", "min-width": "3.5rem" }}>
                  Status:
                </dt>
                <dd style={{ margin: "0", color: "#22c55e" }}>
                  {props.status.message}
                </dd>
              </div>
            </Show>
          </dl>
        </Show>
      </div>
    </div>
  );
};

const SystemHealth: Component = () => {
  const [health, setHealth] = createSignal<SystemHealthType | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [steamcmdStatus, setSteamcmdStatus] =
    createSignal<SteamCmdStatusResponse | null>(null);

  const fetchHealth = async () => {
    try {
      const data = await getSystemHealth();
      setHealth(data);
      setError(null);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    } finally {
      setLoading(false);
    }
  };

  const fetchSteamCmdStatus = async () => {
    try {
      const status = await getSteamCmdStatus();
      setSteamcmdStatus(status);
    } catch {
      // Non-critical — silently ignore
    }
  };

  fetchHealth();

  onMount(() => {
    fetchSteamCmdStatus();
  });

  const interval = setInterval(fetchHealth, 3000);
  onCleanup(() => clearInterval(interval));

  return (
    <div class="system-health">
      <div class="page-header">
        <h1>System Health</h1>
        <Show when={health()}>
          <span class="health-hostname">
            {health()!.hostname} — up {formatUptime(health()!.uptime_secs)}
          </span>
        </Show>
      </div>

      <Show when={loading() && !health()}>
        <Loader message="Loading system metrics" />
      </Show>

      <Show when={error() && !health()}>
        <div class="error-msg">Failed to load system health: {error()}</div>
      </Show>

      <Show when={health()}>
        {(data) => (
          <div class="health-grid">
            <CpuSection cpu={data().cpu} />
            <MemorySection memory={data().memory} />
            <DiskSection disks={data().disks} />
            <NetworkSection networks={data().networks} />
            <Show when={steamcmdStatus()}>
              {(status) => <SteamCmdSection status={status()} />}
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
};

export default SystemHealth;
