import { createSignal } from "solid-js";
import type { ApiError } from "../types/bindings";

const BASE = "/api";
const TOKEN_STORAGE_KEY = "anyserver_access_token";

// ── Token State ─────────────────────────────────────────────────────────────

let accessToken: string | null = null;

/**
 * When true, the session is irrecoverably dead (refresh failed).
 * All subsequent authenticated requests short-circuit with a
 * never-resolving promise so the UI doesn't flash errors while
 * the redirect to /login is in progress.
 */
let sessionDead = false;

// ── JWT Helpers ─────────────────────────────────────────────────────────────

/**
 * Decode a JWT payload without verification (we only need the `exp` claim
 * for scheduling; the backend is the authority on validity).
 */
function decodeJwtPayload(token: string): Record<string, unknown> | null {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return null;
    // Base64url → Base64 → decode
    const b64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const json = atob(b64);
    return JSON.parse(json);
  } catch {
    return null;
  }
}

/**
 * Return the absolute expiry time (ms since epoch) embedded in a JWT,
 * or `null` if the token can't be parsed.
 */
export function getTokenExpiry(token: string): number | null {
  const payload = decodeJwtPayload(token);
  if (!payload || typeof payload.exp !== "number") return null;
  return payload.exp * 1000;
}

/**
 * Returns true if the token is either expired or will expire within
 * `marginMs` milliseconds.
 */
export function isTokenExpiringSoon(
  token: string,
  marginMs: number = 60_000,
): boolean {
  const exp = getTokenExpiry(token);
  if (exp === null) return true; // unparseable → treat as expired
  return exp - Date.now() < marginMs;
}

// ── Persistent Storage (localStorage — shared across tabs) ──────────────────

function persistToken(token: string): void {
  try {
    localStorage.setItem(TOKEN_STORAGE_KEY, token);
  } catch {
    // Storage may be unavailable in some privacy modes
  }
}

function readPersistedToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_STORAGE_KEY);
  } catch {
    return null;
  }
}

function removePersistedToken(): void {
  try {
    localStorage.removeItem(TOKEN_STORAGE_KEY);
  } catch {
    // Silently ignore
  }
}

/**
 * Attempt to restore the access token from localStorage.
 * Unlike the old sessionStorage approach this works across tabs.
 */
function restoreToken(): string | null {
  if (accessToken) return accessToken;
  const stored = readPersistedToken();
  if (stored) {
    accessToken = stored;
  }
  return accessToken;
}

// ── Public Token API ────────────────────────────────────────────────────────

export function getToken(): string | null {
  // E2E test override
  if (typeof window !== "undefined" && (window as any).__E2E_AUTH_TOKEN__) {
    const e2eToken = (window as any).__E2E_AUTH_TOKEN__;
    if (accessToken !== e2eToken) {
      accessToken = e2eToken;
      console.log(
        "[Auth] E2E token detected and applied:",
        accessToken?.substring(0, 20) + "...",
      );
    }
    return accessToken;
  }
  return restoreToken();
}

export function setToken(token: string): void {
  accessToken = token;
  sessionDead = false;
  persistToken(token);
  scheduleProactiveRefresh(token);
  broadcastTokenUpdate(token);
}

export function clearToken(): void {
  accessToken = null;
  cancelProactiveRefresh();
  removePersistedToken();
  broadcastLogout();
}

export function isLoggedIn(): boolean {
  return getToken() !== null;
}

// ── Error Types ─────────────────────────────────────────────────────────────

export class ApiClientError extends Error {
  public readonly status: number;
  public readonly body: ApiError;

  constructor(status: number, body: ApiError) {
    super(body.error);
    this.name = "ApiClientError";
    this.status = status;
    this.body = body;
  }
}

/** Thrown on 429 so callers / global handler can show the countdown UI. */
export class RateLimitError extends Error {
  /** Absolute timestamp (ms) when the next request is allowed. */
  public readonly retryAt: number;
  /** Original seconds value from the Retry-After header. */
  public readonly retryAfterSecs: number;

  constructor(retryAfterSecs: number) {
    super(`Rate limited — retry in ${retryAfterSecs}s`);
    this.name = "RateLimitError";
    this.retryAfterSecs = retryAfterSecs;
    this.retryAt = Date.now() + retryAfterSecs * 1000;
  }
}

// ── Rate-limit reactive signal ──────────────────────────────────────────────

const [rateLimitRetryAt, setRateLimitRetryAt] = createSignal<number | null>(
  null,
);

export { rateLimitRetryAt };

export function emitRateLimitEvent(retryAfterSecs: number): void {
  setRateLimitRetryAt(Date.now() + retryAfterSecs * 1000);
}

