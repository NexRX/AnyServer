import { type Component, Show } from "solid-js";
import type { ServerResourceStats } from "../types/bindings";
import { formatBytes, thresholdClass } from "../utils/format";
import Loader from "./Loader";

export interface ResourcesTabProps {
  stats: ServerResourceStats | null | undefined;
}

const ResourcesTab: Component<ResourcesTabProps> = (props) => {
  return (
    <div class="resources-tab">
      <h3 class="resources-heading">Resource Usage</h3>
      <Show
        when={props.stats}
        fallback={<Loader message="Loading resource stats" compact />}
      >
        {(stats) => {
          const cpuPct = () => stats().cpu_percent;
          const rss = () => stats().memory_rss_bytes;
          const swap = () => stats().memory_swap_bytes;
          const disk = () => stats().disk_usage_bytes;

          return (
            <div class="resources-grid">
              {/* CPU */}
              <div class="health-card">
                <div class="health-card-header">
                  <span class="health-card-icon">🖥</span>
                  <h3>CPU</h3>
                </div>
                <div class="health-card-body">
                  <Show
                    when={cpuPct() !== null}
                    fallback={
                      <span class="resource-na">N/A — server not running</span>
                    }
                  >
                    <div class="health-bar-wrapper">
                      <div class="health-bar-labels">
                        <span class="health-bar-label">Process CPU</span>
                        <span class="health-bar-value">
                          {cpuPct()!.toFixed(1)}%
                        </span>
                      </div>
                      <div class="health-bar-track">
                        <div
                          class={`health-bar-fill health-bar-${thresholdClass(cpuPct()!)}`}
                          style={{
                            width: `${Math.min(100, cpuPct()!)}%`,
                          }}
                        />
                      </div>
                    </div>
                  </Show>
                </div>
              </div>

              {/* Memory */}
              <div class="health-card">
                <div class="health-card-header">
                  <span class="health-card-icon">🧠</span>
                  <h3>Memory</h3>
                </div>
                <div class="health-card-body">
                  <Show
                    when={rss() !== null}
                    fallback={
                      <span class="resource-na">N/A — server not running</span>
                    }
                  >
                    <div class="health-bar-wrapper">
                      <div class="health-bar-labels">
                        <span class="health-bar-label">RSS</span>
                        <span class="health-bar-value">
                          {formatBytes(rss()!)}
                        </span>
                      </div>
                    </div>
                    <Show when={swap() !== null && swap()! > 0}>
                      <div class="health-bar-wrapper">
                        <div class="health-bar-labels">
                          <span class="health-bar-label">Swap</span>
                          <span class="health-bar-value">
                            {formatBytes(swap()!)}
                          </span>
                        </div>
                      </div>
                    </Show>
                  </Show>
                </div>
              </div>

              {/* Disk */}
              <div class="health-card">
                <div class="health-card-header">
                  <span class="health-card-icon">💾</span>
                  <h3>Disk</h3>
                </div>
                <div class="health-card-body">
                  <div class="health-bar-wrapper">
                    <div class="health-bar-labels">
                      <span class="health-bar-label">Server Directory</span>
                      <span class="health-bar-value">
                        {formatBytes(disk())}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          );
        }}
      </Show>
    </div>
  );
};

export default ResourcesTab;
