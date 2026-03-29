import {
  type Component,
  createSignal,
  createEffect,
  onCleanup,
  Show,
} from "solid-js";
import { A, useNavigate, useSearchParams } from "@solidjs/router";
import {
  login,
  getAuthStatus,
  redeemInviteCode,
  RateLimitError,
} from "../api/client";
import { useAuth } from "../context/auth";

const Login: Component = () => {
  const navigate = useNavigate();
  const auth = useAuth();
  const [searchParams, setSearchParams] = useSearchParams();

  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  const [rateLimitCountdown, setRateLimitCountdown] = createSignal(0);

  // Show session-expired banner when redirected from an expired session
  const sessionExpired = () => searchParams.reason === "session_expired";

  // Clear the query param after a short delay so refreshing the login
  // page doesn't re-show the message
  createEffect(() => {
    if (searchParams.reason === "session_expired") {
      setTimeout(
        () => setSearchParams({ reason: undefined }, { replace: true }),
        100,
      );
    }
  });

  let rateLimitTimer: ReturnType<typeof setInterval> | null = null;
  const clearRateLimitTimer = () => {
    if (rateLimitTimer !== null) {
      clearInterval(rateLimitTimer);
      rateLimitTimer = null;
    }
  };
  onCleanup(clearRateLimitTimer);

  const startRateLimitCountdown = (secs: number) => {
    clearRateLimitTimer();
    setRateLimitCountdown(secs);
    rateLimitTimer = setInterval(() => {
      const next = rateLimitCountdown() - 1;
      setRateLimitCountdown(next);
      if (next <= 0) clearRateLimitTimer();
    }, 1000);
  };

  // Invite code redemption state
  const [showInvite, setShowInvite] = createSignal(false);
  const [inviteCode, setInviteCode] = createSignal("");
  const [inviteUsername, setInviteUsername] = createSignal("");
  const [invitePassword, setInvitePassword] = createSignal("");
  const [inviteError, setInviteError] = createSignal<string | null>(null);
  const [inviteSubmitting, setInviteSubmitting] = createSignal(false);
  const [inviteSuccess, setInviteSuccess] = createSignal(false);
  const [inviteRateLimitCountdown, setInviteRateLimitCountdown] =
    createSignal(0);

  let inviteRateLimitTimer: ReturnType<typeof setInterval> | null = null;
  const clearInviteRateLimitTimer = () => {
    if (inviteRateLimitTimer !== null) {
      clearInterval(inviteRateLimitTimer);
      inviteRateLimitTimer = null;
    }
  };
  onCleanup(clearInviteRateLimitTimer);

  const startInviteRateLimitCountdown = (secs: number) => {
    clearInviteRateLimitTimer();
    setInviteRateLimitCountdown(secs);
    inviteRateLimitTimer = setInterval(() => {
      const next = inviteRateLimitCountdown() - 1;
      setInviteRateLimitCountdown(next);
      if (next <= 0) clearInviteRateLimitTimer();
    }, 1000);
  };

  // Clear errors/state when toggling between login and invite forms
  const switchToInvite = () => {
    setError(null);
    setShowInvite(true);
  };

  const switchToLogin = () => {
    setInviteError(null);
    setInviteSuccess(false);
    setShowInvite(false);
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();

    const u = username().trim();
    const p = password();

    if (!u) {
      setError("Username is required.");
      return;
    }
    if (!p) {
      setError("Password is required.");
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const resp = await login({ username: u, password: p });
      auth.setUser(resp.user);
      const settings = await getAuthStatus();
      auth.setSettings(settings);
      navigate("/", { replace: true });
    } catch (e: unknown) {
      if (e instanceof RateLimitError) {
        startRateLimitCountdown(e.retryAfterSecs);
        setError(
          `Too many attempts — please wait ${e.retryAfterSecs} seconds and try again.`,
        );
      } else if (e instanceof Error) {
        setError(e.message);
      } else {
        setError("An unexpected error occurred. Please try again.");
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      handleSubmit(e);
    }
  };

  const handleRedeemInvite = async (e: Event) => {
    e.preventDefault();

    const code = inviteCode().trim();
    const u = inviteUsername().trim();
    const p = invitePassword();

    const stripped = code.replace(/[^a-zA-Z0-9]/g, "");
    if (!stripped || stripped.length !== 8) {
      setInviteError("Please enter a valid 8-character invite code.");
      return;
    }
    if (!u) {
      setInviteError("Username is required.");
      return;
    }
    if (!p) {
      setInviteError("Password is required.");
      return;
    }

    setInviteSubmitting(true);
    setInviteError(null);

    try {
      const resp = await redeemInviteCode({
        code,
        username: u,
        password: p,
      });
      auth.setUser(resp.user);
      const settings = await getAuthStatus();
      auth.setSettings(settings);
      setInviteSuccess(true);
      setTimeout(() => navigate("/", { replace: true }), 500);
    } catch (e: unknown) {
      if (e instanceof RateLimitError) {
        startInviteRateLimitCountdown(e.retryAfterSecs);
        setInviteError(
          `Too many attempts — please wait ${e.retryAfterSecs} seconds and try again.`,
        );
      } else if (e instanceof Error) {
        setInviteError(e.message);
      } else {
        setInviteError("Failed to redeem invite code. Please try again.");
      }
    } finally {
      setInviteSubmitting(false);
    }
  };

  return (
    <div class="auth-page">
      <Show
        when={!showInvite()}
        fallback={
          /* ── Invite Code Redemption Card (replaces login card) ── */
          <div class="auth-card">
            <div class="auth-header">
              <span class="auth-logo" style={{ "font-size": "1.5rem" }}>
                🎟️
              </span>
              <h2
                style={{ "font-size": "1.25rem", "margin-bottom": "0.25rem" }}
              >
                Redeem Invite Code
              </h2>
              <p class="auth-subtitle" style={{ "font-size": "0.8rem" }}>
                Enter the invite code from your admin to create your account
              </p>
            </div>

            <Show when={inviteSuccess()}>
              <div
                style={{
                  background: "var(--success-bg)",
                  border: "1px solid var(--success)",
                  "border-radius": "var(--radius-sm)",
                  padding: "0.75rem",
                  color: "var(--success)",
                  "margin-bottom": "1rem",
                  "text-align": "center",
                  "font-weight": "600",
                }}
              >
                ✓ Account created! Redirecting...
              </div>
            </Show>

            <Show when={inviteError()}>
              {(err) => <div class="error-msg">{err()}</div>}
            </Show>

            <form class="auth-form" onSubmit={handleRedeemInvite}>
              <div class="form-group">
                <label for="invite-code">Invite Code</label>
                <input
                  id="invite-code"
                  type="text"
                  maxLength={9}
                  value={inviteCode()}
                  onInput={(e) => {
                    // Strip everything except alphanumeric, uppercase, limit to 8 chars
                    const raw = e.currentTarget.value
                      .replace(/[^a-zA-Z0-9]/g, "")
                      .toUpperCase()
                      .slice(0, 8);
                    // Auto-insert dash after 4th character for readability
                    const formatted =
                      raw.length > 4
                        ? `${raw.slice(0, 4)}-${raw.slice(4)}`
                        : raw;
                    setInviteCode(formatted);
                    if (inviteError()) setInviteError(null);
                  }}
                  placeholder="XXXX-XXXX"
                  autocomplete="off"
                  style={{
                    "text-align": "center",
                    "font-size": "1.5rem",
                    "letter-spacing": "0.3em",
                    "font-family": "var(--mono)",
                    "font-weight": "700",
                  }}
                />
              </div>

              <div class="form-group">
                <label for="invite-username">Choose a Username</label>
                <input
                  id="invite-username"
                  type="text"
                  value={inviteUsername()}
                  onInput={(e) => {
                    setInviteUsername(e.currentTarget.value);
                    if (inviteError()) setInviteError(null);
                  }}
                  placeholder="Enter your desired username"
                  autocomplete="username"
                />
              </div>

              <div class="form-group">
                <label for="invite-password">Choose a Password</label>
                <input
                  id="invite-password"
                  type="password"
                  value={invitePassword()}
                  onInput={(e) => {
                    setInvitePassword(e.currentTarget.value);
                    if (inviteError()) setInviteError(null);
                  }}
                  placeholder="Min 8 chars, upper+lower+digit"
                  autocomplete="new-password"
                />
              </div>

              <button
                type="submit"
                class="btn btn-primary auth-submit"
                disabled={
                  inviteSubmitting() ||
                  inviteCode().replace(/[^a-zA-Z0-9]/g, "").length !== 8 ||
                  inviteRateLimitCountdown() > 0
                }
                style={{
                  background:
                    inviteCode().replace(/[^a-zA-Z0-9]/g, "").length === 8
                      ? "var(--primary)"
                      : undefined,
                }}
              >
                {inviteSubmitting()
                  ? "Creating account..."
                  : inviteRateLimitCountdown() > 0
                    ? `Try again in ${inviteRateLimitCountdown()}s`
                    : "Redeem & Create Account"}
              </button>
            </form>

            <div class="auth-footer">
              <div
                style={{
                  "margin-top": "0.75rem",
                  "border-top": "1px solid var(--border)",
                  "padding-top": "0.75rem",
                }}
              >
                <button
                  class="btn btn-sm"
                  style={{
                    background: "transparent",
                    color: "var(--primary)",
                    border: "1px solid var(--border)",
                    cursor: "pointer",
                    width: "100%",
                    padding: "0.5rem",
                    "border-radius": "var(--radius-sm)",
                    "font-size": "0.85rem",
                    transition: "all var(--transition)",
                  }}
                  onClick={switchToLogin}
                >
                  ← Back to login
                </button>
              </div>
            </div>
          </div>
        }
      >
        {/* ── Login Card ── */}
        <div class="auth-card">
          <div class="auth-header">
            <span class="auth-logo">⚡</span>
            <h1>AnyServer</h1>
            <p class="auth-subtitle">Sign in to your account</p>
          </div>

          <Show when={sessionExpired()}>
            <div
              class="info-msg"
              style={{
                background: "rgba(234, 179, 8, 0.1)",
                border: "1px solid rgba(234, 179, 8, 0.3)",
                "border-radius": "0.5rem",
                padding: "0.75rem",
                color: "#eab308",
                "margin-bottom": "1rem",
                "text-align": "center",
                "font-size": "0.9rem",
              }}
            >
              Your session has expired. Please sign in again.
            </div>
          </Show>

          <Show when={error()}>
            {(err) => <div class="error-msg">{err()}</div>}
          </Show>

          <form class="auth-form" onSubmit={handleSubmit}>
            <div class="form-group">
              <label for="login-username">Username</label>
              <input
                id="login-username"
                type="text"
                value={username()}
                onInput={(e) => {
                  setUsername(e.currentTarget.value);
                  if (error()) setError(null);
                }}
                onKeyDown={handleKeyDown}
                placeholder="Enter your username"
                autocomplete="username"
                autofocus
              />
            </div>

            <div class="form-group">
              <label for="login-password">Password</label>
              <input
                id="login-password"
                type="password"
                value={password()}
                onInput={(e) => {
                  setPassword(e.currentTarget.value);
                  if (error()) setError(null);
                }}
                onKeyDown={handleKeyDown}
                placeholder="Enter your password"
                autocomplete="current-password"
              />
            </div>

            <button
              type="submit"
              class="btn btn-primary auth-submit"
              disabled={submitting() || rateLimitCountdown() > 0}
            >
              {submitting()
                ? "Signing in..."
                : rateLimitCountdown() > 0
                  ? `Try again in ${rateLimitCountdown()}s`
                  : "Sign In"}
            </button>
          </form>

          <div class="auth-footer">
            <Show when={auth.isRegistrationEnabled()}>
              <p>
                Don't have an account?{" "}
                <A href="/register" class="auth-link">
                  Create one
                </A>
              </p>
            </Show>
            <Show when={!auth.isSetupComplete()}>
              <p>
                First time here?{" "}
                <A href="/setup" class="auth-link">
                  Set up AnyServer
                </A>
              </p>
            </Show>

            <Show when={auth.isSetupComplete()}>
              <div
                style={{
                  "margin-top": "0.75rem",
                  "border-top": "1px solid var(--border)",
                  "padding-top": "0.75rem",
                }}
              >
                <button
                  class="btn btn-sm"
                  style={{
                    background: "transparent",
                    color: "var(--primary)",
                    border: "1px solid var(--border)",
                    cursor: "pointer",
                    width: "100%",
                    padding: "0.5rem",
                    "border-radius": "var(--radius-sm)",
                    "font-size": "0.85rem",
                    transition: "all var(--transition)",
                  }}
                  onClick={switchToInvite}
                >
                  🎟️ Have an invite code?
                </button>
              </div>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Login;