// ── Auth State Change Events ────────────────────────────────────────────────
//
// The auth context subscribes to these so it can react to cross-tab
// events and session expiry without tight coupling.

export type AuthEvent =
  /** A fresh access token was obtained (this or another tab). */
  | "token_refreshed"
  /** The user explicitly logged out (this or another tab). */
  | "logged_out"
  /** All refresh attempts failed — session is irrecoverable. */
  | "session_expired";

type AuthListener = (event: AuthEvent) => void;

const authListeners = new Set<AuthListener>();

/**
 * Register a callback that fires whenever the auth state changes
 * across any tab. Returns an unsubscribe function.
 */
export function onAuthStateChange(listener: AuthListener): () => void {
  authListeners.add(listener);
  return () => {
    authListeners.delete(listener);
  };
}

function notifyAuthListeners(event: AuthEvent): void {
  for (const listener of authListeners) {
    try {
      listener(event);
    } catch (e) {
      console.error("[Auth] Listener error:", e);
    }
  }
}

// ── Cross-Tab Sync (BroadcastChannel + storage event fallback) ──────────────

type AuthBroadcast =
  | { type: "token_updated"; token: string }
  | { type: "logged_out" }
  | { type: "session_expired" };

let authChannel: BroadcastChannel | null = null;

function broadcastTokenUpdate(token: string): void {
  try {
    authChannel?.postMessage({
      type: "token_updated",
      token,
    } satisfies AuthBroadcast);
  } catch {
    // Channel may be closed
  }
}

function broadcastLogout(): void {
  try {
    authChannel?.postMessage({ type: "logged_out" } satisfies AuthBroadcast);
  } catch {
    // Channel may be closed
  }
}

function broadcastSessionExpired(): void {
  try {
    authChannel?.postMessage({
      type: "session_expired",
    } satisfies AuthBroadcast);
  } catch {
    // Channel may be closed
  }
}

function handleBroadcastMessage(msg: AuthBroadcast): void {
  switch (msg.type) {
    case "token_updated":
      accessToken = msg.token;
      sessionDead = false;
      persistToken(msg.token);
      scheduleProactiveRefresh(msg.token);
      notifyAuthListeners("token_refreshed");
      break;

    case "logged_out":
      accessToken = null;
      cancelProactiveRefresh();
      removePersistedToken();
      notifyAuthListeners("logged_out");
      break;

    case "session_expired":
      accessToken = null;
      sessionDead = true;
      cancelProactiveRefresh();
      removePersistedToken();
      notifyAuthListeners("session_expired");
      break;
  }
}

/**
 * Initialise the cross-tab synchronisation channels.
 * Called once during `startSessionManager()`.
 */
function initCrossTabSync(): void {
  // Primary: BroadcastChannel (typed messages, reliable)
  if (typeof BroadcastChannel !== "undefined") {
    try {
      authChannel = new BroadcastChannel("anyserver_auth");
      authChannel.onmessage = (event: MessageEvent<AuthBroadcast>) => {
        handleBroadcastMessage(event.data);
      };
    } catch {
      // Fallback to storage-only
    }
  }

  // Fallback: storage events (fires in OTHER tabs when localStorage changes)
  if (typeof window !== "undefined") {
    window.addEventListener("storage", (event: StorageEvent) => {
      if (event.key !== TOKEN_STORAGE_KEY) return;

      // If we already have BroadcastChannel, the BC message will arrive
      // first and be more informative. But handle storage events anyway
      // as a fallback for environments without BC support.
      if (event.newValue) {
        // Token was updated by another tab
        if (event.newValue !== accessToken) {
          accessToken = event.newValue;
          sessionDead = false;
          scheduleProactiveRefresh(event.newValue);
          notifyAuthListeners("token_refreshed");
        }
      } else {
        // Token was removed by another tab (logout or session expiry)
        if (accessToken !== null) {
          accessToken = null;
          cancelProactiveRefresh();
          // We can't distinguish logout vs session_expired from a storage
          // event alone. If the BroadcastChannel message hasn't already
          // handled it, treat it as logged_out (safer — the login page
          // won't show the "session expired" banner, but that's acceptable
          // for the fallback path).
          notifyAuthListeners("logged_out");
        }
      }
    });
  }
}

// ── Proactive Token Refresh ─────────────────────────────────────────────────
//
// Instead of waiting for a 401 to trigger a refresh, we schedule a
// refresh at ~75% of the access token's lifetime. This keeps the user
// seamlessly logged in during idle periods and avoids the jarring
// 401→refresh→retry flow.

let proactiveRefreshTimer: ReturnType<typeof setTimeout> | null = null;

function cancelProactiveRefresh(): void {
  if (proactiveRefreshTimer !== null) {
    clearTimeout(proactiveRefreshTimer);
    proactiveRefreshTimer = null;
  }
}

