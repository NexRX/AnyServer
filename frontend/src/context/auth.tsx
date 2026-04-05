import {
  type Component,
  type ParentProps,
  createContext,
  useContext,
  createSignal,
  onMount,
  onCleanup,
  batch,
} from "solid-js";
import type { UserPublic, AppSettings } from "../types/bindings";
import {
  getToken,
  clearToken,
  getMe,
  getAuthStatus,
  isLoggedIn as checkToken,
  initializeAuth,
  logout as apiLogout,
  startSessionManager,
  onAuthStateChange,
  isTokenExpiringSoon,
  RateLimitError,
} from "../api/client";
import type { AuthEvent } from "../api/client";

export interface AuthState {
  user: () => UserPublic | null;
  settings: () => AppSettings | null;
  loading: () => boolean;
  isLoggedIn: () => boolean;
  isAdmin: () => boolean;
  isSetupComplete: () => boolean;
  isRegistrationEnabled: () => boolean;
  isRunCommandsAllowed: () => boolean;
  /** Non-null when the user was deauthenticated for a specific reason. */
  deauthReason: () => "session_expired" | null;
  refresh: () => Promise<void>;
  logout: () => void;
  setUser: (user: UserPublic) => void;
  setSettings: (settings: AppSettings) => void;
}

const AuthContext = createContext<AuthState>();

export const AuthProvider: Component<ParentProps> = (props) => {
  const [user, setUser] = createSignal<UserPublic | null>(null);
  const [settings, setSettings] = createSignal<AppSettings | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [deauthReason, setDeauthReason] = createSignal<
    "session_expired" | null
  >(null);

  // ── Settings fetch with retry ──────────────────────────────────────────

  const fetchSettingsWithRetry = async (
    retries = 3,
    delayMs = 250,
  ): Promise<boolean> => {
    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        const authStatus = await getAuthStatus();
        setSettings(authStatus);
        return true;
      } catch (err: unknown) {
        console.warn(
          `[Auth] getAuthStatus attempt ${attempt}/${retries} failed:`,
          err,
        );
        if (attempt < retries) {
          const delay =
            err instanceof RateLimitError
              ? err.retryAfterSecs * 1000
              : delayMs * attempt;
          await new Promise((r) => setTimeout(r, delay));
        }
      }
    }
    return false;
  };

  // ── Core refresh logic ─────────────────────────────────────────────────

  const refresh = async () => {
    setLoading(true);
    try {
      const gotSettings = await fetchSettingsWithRetry();
      if (!gotSettings) {
        console.error("[Auth] Failed to fetch auth status after retries");
        return;
      }

      await initializeAuth();

      if (checkToken()) {
        try {
          const me = await getMe();
          batch(() => {
            setUser(me.user);
            setSettings(me.settings);
            setDeauthReason(null);
          });
        } catch (err: unknown) {
          console.warn("[Auth] Token validation failed, logging out:", err);
          batch(() => {
            clearToken();
            setUser(null);
          });
        }
      } else {
        setUser(null);
      }
    } catch (err: unknown) {
      console.error("[Auth] Unexpected error during auth refresh:", err);
    } finally {
      setLoading(false);
    }
  };

  // ── Logout ─────────────────────────────────────────────────────────────

  const logout = async () => {
    try {
      await apiLogout();
    } catch (err: unknown) {
      console.warn("[Auth] Logout API call failed:", err);
    }
    batch(() => {
      clearToken();
      setUser(null);
      setDeauthReason(null);
    });
  };

  // ── Cross-tab auth event handler ───────────────────────────────────────
  //
  // `onAuthStateChange` fires when:
  //   - Another tab refreshed the access token ("token_refreshed")
  //   - Another tab explicitly logged out ("logged_out")
  //   - Any tab's refresh attempt failed irrecoverably ("session_expired")

  const handleAuthEvent = (event: AuthEvent) => {
    switch (event) {
      case "token_refreshed": {
        // Another tab got a fresh token. If we don't currently have a
        // user (e.g. this tab was on /login), try to hydrate.
        if (user() === null && checkToken()) {
          // Re-validate the session in the background — don't block.
          getMe()
            .then((me) => {
              batch(() => {
                setUser(me.user);
                setSettings(me.settings);
                setDeauthReason(null);
              });
            })
            .catch(() => {
              // Token might be for a different session or already
              // invalid — just ignore; the proactive refresh or next
              // API call will sort it out.
            });
        }
        break;
      }

      case "logged_out": {
        // Another tab logged out — mirror locally.
        batch(() => {
          setUser(null);
          setDeauthReason(null);
        });
        break;
      }

      case "session_expired": {
        // Session is irrecoverably dead across all tabs.
        batch(() => {
          setUser(null);
          setDeauthReason("session_expired");
        });
        break;
      }
    }
  };

  // ── Visibility-based re-validation ─────────────────────────────────────
  //
  // When a tab comes back from the background, the proactive refresh timer
  // in core.ts handles token freshness. Here we handle the higher-level
  // concern: if the token was cleared (by another tab) while we were hidden,
  // update the user signal to trigger the redirect to /login.
  //
  // We also re-validate the user profile if the tab was hidden for a long
  // time, since the user's role/permissions may have changed.

  let lastVisibleAt = Date.now();

  const handleVisibilityChange = () => {
    if (document.visibilityState !== "visible") return;

    const now = Date.now();
    const hiddenDuration = now - lastVisibleAt;
    lastVisibleAt = now;

    const token = getToken();

    if (!token) {
      // Token was cleared while we were hidden
      if (user() !== null) {
        batch(() => {
          setUser(null);
          // Don't set deauthReason here — the cross-tab event handler
          // already set it if this was a session_expired scenario.
        });
      }
      return;
    }

    // If the tab was hidden for more than 5 minutes, re-validate the
    // user profile to pick up role/permission changes.
    if (hiddenDuration > 5 * 60 * 1000 && user() !== null) {
      getMe()
        .then((me) => {
          batch(() => {
            setUser(me.user);
            setSettings(me.settings);
          });
        })
        .catch(() => {
          // If this fails, the 401 handler in core.ts will take care of
          // refreshing or deauthing. No need to act here.
        });
    }
  };

  // ── Lifecycle ──────────────────────────────────────────────────────────

  onMount(() => {
    // Start the session manager (cross-tab sync, proactive refresh,
    // visibility-aware token management in core.ts). Idempotent.
    startSessionManager();

    // Subscribe to cross-tab auth events.
    const unsubscribe = onAuthStateChange(handleAuthEvent);
    onCleanup(unsubscribe);

    // Listen for tab visibility changes.
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", handleVisibilityChange);
      onCleanup(() => {
        document.removeEventListener(
          "visibilitychange",
          handleVisibilityChange,
        );
      });
    }

    // Perform the initial auth check.
    refresh();
  });

  // ── Context value ──────────────────────────────────────────────────────

  const state: AuthState = {
    user,
    settings,
    loading,
    isLoggedIn: () => user() !== null,
    isAdmin: () => user()?.role === "admin",
    isSetupComplete: () => settings()?.setup_complete ?? false,
    isRegistrationEnabled: () => settings()?.registration_enabled ?? false,
    isRunCommandsAllowed: () => settings()?.allow_run_commands ?? false,
    deauthReason,
    refresh,
    logout,
    setUser: (u: UserPublic) => {
      setUser(u);
      setDeauthReason(null);
    },
    setSettings,
  };

  return (
    <AuthContext.Provider value={state}>{props.children}</AuthContext.Provider>
  );
};

export function useAuth(): AuthState {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth() must be used within an <AuthProvider>");
  }
  return ctx;
}
