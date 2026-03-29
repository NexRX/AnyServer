import {
  type Component,
  createSignal,
  Show,
} from "solid-js";
import { changePassword } from "../../api/client";

const PasswordTab: Component = () => {
  const [currentPassword, setCurrentPassword] = createSignal("");
  const [newPassword, setNewPassword] = createSignal("");
  const [confirmPassword, setConfirmPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal(false);
  const [submitting, setSubmitting] = createSignal(false);

  const validate = (): string | null => {
    if (!currentPassword()) return "Current password is required.";
    if (!newPassword()) return "New password is required.";
    if (newPassword().length < 6)
      return "New password must be at least 6 characters.";
    if (newPassword().length > 256)
      return "New password must be at most 256 characters.";
    if (newPassword() !== confirmPassword())
      return "New passwords do not match.";
    if (currentPassword() === newPassword())
      return "New password must be different from the current password.";
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
    setSuccess(false);

    try {
      await changePassword({
        current_password: currentPassword(),
        new_password: newPassword(),
      });
      setSuccess(true);
      setCurrentPassword("");
      setNewPassword("");
      setConfirmPassword("");
      setTimeout(() => setSuccess(false), 5000);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to change password.");
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
    <div class="admin-password" style={{ "max-width": "480px" }}>
      <h2>Change Your Password</h2>

      <Show when={error()}>
        {(err) => <div class="error-msg">{err()}</div>}
      </Show>

      <Show when={success()}>
        <div
          style={{
            background: "var(--success-bg)",
            border: "1px solid rgba(34, 197, 94, 0.3)",
            "border-radius": "var(--radius-sm)",
            padding: "0.7rem 1rem",
            color: "var(--success)",
            "margin-bottom": "1rem",
            "font-size": "0.9rem",
          }}
        >
          ✓ Password changed successfully.
        </div>
      </Show>

      <form class="auth-form" onSubmit={handleSubmit}>
        <div class="form-group">
          <label for="pwd-current">Current Password</label>
          <input
            id="pwd-current"
            type="password"
            value={currentPassword()}
            onInput={(e) => {
              setCurrentPassword(e.currentTarget.value);
              if (error()) setError(null);
            }}
            onKeyDown={handleKeyDown}
            placeholder="Enter your current password"
            autocomplete="current-password"
          />
        </div>

        <div class="form-group">
          <label for="pwd-new">New Password</label>
          <input
            id="pwd-new"
            type="password"
            value={newPassword()}
            onInput={(e) => {
              setNewPassword(e.currentTarget.value);
              if (error()) setError(null);
            }}
            onKeyDown={handleKeyDown}
            placeholder="Enter a new password"
            autocomplete="new-password"
          />
          <small>At least 6 characters.</small>
        </div>

        <div class="form-group">
          <label for="pwd-confirm">Confirm New Password</label>
          <input
            id="pwd-confirm"
            type="password"
            value={confirmPassword()}
            onInput={(e) => {
              setConfirmPassword(e.currentTarget.value);
              if (error()) setError(null);
            }}
            onKeyDown={handleKeyDown}
            placeholder="Re-enter the new password"
            autocomplete="new-password"
          />
        </div>

        <button type="submit" class="btn btn-primary" disabled={submitting()}>
          {submitting() ? "Changing..." : "Change Password"}
        </button>
      </form>
    </div>
  );
};

export default PasswordTab;
