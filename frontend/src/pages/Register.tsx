import { type Component, createSignal, onCleanup, Show } from "solid-js";
import { A, useNavigate } from "@solidjs/router";
import { register, getAuthStatus, RateLimitError } from "../api/client";
import { useAuth } from "../context/auth";

const Register: Component = () => {
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

  if (!auth.isSetupComplete()) {
    navigate("/setup", { replace: true });
  }

  if (auth.isSetupComplete() && !auth.isRegistrationEnabled()) {
    navigate("/login", { replace: true });
  }

  if (auth.isLoggedIn()) {
    navigate("/", { replace: true });
  }

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
      const resp = await register({
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
          "An unexpected error occurred during registration. Please try again.",
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

  return (
    <div class="auth-page">
      <div class="auth-card">
        <div class="auth-header">
          <span class="auth-logo">⚡</span>
          <h1>AnyServer</h1>
          <p class="auth-subtitle">Create a new account</p>
        </div>

        <Show when={error()}>
          {(err) => <div class="error-msg">{err()}</div>}
        </Show>

        <form class="auth-form" onSubmit={handleSubmit}>
          <div class="form-group">
            <label for="register-username">Username</label>
            <input
              id="register-username"
              type="text"
              value={username()}
              onInput={(e) => {
                setUsername(e.currentTarget.value);
                if (error()) setError(null);
              }}
              onKeyDown={handleKeyDown}
              placeholder="Choose a username"
              autocomplete="username"
              autofocus
            />
            <small>
              3–32 characters. Letters, digits, underscores, and hyphens only.
            </small>
          </div>

          <div class="form-group">
            <label for="register-password">Password</label>
            <input
              id="register-password"
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
            <label for="register-confirm">Confirm Password</label>
            <input
              id="register-confirm"
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
              ? "Creating account..."
              : rateLimitCountdown() > 0
                ? `Try again in ${rateLimitCountdown()}s`
                : "Create Account"}
          </button>
        </form>

        <div class="auth-footer">
          <p>
            Already have an account?{" "}
            <A href="/login" class="auth-link">
              Sign in
            </A>
          </p>
        </div>
      </div>
    </div>
  );
};

export default Register;
