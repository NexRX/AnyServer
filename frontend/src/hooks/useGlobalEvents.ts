/**
 * useGlobalEvents — SolidJS hook for the Dashboard's global WebSocket.
 *
 * This hook owns the lifecycle of the global events WebSocket connection
 * (`/api/ws/events`) which streams `StatusChange` messages for all servers.
 * It replaces the hand-rolled WebSocket management that was duplicated in
 * `Dashboard.tsx`.
 *
 * Usage (in Dashboard.tsx):
 *
 *   const globalEvents = useGlobalEvents({ onReconnect: () => refetch() });
 *
 *   // Reactive connection state for the banner:
 *   <ConnectionBanner state={globalEvents.connectionState()} />
 *
 *   // Runtime overrides for live status updates:
 *   const patched = patchServer(server, globalEvents.runtimeOverrides());
 */

import { createSignal, batch, type Accessor } from "solid-js";
import { getWsTicket } from "../api/auth";
import { useWebSocket } from "./useWebSocket";
import type {
  ConnectionState,
  ReconnectSchedule,
} from "../utils/ReconnectingWebSocket";
import type { WsMessage, ServerRuntime } from "../types/bindings";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface UseGlobalEventsOptions {
  /**
   * Called when the WebSocket reconnects after a disconnect.
   * Typically used to trigger a data refetch so the UI is in sync.
   */
  onReconnect?: () => void;
}

export interface UseGlobalEventsReturn {
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

  /**
   * A reactive map of server_id → ServerRuntime, populated from
   * StatusChange messages received over the WebSocket. These override
   * the data from the initial HTTP fetch to provide real-time updates.
   */
  runtimeOverrides: Accessor<Record<string, ServerRuntime>>;

  /** Force an immediate reconnect, resetting the backoff counter. */
  reconnectNow: () => void;
}

// ─── URL Factory ────────────────────────────────────────────────────────────

/**
 * Build a WebSocket URL factory for the global events endpoint.
 * Each invocation fetches a fresh one-time authentication ticket.
 */
function makeGlobalUrlFactory(): () => Promise<string> {
  return async () => {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    const path = "/api/ws/events";
    const { ticket } = await getWsTicket(path);
    return `${proto}//${location.host}${path}?ticket=${encodeURIComponent(ticket)}`;
  };
}

// ─── Hook ───────────────────────────────────────────────────────────────────

export function useGlobalEvents(
  options: UseGlobalEventsOptions = {},
): UseGlobalEventsReturn {
  const [runtimeOverrides, setRuntimeOverrides] = createSignal<
    Record<string, ServerRuntime>
  >({});

  // ── Message handler ──

  const handleMessage = (raw: unknown) => {
    let msg: WsMessage;
    try {
      msg = typeof raw === "string" ? JSON.parse(raw) : (raw as WsMessage);
    } catch (e) {
      console.error("[useGlobalEvents] Failed to parse message:", e, raw);
      return;
    }

    if (msg.type === "StatusChange") {
      const runtime = msg.data;
      setRuntimeOverrides((prev) => ({
        ...prev,
        [runtime.server_id]: runtime,
      }));
    }
    // The global events endpoint currently only sends StatusChange messages.
    // If more message types are added in the future, handle them here.
  };

  // ── Reconnect handler ──
  // On reconnect, clear stale overrides (they'll be repopulated by new
  // StatusChange messages) and notify the parent so it can refetch.
  const handleReconnect = () => {
    batch(() => {
      setRuntimeOverrides({});
    });
    options.onReconnect?.();
  };

  // ── WebSocket connection ──
  // The URL is static (global events endpoint doesn't change), but we
  // still use a factory so each reconnect gets a fresh auth ticket.
  const ws = useWebSocket({
    url: () => makeGlobalUrlFactory(),
    onMessage: handleMessage,
    onReconnect: handleReconnect,
    label: "GlobalEvents",
  });

  return {
    connectionState: ws.state,
    isConnected: ws.isConnected,
    reconnectInfo: ws.reconnectInfo,
    runtimeOverrides,
    reconnectNow: ws.reconnectNow,
  };
}
