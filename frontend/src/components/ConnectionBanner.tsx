/**
 * ConnectionBanner — Shared connection status banner with built-in debounce.
 *
 * This component replaces the ad-hoc banner + debounce timer pattern that was
 * previously duplicated in ServerDetail and Dashboard. It encapsulates all the
 * timing logic so that consumers simply pass the raw connection state and the
 * banner handles the rest.
 *
 * Design principles:
 * - The banner NEVER appears for brief, expected disconnects (e.g., pipeline
 *   completion causing a WebSocket reconnect). Only sustained disconnects
 *   (longer than `debounceMs`) trigger the banner.
 * - Once shown, the banner disappears immediately when the connection is
 *   re-established — no delay on the "good news" path.
 * - The component is fully self-contained: no external timers, no parent
 *   callbacks, no coordination required.
 * - When a reconnect attempt is scheduled, the banner shows a countdown
 *   so the user knows when the next attempt will happen — much more
 *   informative than a static "reconnecting…" message during long backoffs.
 *
 * Usage:
 *
 *   <ConnectionBanner state={connectionState()} />
 *   <ConnectionBanner state={connectionState()} debounceMs={5000} />
 *   <ConnectionBanner state={connectionState()} reconnectInfo={reconnectInfo()} />
 */

import {
  type Component,
  createSignal,
  createEffect,
  onCleanup,
  Show,
} from "solid-js";
import type { ConnectionState } from "../utils/ReconnectingWebSocket";
import type { ReconnectSchedule } from "../utils/ReconnectingWebSocket";

export interface ConnectionBannerProps {
  /**
   * The current connection state from a `useWebSocket`-based hook.
   */
  state: ConnectionState;

  /**
   * How long (ms) a disconnect must persist before the banner appears.
   * This prevents the banner from flashing during transient reconnects
   * (e.g., after a pipeline completes and the backend closes the socket).
   *
   * @default 3000
   */
  debounceMs?: number;

  /**
   * Optional custom message to display. Defaults to "Connection lost — reconnecting…".
   */
  message?: string;

  /**
   * Optional custom CSS class name for the banner container.
   * Falls back to "ws-disconnect-banner" to match existing styles.
   */
  class?: string;

  /**
   * Optional custom CSS class name for the icon span.
   * Falls back to "ws-disconnect-icon" to match existing styles.
   */
  iconClass?: string;

  /**
   * Optional reconnect schedule info from the WebSocket hook.
   * When provided, the banner displays a countdown timer showing
   * how many seconds remain until the next reconnect attempt.
   */
  reconnectInfo?: ReconnectSchedule | null;
}

/**
 * Returns true if the given connection state represents a fully established
 * connection. Only this state should dismiss the banner and cancel the
 * debounce timer.
 *
 * Previously, any non-"disconnected/reconnecting" state (including
 * "connecting") would clear the timer. This caused flapping: each reconnect
 * attempt briefly transitions through "connecting", which reset the debounce
 * timer even though the connection wasn't actually restored. Now only
 * "connected" is treated as "good" — all other states either start the
 * debounce timer or leave it running.
 */
function isConnectedState(state: ConnectionState): boolean {
  return state === "connected";
}

/**
 * Returns true if the given connection state represents a "not connected"
 * condition that should (potentially, after debounce) start showing the
 * banner. The "connecting" state is included because during reconnect
 * cycles it is a transient state that should not interrupt the banner.
 */
function isDisconnectedState(state: ConnectionState): boolean {
  return (
    state === "disconnected" ||
    state === "reconnecting" ||
    state === "connecting"
  );
}

const ConnectionBanner: Component<ConnectionBannerProps> = (props) => {
  const debounceMs = () => props.debounceMs ?? 3000;
  const baseMessage = () => props.message ?? "Connection lost — reconnecting…";
  const bannerClass = () => props.class ?? "ws-disconnect-banner";
  const iconClass = () => props.iconClass ?? "ws-disconnect-icon";

  // Whether the banner is actually visible (after debounce).
  const [visible, setVisible] = createSignal(false);

  // Countdown seconds remaining until next reconnect attempt.
  const [countdownSecs, setCountdownSecs] = createSignal<number | null>(null);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let countdownInterval: ReturnType<typeof setInterval> | null = null;

  const clearDebounceTimer = () => {
    if (debounceTimer !== null) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
  };

  const clearCountdownInterval = () => {
    if (countdownInterval !== null) {
      clearInterval(countdownInterval);
      countdownInterval = null;
    }
  };

  // ── Countdown logic ──
  // When reconnectInfo changes, start a countdown interval that ticks
  // every 100ms for smooth display, showing seconds remaining.
  createEffect(() => {
    const info = props.reconnectInfo;

    if (!info) {
      // No scheduled reconnect — clear the countdown.
      clearCountdownInterval();
      setCountdownSecs(null);
      return;
    }

    // We have a scheduled reconnect — start a countdown.
    const { nextAttemptAt } = info;

    const updateCountdown = () => {
      const remaining = Math.max(0, nextAttemptAt - Date.now());
      const secs = Math.ceil(remaining / 1000);
      setCountdownSecs(secs > 0 ? secs : null);

      // Stop ticking once we've reached zero.
      if (remaining <= 0) {
        clearCountdownInterval();
      }
    };

    // Immediately compute the first value.
    updateCountdown();

    // Tick every 200ms for a responsive countdown.
    clearCountdownInterval();
    countdownInterval = setInterval(updateCountdown, 200);
  });

  // ── Debounce / visibility logic ──
  createEffect(() => {
    const state = props.state;

    if (isConnectedState(state)) {
      // Fully connected — immediately hide the banner and cancel any
      // pending debounce timer. This is the only "good news" path.
      clearDebounceTimer();
      setVisible(false);
    } else if (isDisconnectedState(state)) {
      // Disconnected / reconnecting / connecting (during a reconnect cycle).
      // Start the debounce timer if not already running.
      if (debounceTimer === null && !visible()) {
        debounceTimer = setTimeout(() => {
          debounceTimer = null;
          setVisible(true);
        }, debounceMs());
      }
      // If the banner is already visible, keep it visible (no-op).
      // If a debounce timer is already running, let it run (no-op).
    }
    // For "idle" and "closed" — do nothing. These are terminal/initial
    // states that should not start the timer or dismiss the banner.
  });

  onCleanup(() => {
    clearDebounceTimer();
    clearCountdownInterval();
  });

  /**
   * Build the display message, appending a countdown if available.
   */
  const bannerMessage = () => {
    const secs = countdownSecs();
    const base = baseMessage();

    if (secs !== null && secs > 0) {
      return `${base} (retrying in ${secs}s)`;
    }
    return base;
  };

  return (
    <Show when={visible()}>
      <div class={bannerClass()} role="alert" aria-live="polite">
        <span class={iconClass()}>⚠</span>
        {bannerMessage()}
      </div>
    </Show>
  );
};

export default ConnectionBanner;
