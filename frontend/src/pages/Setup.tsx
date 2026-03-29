import {
  type Component,
  createSignal,
  createEffect,
  onCleanup,
  Show,
} from "solid-js";
import { A, useNavigate } from "@solidjs/router";
import { setup, getAuthStatus, RateLimitError } from "../api/client";
import { useAuth } from "../context/auth";
import Loader from "../components/Loader";

const Setup: Component = () => {
  const navigate = useNavigate();
  const auth = useAuth();

  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [confirmPassword, setConfirmPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  const [rateLimitCountdown, setRateLimitCountdown] = createSignal(0);

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

  // Reactively redirect away once we *know* setup is already done.
  // This must be an effect (not a one-shot check) because `settings()`
  // is still null while the auth context is loading on first render.
  // We explicitly guard against null settings — `isSetupComplete()`
  // returns false both when setup is genuinely incomplete AND when
  // settings haven't loaded yet; we must only redirect in the former case.
  createEffect(() => {
    if (auth.settings() !== null && auth.isSetupComplete()) {
      navigate("/login", { replace: true });
    }
  });

  const validate = (): string | null => {
    const u = username().trim();
    const p = password();
    const cp = confirmPassword();

    if (!u) return "Username is required.";
    if (u.length < 3) return "Username must be at least 3 characters.";
    if (u.length > 32) return "Username must be at most 32 characters.";
    if (!/^[a-zA-Z0-9_-]+$/.test(u))
      return "Username may only contain letters, digits, underscores, and hyphens.";
    if (!p) return "Password is required.";
    if (p.length < 6) return "Password must be at least 6 characters.";
    if (p.length > 256) return "Password must be at most 256 characters.";
    if (p !== cp) return "Passwords do not match.";
    return null;
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();

    const validationError = validate();
    if (validationError) {
      setError(validationError);
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const resp = await setup({
        username: username().trim().toLowerCase(),
        password: password(),
      });
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
        setError(
          "An unexpected error occurred during setup. Please try again.",
        );
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

  // Don't render the setup form until we know whether setup is actually
  // needed.  While settings are null the auth context is still loading;
  // showing the form prematurely would confuse users who have already
  // completed setup (they'd briefly see it before the redirect fires).
  const settingsKnown = () => auth.settings() !== null;

  return (
    <Show when={settingsKnown()} fallback={<Loader message="Loading" />}>
      <div class="auth-page">
        <div class="auth-card">
          <div class="auth-header">
            <span class="auth-logo">⚡</span>
            <h1>Welcome to AnyServer</h1>
            <p class="auth-subtitle">
              Let's get started by creating your administrator account. This
              will be the first user with full control over AnyServer.
            </p>
          </div>

          <Show when={error()}>
            {(err) => <div class="error-msg">{err()}</div>}
          </Show>

          <form class="auth-form" onSubmit={handleSubmit}>
            <div class="form-group">
              <label for="setup-username">Admin Username</label>
              <input
                id="setup-username"
                type="text"
                value={username()}
                onInput={(e) => {
                  setUsername(e.currentTarget.value);
                  if (error()) setError(null);
                }}
                onKeyDown={handleKeyDown}
                placeholder="Choose a username (e.g. admin)"
                autocomplete="username"
                autofocus
              />
              <small>
                3–32 characters. Letters, digits, underscores, and hyphens only.
              </small>
            </div>

            <div class="form-group">
              <label for="setup-password">Password</label>
              <input
                id="setup-password"
                type="password"
                value={password()}
                onInput={(e) => {
                  setPassword(e.currentTarget.value);
                  if (error()) setError(null);
                }}
                onKeyDown={handleKeyDown}
                placeholder="Choose a strong password"
                autocomplete="new-password"
              />
              <small>At least 6 characters.</small>
            </div>

            <div class="form-group">
              <label for="setup-confirm">Confirm Password</label>
              <input
                id="setup-confirm"
                type="password"
                value={confirmPassword()}
                onInput={(e) => {
                  setConfirmPassword(e.currentTarget.value);
                  if (error()) setError(null);
                }}
                onKeyDown={handleKeyDown}
                placeholder="Re-enter your password"
                autocomplete="new-password"
              />
            </div>

            <button
              type="submit"
              class="btn btn-primary auth-submit"
              disabled={submitting() || rateLimitCountdown() > 0}
            >
              {submitting()
                ? "Creating admin account..."
                : rateLimitCountdown() > 0
                  ? `Try again in ${rateLimitCountdown()}s`
                  : "Create Admin Account"}
            </button>
          </form>

          <div class="auth-footer">
            <p class="auth-hint">
              This page is only available once — before any user accounts exist.
              After creating the admin account, you can manage additional users
              and enable self-registration from the admin panel.
            </p>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default Setup;
