/**
 * useWebSocket — SolidJS reactive hook wrapping ReconnectingWebSocket.
 *
 * Bridges the framework-agnostic ReconnectingWebSocket class into
 * SolidJS's reactive system, providing:
 * - Reactive signals for connection state
 * - Reactive reconnect schedule info (for countdown display)
 * - Automatic cleanup via `onCleanup`
 * - Reactive URL/options (re-creates the connection when dependencies change)
 * - A clean, minimal API for consumers
 *
 * This is the low-level hook. Most consumers should use the higher-level
 * `useServerConsole` or `useGlobalEvents` hooks instead.
 */

import {
  createSignal,
  onCleanup,
  createEffect,
  on,
  type Accessor,
} from "solid-js";
import {
  ReconnectingWebSocket,
  type ConnectionState,
  type ReconnectingWebSocketOptions,
  type ReconnectSchedule,
} from "../utils/ReconnectingWebSocket";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface UseWebSocketOptions {
  /**
   * Either a static URL string, or an async factory that produces a URL.
   * Wrapped in an accessor so that changes (e.g., serverId) trigger reconnection.
   */
  url: Accessor<string | (() => Promise<string>)>;

  /**
   * Called for every incoming WebSocket message (raw `MessageEvent.data`).
   * This is intentionally a callback rather than a signal because messages
   * arrive at high frequency and should be processed imperatively
   * (e.g., appending to a log buffer) rather than reactively.
   */
  onMessage?: (data: unknown) => void;

  /**
   * Called when the connection transitions to "connected" after having
   * been disconnected. NOT called on the initial connection.
   */
  onReconnect?: () => void;

  /**
   * Maximum reconnect delay in ms.
   * @default 10_000
   */
  maxReconnectDelay?: number;

  /**
   * Base delay before the first reconnect attempt in ms.
   * @default 1_000
   */
  initialReconnectDelay?: number;

  /**
   * Backoff multiplier.
   * @default 1.5
   */
  backoffMultiplier?: number;

  /**
   * Pause reconnection when the tab is hidden.
   * @default true
   */
  respectVisibility?: boolean;

  /**
   * Label for debug logging.
   */
  label?: string;
}

export interface UseWebSocketReturn {
  /** Reactive connection state. */
  state: Accessor<ConnectionState>;

  /** Shorthand: `state() === "connected"`. */
  isConnected: Accessor<boolean>;

  /**
   * Reactive reconnect schedule info. Non-null when a reconnect attempt is
   * pending, null when connected or idle. Consumers (e.g. ConnectionBanner)
   * can use `nextAttemptAt` to display a countdown timer.
   */
  reconnectInfo: Accessor<ReconnectSchedule | null>;

  /**
   * Force an immediate reconnect, resetting the backoff counter.
   * Useful after an expected disconnect (e.g., server reset triggers refetch).
   */
  reconnectNow: () => void;

  /**
   * Permanently close the connection. After calling this, the hook
   * will not attempt any further reconnections.
   */
  close: () => void;
}

// ─── Hook ───────────────────────────────────────────────────────────────────

export function useWebSocket(options: UseWebSocketOptions): UseWebSocketReturn {
  const [state, setState] = createSignal<ConnectionState>("idle");
  const [reconnectInfo, setReconnectInfo] =
    createSignal<ReconnectSchedule | null>(null);

  let rws: ReconnectingWebSocket | null = null;
  let wasConnected = false;

  /**
   * Tear down the current ReconnectingWebSocket instance, if any.
   */
  const destroyCurrent = () => {
    if (rws) {
      rws.close();
      rws = null;
    }
    wasConnected = false;
  };

  /**
   * Create a fresh ReconnectingWebSocket instance with the current options
   * and wire up event handlers.
   */
  const createInstance = (url: string | (() => Promise<string>)) => {
    destroyCurrent();

    const rwsOptions: ReconnectingWebSocketOptions = {
      url,
      maxReconnectDelay: options.maxReconnectDelay,
      initialReconnectDelay: options.initialReconnectDelay,
      backoffMultiplier: options.backoffMultiplier,
      respectVisibility: options.respectVisibility,
      label: options.label,
    };

    const instance = new ReconnectingWebSocket(rwsOptions);
    rws = instance;

    instance.on("stateChange", (newState) => {
      setState(newState);

      // Clear reconnect schedule when fully connected.
      if (newState === "connected") {
        setReconnectInfo(null);
      }

      // Detect reconnection (connected after having been connected before).
      if (newState === "connected" && wasConnected) {
        options.onReconnect?.();
      }
      if (newState === "connected") {
        wasConnected = true;
      }
    });

    instance.on("reconnectScheduled", (schedule) => {
      setReconnectInfo(schedule);
    });

    instance.on("message", (data) => {
      options.onMessage?.(data);
    });

    // Start the connection.
    instance.open();
  };

  // Reactively (re-)create the connection whenever the URL accessor changes.
  // `on(url, ...)` tracks changes to the url accessor specifically.
  // `defer: false` means it runs immediately on creation too.
  createEffect(
    on(options.url, (urlValue) => {
      createInstance(urlValue);
    }),
  );

  // Cleanup when the owning component/scope is disposed.
  onCleanup(() => {
    destroyCurrent();
  });

  return {
    state,
    isConnected: () => state() === "connected",
    reconnectInfo,
    reconnectNow: () => rws?.reconnectNow(),
    close: () => {
      destroyCurrent();
      setState("closed");
      setReconnectInfo(null);
    },
  };
}