function scheduleProactiveRefresh(token: string): void {
  cancelProactiveRefresh();

  const expiresAt = getTokenExpiry(token);
  if (expiresAt === null) return;

  const now = Date.now();
  const timeUntilExpiry = expiresAt - now;

  if (timeUntilExpiry <= 0) return; // Already expired

  // Refresh at 75% of lifetime, but at least 30s before expiry,
  // and never sooner than 10s from now (avoid tight loops).
  const refreshIn = Math.max(
    Math.min(timeUntilExpiry * 0.75, timeUntilExpiry - 30_000),
    Math.min(10_000, timeUntilExpiry - 5_000),
  );

  if (refreshIn <= 0) {
    // Token expires in < 5s — try to refresh immediately
    doProactiveRefresh();
    return;
  }

  proactiveRefreshTimer = setTimeout(() => {
    proactiveRefreshTimer = null;
    doProactiveRefresh();
  }, refreshIn);
}

async function doProactiveRefresh(): Promise<void> {
  // Only refresh from the visible tab to avoid thundering herd.
  // Hidden tabs will pick up the new token via BroadcastChannel/storage.
  // Exception: if no tab is visible (all hidden), one of them should still
  // refresh. We use a small random delay + localStorage check to coordinate.
  if (
    typeof document !== "undefined" &&
    document.visibilityState === "hidden"
  ) {
    // Schedule a check for when we become visible
    const onVisible = () => {
      document.removeEventListener("visibilitychange", onVisible);
      // When we become visible, check if another tab already refreshed
      const stored = readPersistedToken();
      if (stored && stored !== accessToken) {
        // Another tab refreshed while we were hidden
        accessToken = stored;
        scheduleProactiveRefresh(stored);
        notifyAuthListeners("token_refreshed");
      } else if (stored && !isTokenExpiringSoon(stored, 120_000)) {
        // Token is still fresh enough — just reschedule
        scheduleProactiveRefresh(stored);
      } else {
        // Token is stale — refresh now
        refreshAccessToken().catch(() => {
          // Will be handled by the next API call's 401 path
        });
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    return;
  }

  try {
    await refreshAccessToken();
  } catch {
    // Refresh failed — the next API call will get a 401 and handle it.
    // If the refresh token is truly dead, that path will emit
    // session_expired. Don't do it here to avoid false positives
    // (e.g. transient network error).
  }
}

// ── Visibility Change Handler ───────────────────────────────────────────────
//
// When a tab becomes visible after being in the background, browsers may
// have throttled timers. Check if the token needs attention.

function initVisibilityHandler(): void {
  if (typeof document === "undefined") return;

  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "visible") return;

    const token = getToken();
    if (!token) return; // Not logged in

    // Check if another tab updated the token while we were hidden
    const stored = readPersistedToken();
    if (stored && stored !== accessToken) {
      accessToken = stored;
      scheduleProactiveRefresh(stored);
      return;
    }

    // Check if the token is expired or about to expire
    if (isTokenExpiringSoon(token, 120_000)) {
      // Token expires within 2 minutes — refresh now
      refreshAccessToken().catch(() => {
        // Will be caught by the next API request's 401 handler
      });
    } else {
      // Token is still good — ensure proactive refresh is scheduled
      scheduleProactiveRefresh(token);
    }
  });
}

// ── Token Refresh ───────────────────────────────────────────────────────────

let isRefreshing = false;
let refreshPromise: Promise<string> | null = null;

async function refreshAccessToken(): Promise<string> {
  if (isRefreshing && refreshPromise) {
    return refreshPromise;
  }

  isRefreshing = true;
  refreshPromise = (async () => {
    try {
      const res = await fetch(`${BASE}/auth/refresh`, {
        method: "POST",
        credentials: "include",
        headers: { "X-Requested-With": "AnyServer" },
      });

      if (!res.ok) {
        const err: ApiError = await res.json().catch(() => ({
          error: res.statusText || `HTTP ${res.status}`,
          details: null,
        }));
        throw new ApiClientError(res.status, err);
      }

      const data: { token: string } = await res.json();
      setToken(data.token);
      return data.token;
    } finally {
      isRefreshing = false;
      refreshPromise = null;
    }
  })();

  return refreshPromise;
}

// ── API Request ─────────────────────────────────────────────────────────────

