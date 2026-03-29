import {
  type Component,
  type ParentProps,
  createContext,
  useContext,
  createSignal,
  createResource,
  onMount,
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
  RateLimitError,
} from "../api/client";

export interface AuthState {
  user: () => UserPublic | null;
  settings: () => AppSettings | null;
  loading: () => boolean;
  isLoggedIn: () => boolean;
  isAdmin: () => boolean;
  isSetupComplete: () => boolean;
  isRegistrationEnabled: () => boolean;
  isRunCommandsAllowed: () => boolean;
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
          // If the failure was a rate-limit error, use its Retry-After
          // duration instead of the fixed backoff so we don't hammer the
          // server while it's telling us to wait.
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
          // Batch both updates so that dependent computations (navbar
          // visibility, shouldRedirect, etc.) only see the final consistent
          // state rather than an intermediate where user is set but settings
          // haven't been overwritten yet, or vice-versa.
          batch(() => {
            setUser(me.user);
            setSettings(me.settings);
          });
        } catch (err: unknown) {
          console.warn("[Auth] Token validation failed, logging out:", err);
          // Batch the "logged-out" state transition so the navbar and
          // redirect logic see user=null and token=cleared atomically.
          // Without this, setUser(null) fires first causing the navbar
          // to disappear before shouldRedirect navigates to /login.
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

  const logout = async () => {
    try {
      await apiLogout();
    } catch (err: unknown) {
      console.warn("[Auth] Logout API call failed:", err);
    }
    batch(() => {
      clearToken();
      setUser(null);
    });
  };

  onMount(() => {
    refresh();
  });

  const state: AuthState = {
    user,
    settings,
    loading,
    isLoggedIn: () => user() !== null,
    isAdmin: () => user()?.role === "admin",
    isSetupComplete: () => settings()?.setup_complete ?? false,
    isRegistrationEnabled: () => settings()?.registration_enabled ?? false,
    isRunCommandsAllowed: () => settings()?.allow_run_commands ?? false,
    refresh,
    logout,
    setUser,
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
