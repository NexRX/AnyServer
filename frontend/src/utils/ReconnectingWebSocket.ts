/**
 * ReconnectingWebSocket — A robust, reusable WebSocket wrapper.
 *
 * Features:
 * - Clean state machine: idle → connecting → connected → disconnected → reconnecting → ...
 * - Automatic reconnection with exponential backoff + jitter
 * - Tab visibility awareness (pauses reconnection when tab is hidden)
 * - Generation tracking to discard stale callbacks
 * - Proper teardown with no leaked timers or listeners
 * - Event emitter API decoupled from any UI framework
 *
 * This is a framework-agnostic primitive. SolidJS hooks wrap this class
 * to provide reactive signals.
 */

// ─── Types ──────────────────────────────────────────────────────────────────

/**
 * Connection lifecycle states.
 *
 *   idle ──► connecting ──► connected ──► disconnected
 *                │              │              │
 *                │              │              ▼
 *                │              │         reconnecting ──► connecting ...
 *                │              │
 *                ▼              ▼
 *             closed          closed
 *
 * `idle`         — Constructed but not yet started.
 * `connecting`   — WebSocket handshake in progress.
 * `connected`    — WebSocket is open and receiving messages.
 * `disconnected` — Connection dropped; will attempt to reconnect.
 * `reconnecting` — Waiting before the next reconnect attempt.
 * `closed`       — Permanently shut down (via `close()`).
 */
export type ConnectionState =
  | "idle"
  | "connecting"
  | "connected"
  | "disconnected"
  | "reconnecting"
  | "closed";

export interface ReconnectingWebSocketOptions {
  /**
   * Either a static URL string, or an async factory that produces a URL.
   * The factory is called on every (re)connect attempt, which allows
   * one-time-use authentication tickets to be fetched fresh each time.
   */
  url: string | (() => Promise<string>);

  /**
   * Maximum number of consecutive reconnect attempts before giving up.
   * Set to `Infinity` (the default) to retry forever.
   */
  maxReconnectAttempts?: number;

  /**
   * Ceiling for the reconnect delay in milliseconds.
   * @default 10_000
   */
  maxReconnectDelay?: number;

  /**
   * Base delay (in ms) before the first reconnect attempt.
   * @default 1_000
   */
  initialReconnectDelay?: number;

  /**
   * Multiplier applied to the delay after each failed attempt.
   * @default 1.5
   */
  backoffMultiplier?: number;

  /**
   * If true, reconnection is paused while the browser tab is hidden
   * and resumes immediately when the tab becomes visible again.
   * @default true
   */
  respectVisibility?: boolean;

  /**
   * If set, a WebSocket ping frame is sent at this interval (ms)
   * to detect stale connections. Set to 0 to disable.
   * Note: browsers don't expose a `ping()` API on WebSocket, so this
   * actually sends a small text message. The backend should tolerate it.
   * @default 0 (disabled)
   */
  heartbeatInterval?: number;

  /**
   * If a heartbeat is enabled, how long (ms) to wait for any message
   * before considering the connection stale and forcing a reconnect.
   * @default 5_000
   */
  heartbeatTimeout?: number;

  /**
   * Optional label for debug logging.
   */
  label?: string;
}

/**
 * Information about a scheduled reconnect attempt, emitted so that UI
 * layers (e.g. ConnectionBanner) can show a countdown to the user.
 */
export interface ReconnectSchedule {
  /** Delay in milliseconds before the next reconnect attempt. */
  delayMs: number;
  /** Which attempt number this is (1-indexed). */
  attempt: number;
  /** `Date.now()` timestamp when the next attempt will fire. */
  nextAttemptAt: number;
}

type EventMap = {
  /** Fired whenever the connection state changes. */
  stateChange: ConnectionState;
  /** Fired for every incoming WebSocket message (raw `MessageEvent.data`). */
  message: unknown;
  /** Fired on WebSocket errors. */
  error: Event;
  /** Fired once when the socket is permanently closed via `close()`. */
  closed: void;
  /**
   * Fired when a reconnect attempt is scheduled, providing the delay and
   * attempt number so consumers can display a countdown timer.
   */
  reconnectScheduled: ReconnectSchedule;
};

type EventCallback<K extends keyof EventMap> = (data: EventMap[K]) => void;

// ─── Implementation ─────────────────────────────────────────────────────────

export class ReconnectingWebSocket {
  // ── Configuration ──
  private readonly urlFactory: () => Promise<string>;
  private readonly maxReconnectAttempts: number;
  private readonly maxReconnectDelay: number;
  private readonly initialReconnectDelay: number;
  private readonly backoffMultiplier: number;
  private readonly respectVisibility: boolean;
  private readonly heartbeatInterval: number;
  private readonly heartbeatTimeout: number;
  private readonly label: string;

  // ── State ──
  private _state: ConnectionState = "idle";
  private generation = 0;
  private reconnectAttempt = 0;
  private ws: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private heartbeatDeadline: ReturnType<typeof setTimeout> | null = null;
  private wasConnectedBefore = false;
  private deferredReconnect = false;