export async function request<TResponse>(
  method: string,
  path: string,
  body?: unknown,
  options?: {
    noAuth?: boolean;
    _isRetry?: boolean;
    _isRateLimitRetry?: boolean;
  },
): Promise<TResponse> {
  // If the session is dead, don't bother making requests — the app is
  // redirecting to /login. Return a never-resolving promise to prevent
  // callers from seeing errors or stale data.
  if (sessionDead && !options?.noAuth) {
    return new Promise<never>(() => {});
  }

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  let usedToken: string | null = null;
  if (!options?.noAuth) {
    usedToken = getToken();
    if (usedToken) {
      headers["Authorization"] = `Bearer ${usedToken}`;
    }
  }

  const opts: RequestInit = {
    method,
    headers,
    credentials: "include",
  };

  if (body !== undefined) {
    opts.body = JSON.stringify(body);
  }

  const res = await fetch(`${BASE}${path}`, opts);

  // ── 429 Too Many Requests — auto-retry once with countdown banner ──
  if (res.status === 429) {
    const retryAfterSecs = parseInt(res.headers.get("retry-after") ?? "10", 10);

    // First 429: emit the global banner event, wait, then retry once.
    if (!options?._isRateLimitRetry) {
      emitRateLimitEvent(retryAfterSecs);
      await new Promise((r) => setTimeout(r, retryAfterSecs * 1000));
      return request<TResponse>(method, path, body, {
        ...options,
        _isRateLimitRetry: true,
      });
    }

    // Already retried once — surface the error so the UI can handle it.
    throw new RateLimitError(retryAfterSecs);
  }

  if (res.status === 401 && !options?._isRetry && !options?.noAuth) {
    // ── Cross-tab recovery ──
    // Before attempting our own refresh, check if another tab already
    // refreshed the token (it would be in localStorage). This avoids
    // redundant refresh calls and the associated token rotation race.
    const storedToken = readPersistedToken();
    if (storedToken && storedToken !== usedToken) {
      accessToken = storedToken;
      scheduleProactiveRefresh(storedToken);
      return request<TResponse>(method, path, body, {
        ...options,
        _isRetry: true,
      });
    }

    // ── Self-refresh ──
    try {
      await refreshAccessToken();
      return request<TResponse>(method, path, body, {
        ...options,
        _isRetry: true,
      });
    } catch (_refreshError) {
      // Refresh failed — session is irrecoverable.
      sessionDead = true;
      accessToken = null;
      cancelProactiveRefresh();
      removePersistedToken();

      // Notify other tabs
      broadcastSessionExpired();

      // Notify local auth context (which will set user=null,
      // triggering the router redirect to /login)
      notifyAuthListeners("session_expired");

      // Return a never-resolving promise so the caller doesn't
      // continue with stale/error state while the redirect happens.
      return new Promise<never>(() => {});
    }
  }

  if (!res.ok) {
    const err: ApiError = await res.json().catch(() => ({
      error: res.statusText || `HTTP ${res.status}`,
      details: null,
    }));
    throw new ApiClientError(res.status, err);
  }

  // 204 No Content — nothing to parse.
  if (res.status === 204) {
    return undefined as TResponse;
  }

  return res.json() as Promise<TResponse>;
}

// ── Session Manager Initialization ──────────────────────────────────────────

let sessionManagerStarted = false;

/**
 * Start the cross-tab sync, proactive refresh, and visibility handlers.
 * Safe to call multiple times — only the first call has effect.
 * Should be called early in the app lifecycle (e.g. from AuthProvider).
 */
export function startSessionManager(): void {
  if (sessionManagerStarted) return;
  sessionManagerStarted = true;

  initCrossTabSync();
  initVisibilityHandler();

  // If we already have a token (restored from localStorage), kick off
  // the proactive refresh scheduler.
  const token = getToken();
  if (token) {
    scheduleProactiveRefresh(token);
  }
}

export async function initializeAuth(): Promise<void> {
  if (typeof window !== "undefined" && (window as any).__E2E_AUTH_TOKEN__) {
    accessToken = (window as any).__E2E_AUTH_TOKEN__;
    console.log(
      "[Auth] E2E token detected in initializeAuth, skipping refresh",
    );
    return;
  }

  // Start the session manager (cross-tab sync, visibility, proactive refresh).
  // This is idempotent so it's fine if AuthProvider also calls it.
  startSessionManager();

  // If we have a token from localStorage, check if it's still usable.
  const existing = restoreToken();
  if (existing) {
    if (!isTokenExpiringSoon(existing, 30_000)) {
      // Token has >30s of life left — use it as-is. The proactive
      // refresh scheduler (started above) will refresh it before it
      // expires.
      return;
    }

    // Token is expired or about to expire — try to refresh.
    try {
      await refreshAccessToken();
      return;
    } catch {
      // Refresh failed — fall through to clear state
      clearToken();
      return;
    }
  }

  // No stored token — try to refresh using the HttpOnly refresh cookie.
  // This handles the case where the user still has a valid refresh cookie
  // but no access token (e.g. cleared browser storage but not cookies).
  try {
    await refreshAccessToken();
  } catch (_error) {
    clearToken();
  }
}
