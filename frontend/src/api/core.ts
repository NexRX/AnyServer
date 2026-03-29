import { createSignal } from "solid-js";
import type { ApiError } from "../types/bindings";

const BASE = "/api";
const TOKEN_STORAGE_KEY = "anyserver_access_token";

let accessToken: string | null = null;

/**
 * Attempt to restore the access token from sessionStorage.
 * This allows the token to survive hard refreshes without needing
 * to hit the refresh endpoint (which triggers token rotation).
 */
function restoreToken(): string | null {
  if (accessToken) return accessToken;
  try {
    const stored = sessionStorage.getItem(TOKEN_STORAGE_KEY);
    if (stored) {
      accessToken = stored;
    }
  } catch {
    // sessionStorage may be unavailable (e.g. in some privacy modes)
  }
  return accessToken;
}

function persistToken(token: string): void {
  try {
    sessionStorage.setItem(TOKEN_STORAGE_KEY, token);
  } catch {
    // Silently ignore storage errors
  }
}

function removePersistedToken(): void {
  try {
    sessionStorage.removeItem(TOKEN_STORAGE_KEY);
  } catch {
    // Silently ignore storage errors
  }
}

export function getToken(): string | null {
  if (typeof window !== "undefined" && (window as any).__E2E_AUTH_TOKEN__) {
    const e2eToken = (window as any).__E2E_AUTH_TOKEN__;
    if (accessToken !== e2eToken) {
      accessToken = e2eToken;
      console.log(
        "[Auth] E2E token detected and applied:",
        accessToken?.substring(0, 20) + "...",
      );
    }
  }
  return restoreToken();
}

export function setToken(token: string): void {
  accessToken = token;
  persistToken(token);
}

export function clearToken(): void {
  accessToken = null;
  removePersistedToken();
}

export function isLoggedIn(): boolean {
  return getToken() !== null;
}

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

// ── Rate-limit reactive signal ──────────────────────────────────────
// Absolute timestamp (ms) when requests are allowed again, or null when
// there is no active rate limit.
const [rateLimitRetryAt, setRateLimitRetryAt] = createSignal<number | null>(
  null,
);

export { rateLimitRetryAt };

export function emitRateLimitEvent(retryAfterSecs: number): void {
  setRateLimitRetryAt(Date.now() + retryAfterSecs * 1000);
}

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
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  if (!options?.noAuth) {
    const token = getToken();
    if (token) {
      headers["Authorization"] = `Bearer ${token}`;
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
    try {
      await refreshAccessToken();
      return request<TResponse>(method, path, body, {
        ...options,
        _isRetry: true,
      });
    } catch (refreshError) {
      clearToken();
      // Don't throw — redirect to login with a session-expired flag.
      // A hard redirect ensures all in-flight requests and reactive
      // computations are abandoned cleanly.
      window.location.href = "/login?reason=session_expired";
      // Return a never-resolving promise to prevent the caller from
      // continuing with stale/error state while the redirect happens.
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

  return res.json() as Promise<TResponse>;
}

export async function initializeAuth(): Promise<void> {
  if (typeof window !== "undefined" && (window as any).__E2E_AUTH_TOKEN__) {
    accessToken = (window as any).__E2E_AUTH_TOKEN__;
    console.log(
      "[Auth] E2E token detected in initializeAuth, skipping refresh",
    );
    return;
  }

  // If we already have a token restored from sessionStorage, skip the
  // refresh call entirely.  The token may be expired, but that's fine —
  // the 401-retry logic in `request()` will transparently refresh it on
  // the first real API call that needs auth.  This avoids hitting the
  // refresh endpoint on every hard reload, which previously caused
  // refresh-token rotation race conditions: if the browser hadn't yet
  // stored the new Set-Cookie before the page reloaded, the old
  // (now-revoked) refresh token would be sent, triggering reuse
  // detection and revoking the entire token family.
  if (restoreToken()) {
    return;
  }

  try {
    await refreshAccessToken();
  } catch (error) {
    clearToken();
  }
}