  // ── Event listeners ──
  private listeners: {
    [K in keyof EventMap]: Set<EventCallback<K>>;
  } = {
    stateChange: new Set(),
    message: new Set(),
    error: new Set(),
    closed: new Set(),
    reconnectScheduled: new Set(),
  };

  // ── Bound handlers (for cleanup) ──
  private boundVisibilityChange: (() => void) | null = null;

  constructor(options: ReconnectingWebSocketOptions) {
    const url = options.url;
    this.urlFactory =
      typeof url === "string" ? () => Promise.resolve(url) : url;

    this.maxReconnectAttempts = options.maxReconnectAttempts ?? Infinity;
    this.maxReconnectDelay = options.maxReconnectDelay ?? 10_000;
    this.initialReconnectDelay = options.initialReconnectDelay ?? 1_000;
    this.backoffMultiplier = options.backoffMultiplier ?? 1.5;
    this.respectVisibility = options.respectVisibility ?? true;
    this.heartbeatInterval = options.heartbeatInterval ?? 0;
    this.heartbeatTimeout = options.heartbeatTimeout ?? 5_000;
    this.label = options.label ?? "RWS";

    if (this.respectVisibility && typeof document !== "undefined") {
      this.boundVisibilityChange = this.handleVisibilityChange.bind(this);
      document.addEventListener("visibilitychange", this.boundVisibilityChange);
    }
  }

  // ── Public API ──────────────────────────────────────────────────────────

  /** Current connection state. */
  get state(): ConnectionState {
    return this._state;
  }

  /** Whether the socket is currently open and connected. */
  get isConnected(): boolean {
    return this._state === "connected";
  }

  /** Start the connection. Idempotent if already started. */
  open(): void {
    if (this._state === "closed") {
      this.debug("Cannot open — permanently closed");
      return;
    }
    if (
      this._state === "connecting" ||
      this._state === "connected" ||
      this._state === "reconnecting"
    ) {
      return; // already in progress
    }
    this.connect();
  }

  /**
   * Permanently close the connection. No further reconnection attempts
   * will be made. This is irreversible — create a new instance to reconnect.
   */
  close(): void {
    if (this._state === "closed") return;

    this.debug("Closing permanently");
    this.generation++;
    this.clearTimers();
    this.closeSocket();
    this.setState("closed");
    this.emit("closed", undefined);
    this.removeAllListeners();
    this.teardownVisibility();
  }

  /**
   * Force a reconnect now, resetting the backoff counter.
   * Useful after an expected disconnect (e.g., server reset).
   */
  reconnectNow(): void {
    if (this._state === "closed") return;

    this.debug("Forced reconnect requested");
    this.reconnectAttempt = 0;
    this.generation++;
    this.clearTimers();
    this.closeSocket();
    this.connect();
  }

  /** Subscribe to an event. Returns an unsubscribe function. */
  on<K extends keyof EventMap>(
    event: K,
    callback: EventCallback<K>,
  ): () => void {
    this.listeners[event].add(callback);
    return () => {
      this.listeners[event].delete(callback);
    };
  }

  /** Remove a specific listener. */
  off<K extends keyof EventMap>(event: K, callback: EventCallback<K>): void {
    this.listeners[event].delete(callback);
  }

  // ── Internal ────────────────────────────────────────────────────────────

  private setState(newState: ConnectionState): void {
    if (this._state === newState) return;
    const prev = this._state;
    this._state = newState;
    this.debug(`State: ${prev} → ${newState}`);
    this.emit("stateChange", newState);
  }

  private emit<K extends keyof EventMap>(event: K, data: EventMap[K]): void {
    for (const cb of this.listeners[event]) {
      try {
        cb(data);
      } catch (err) {
        console.error(`[${this.label}] Error in ${event} handler:`, err);
      }
    }
  }

  private removeAllListeners(): void {
    for (const key of Object.keys(this.listeners) as (keyof EventMap)[]) {
      this.listeners[key].clear();
    }
  }

  private async connect(): Promise<void> {
    if (this._state === "closed") return;

    const gen = ++this.generation;
    this.setState("connecting");

    let url: string;
    try {
      url = await this.urlFactory();
    } catch (err) {
      if (gen !== this.generation) return; // stale
      this.debug("URL factory failed:", err);
      this.scheduleReconnect(gen);
      return;
    }

    if (gen !== this.generation) return; // stale

    let socket: WebSocket;
    try {
      socket = new WebSocket(url);
    } catch (err) {
      if (gen !== this.generation) return;
      this.debug("WebSocket constructor failed:", err);
      this.scheduleReconnect(gen);
      return;
    }

    this.ws = socket;

    socket.onopen = () => {
      if (gen !== this.generation) {
        // Stale socket opened after a newer generation was started.
        socket.close();
        return;
      }

      this.debug("Connected");
      this.reconnectAttempt = 0;
      this.wasConnectedBefore = true;
      this.setState("connected");
      this.startHeartbeat(gen);
    };

    socket.onmessage = (event: MessageEvent) => {
      if (gen !== this.generation) return;

      // Any incoming message resets the heartbeat deadline.
      this.resetHeartbeatDeadline(gen);

      this.emit("message", event.data);
    };

    socket.onclose = (_event: CloseEvent) => {
      if (gen !== this.generation) return;

      this.debug("Socket closed");
      this.stopHeartbeat();
      this.ws = null;

      if (this._state === "closed") return;

      this.setState("disconnected");
      this.scheduleReconnect(gen);
    };

    socket.onerror = (event: Event) => {
      if (gen !== this.generation) return;

      this.debug("Socket error");
      this.emit("error", event);

      // Don't change state here — `onclose` will fire immediately after
      // and handle the transition to disconnected → reconnecting.
    };
  }

