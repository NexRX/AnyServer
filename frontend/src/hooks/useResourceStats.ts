import { createSignal, createEffect, onCleanup, type Accessor } from "solid-js";
import { getServerStats } from "../api/client";
import type { ServerResourceStats } from "../types/bindings";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface UseResourceStatsReturn {
  /** The most recent resource stats snapshot, or null if not yet fetched. */
  resourceStats: Accessor<ServerResourceStats | null>;
  /** Manually trigger a stats fetch. */
  refresh: () => Promise<void>;
}

// ─── Constants ──────────────────────────────────────────────────────────────

const POLL_INTERVAL_MS = 3000;

// ─── Hook ───────────────────────────────────────────────────────────────────

/**
 * Polls server resource stats while a given tab is active.
 *
 * Stats are fetched immediately when the tab becomes "resources", then
 * re-fetched every {@link POLL_INTERVAL_MS} ms. Polling stops when the
 * tab changes away.
 *
 * @param serverId  - Accessor returning the current server ID.
 * @param activeTab - Accessor returning the currently selected tab name.
 *                    Polling is only active when the value is `"resources"`.
 */
export function useResourceStats(
  serverId: Accessor<string>,
  activeTab: Accessor<string>,
): UseResourceStatsReturn {
  const [resourceStats, setResourceStats] =
    createSignal<ServerResourceStats | null>(null);

  let statsInterval: ReturnType<typeof setInterval> | null = null;

  const fetchResourceStats = async () => {
    try {
      const stats = await getServerStats(serverId());
      setResourceStats(stats);
    } catch {
      // Silently ignore — stats are best-effort.
    }
  };

  // Start / stop polling based on active tab.
  createEffect(() => {
    if (activeTab() === "resources") {
      fetchResourceStats();
      if (!statsInterval) {
        statsInterval = setInterval(fetchResourceStats, POLL_INTERVAL_MS);
      }
    } else {
      if (statsInterval) {
        clearInterval(statsInterval);
        statsInterval = null;
      }
    }
  });

  // Ensure interval is cleaned up when the component unmounts.
  onCleanup(() => {
    if (statsInterval) {
      clearInterval(statsInterval);
      statsInterval = null;
    }
  });

  return {
    resourceStats,
    refresh: fetchResourceStats,
  };
}