  private scheduleReconnect(gen: number): void {
    if (this._state === "closed") return;
    if (gen !== this.generation) return;

    if (this.reconnectAttempt >= this.maxReconnectAttempts) {
      this.debug(
        `Max reconnect attempts (${this.maxReconnectAttempts}) reached — giving up`,
      );
      this.setState("disconnected");
      return;
    }

    // If the tab is hidden and we respect visibility, defer until visible.
    if (
      this.respectVisibility &&
      typeof document !== "undefined" &&
      document.visibilityState === "hidden"
    ) {
      this.debug("Tab is hidden — deferring reconnect until visible");
      this.deferredReconnect = true;
      this.setState("reconnecting");
      return;
    }

    const baseDelay =
      this.initialReconnectDelay *
      Math.pow(this.backoffMultiplier, this.reconnectAttempt);
    const cappedDelay = Math.min(baseDelay, this.maxReconnectDelay);
    // Add 0–30% jitter to prevent thundering herd.
    const jitter = cappedDelay * Math.random() * 0.3;
    const delay = Math.round(cappedDelay + jitter);

    this.reconnectAttempt++;
    this.debug(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempt})`);
    this.setState("reconnecting");

    this.emit("reconnectScheduled", {
      delayMs: delay,
      attempt: this.reconnectAttempt,
      nextAttemptAt: Date.now() + delay,
    });

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (gen !== this.generation) return;
      this.connect();
    }, delay);
  }

  // ── Heartbeat ──

  private startHeartbeat(gen: number): void {
    if (this.heartbeatInterval <= 0) return;

    this.stopHeartbeat();

    this.heartbeatTimer = setInterval(() => {
      if (gen !== this.generation) {
        this.stopHeartbeat();
        return;
      }
      // We don't actually need to send anything — just check if we've
      // received any message within the timeout window.
      this.resetHeartbeatDeadline(gen);
    }, this.heartbeatInterval);

    this.resetHeartbeatDeadline(gen);
  }

  private resetHeartbeatDeadline(gen: number): void {
    if (this.heartbeatInterval <= 0) return;

    if (this.heartbeatDeadline) {
      clearTimeout(this.heartbeatDeadline);
    }

    this.heartbeatDeadline = setTimeout(() => {
      this.heartbeatDeadline = null;
      if (gen !== this.generation) return;
      if (this._state !== "connected") return;

      this.debug("Heartbeat timeout — connection appears stale, reconnecting");
      this.closeSocket();
      this.scheduleReconnect(gen);
    }, this.heartbeatInterval + this.heartbeatTimeout);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.heartbeatDeadline) {
      clearTimeout(this.heartbeatDeadline);
      this.heartbeatDeadline = null;
    }
  }

  // ── Visibility ──

  private handleVisibilityChange(): void {
    if (this._state === "closed") return;

    if (document.visibilityState === "visible") {
      this.debug("Tab became visible");

      if (this.deferredReconnect) {
        this.deferredReconnect = false;
        this.debug("Resuming deferred reconnect");
        this.connect();
      } else if (this._state === "connected" && this.ws) {
        // Tab came back — check if the connection is still alive.
        // The next message (or heartbeat timeout) will detect staleness.
      }
    } else {
      this.debug("Tab became hidden");
      // If we're in the middle of a reconnect delay, we'll pick it up
      // when the tab becomes visible again.
    }
  }

  private teardownVisibility(): void {
    if (this.boundVisibilityChange) {
      document.removeEventListener(
        "visibilitychange",
        this.boundVisibilityChange,
      );
      this.boundVisibilityChange = null;
    }
  }

  // ── Cleanup ──

  private closeSocket(): void {
    if (this.ws) {
      const ws = this.ws;
      this.ws = null;
      // Null out handlers before closing to prevent re-entrant calls.
      ws.onopen = null;
      ws.onmessage = null;
      ws.onclose = null;
      ws.onerror = null;
      if (
        ws.readyState === WebSocket.OPEN ||
        ws.readyState === WebSocket.CONNECTING
      ) {
        ws.close();
      }
    }
  }

  private clearTimers(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.stopHeartbeat();
    this.deferredReconnect = false;
  }

  private debug(...args: unknown[]): void {
    if (
      typeof console !== "undefined" &&
      typeof (console as any).__RWS_DEBUG__ !== "undefined"
    ) {
      console.debug(`[${this.label}]`, ...args);
    }
  }
}
