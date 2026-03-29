import {
  type Component,
  createSignal,
  Show,
} from "solid-js";
import Loader from "../Loader";
import {
  getSmtpConfig,
  saveSmtpConfig,
  deleteSmtpConfig,
  sendTestEmail,
} from "../../api/client";

const SmtpTab: Component = () => {
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [testing, setTesting] = createSignal(false);
  const [deleting, setDeleting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);

  const [host, setHost] = createSignal("");
  const [port, setPort] = createSignal(587);
  const [tls, setTls] = createSignal(true);
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [fromAddress, setFromAddress] = createSignal("");
  const [passwordSet, setPasswordSet] = createSignal(false);
  const [testRecipient, setTestRecipient] = createSignal("");

  // Load existing SMTP config
  const loadConfig = async () => {
    setLoading(true);
    try {
      const config = await getSmtpConfig();
      if (config) {
        setHost(config.host);
        setPort(config.port);
        setTls(config.tls);
        setUsername(config.username);
        setFromAddress(config.from_address);
        setPasswordSet(config.password_set);
        setPassword("");
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load SMTP config");
    } finally {
      setLoading(false);
    }
  };

  loadConfig();

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      const result = await saveSmtpConfig({
        host: host(),
        port: port(),
        tls: tls(),
        username: username(),
        password: password() || (undefined as unknown as string),
        from_address: fromAddress(),
      });
      setPasswordSet(result.password_set);
      setPassword("");
      setSuccess("SMTP configuration saved successfully.");
      setTimeout(() => setSuccess(null), 4000);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to save SMTP config");
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (
      !confirm(
        "Are you sure you want to remove the SMTP configuration? Email alerts will stop working.",
      )
    )
      return;
    setDeleting(true);
    setError(null);
    try {
      await deleteSmtpConfig();
      setHost("");
      setPort(587);
      setTls(true);
      setUsername("");
      setPassword("");
      setFromAddress("");
      setPasswordSet(false);
      setSuccess("SMTP configuration removed.");
      setTimeout(() => setSuccess(null), 4000);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to delete SMTP config");
    } finally {
      setDeleting(false);
    }
  };

  const handleTestEmail = async () => {
    if (!testRecipient().trim()) {
      setError("Enter a recipient email address for the test.");
      return;
    }
    setTesting(true);
    setError(null);
    setSuccess(null);
    try {
      const result = await sendTestEmail({ recipient: testRecipient().trim() });
      if (result.success) {
        setSuccess(`Test email sent to ${testRecipient().trim()}`);
        setTimeout(() => setSuccess(null), 5000);
      } else {
        setError(`Test email failed: ${result.error || "Unknown error"}`);
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to send test email");
    } finally {
      setTesting(false);
    }
  };

  return (
    <div class="admin-settings" style={{ "max-width": "560px" }}>
      <h2>SMTP Configuration</h2>
      <p
        style={{
          color: "var(--text-muted)",
          "font-size": "0.9rem",
          "margin-bottom": "1.5rem",
        }}
      >
        Configure an SMTP server to enable email alerts. Credentials are stored
        securely and never returned via the API.
      </p>

      <Show when={error()}>
        {(err) => (
          <div class="error-msg" style={{ "margin-bottom": "1rem" }}>
            {err()}
          </div>
        )}
      </Show>
      <Show when={success()}>
        {(msg) => (
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
            ✓ {msg()}
          </div>
        )}
      </Show>

      <Show
        when={!loading()}
        fallback={<Loader message="Loading SMTP configuration" />}
      >
        <form
          class="auth-form"
          onSubmit={(e) => {
            e.preventDefault();
            handleSave();
          }}
        >
          <div class="form-group">
            <label for="smtp-host">SMTP Host</label>
            <input
              id="smtp-host"
              type="text"
              value={host()}
              onInput={(e) => setHost(e.currentTarget.value)}
              placeholder="smtp.gmail.com"
            />
          </div>

          <div style={{ display: "flex", gap: "1rem" }}>
            <div class="form-group" style={{ flex: 1 }}>
              <label for="smtp-port">Port</label>
              <input
                id="smtp-port"
                type="number"
                value={port()}
                onInput={(e) => setPort(parseInt(e.currentTarget.value) || 587)}
              />
            </div>
            <div
              class="form-group"
              style={{
                flex: 1,
                display: "flex",
                "align-items": "center",
                "padding-top": "1.5rem",
              }}
            >
              <label
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "0.5rem",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={tls()}
                  onChange={(e) => setTls(e.currentTarget.checked)}
                />
                Use TLS
              </label>
            </div>
          </div>

          <div class="form-group">
            <label for="smtp-username">Username</label>
            <input
              id="smtp-username"
              type="text"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
              placeholder="alerts@example.com"
              autocomplete="off"
            />
          </div>

          <div class="form-group">
            <label for="smtp-password">
              Password
              <Show when={passwordSet()}>
                <span
                  style={{
                    color: "var(--text-dim)",
                    "font-weight": "normal",
                    "margin-left": "0.5rem",
                    "font-size": "0.85rem",
                  }}
                >
                  (set — leave blank to keep)
                </span>
              </Show>
            </label>
            <input
              id="smtp-password"
              type="password"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              placeholder={passwordSet() ? "••••••••" : "Enter password"}
              autocomplete="new-password"
            />
          </div>

          <div class="form-group">
            <label for="smtp-from">From Address</label>
            <input
              id="smtp-from"
              type="text"
              value={fromAddress()}
              onInput={(e) => setFromAddress(e.currentTarget.value)}
              placeholder="AnyServer <alerts@example.com>"
            />
          </div>

          <div
            style={{ display: "flex", gap: "0.5rem", "margin-top": "0.5rem" }}
          >
            <button type="submit" class="btn btn-primary" disabled={saving()}>
              {saving() ? "Saving..." : "Save SMTP Config"}
            </button>
            <Show when={passwordSet()}>
              <button
                type="button"
                class="btn btn-danger-outline"
                onClick={handleDelete}
                disabled={deleting()}
              >
                {deleting() ? "Removing..." : "Remove"}
              </button>
            </Show>
          </div>
        </form>

        <Show when={passwordSet()}>
          <hr
            style={{
              border: "none",
              "border-top": "1px solid var(--border)",
              margin: "1.5rem 0",
            }}
          />
          <h3 style={{ "margin-bottom": "0.75rem" }}>Send Test Email</h3>
          <div
            style={{
              display: "flex",
              gap: "0.5rem",
              "align-items": "flex-end",
            }}
          >
            <div class="form-group" style={{ flex: 1, "margin-bottom": 0 }}>
              <label for="test-recipient">Recipient</label>
              <input
                id="test-recipient"
                type="email"
                value={testRecipient()}
                onInput={(e) => setTestRecipient(e.currentTarget.value)}
                placeholder="you@example.com"
              />
            </div>
            <button
              class="btn btn-sm"
              onClick={handleTestEmail}
              disabled={testing()}
              style={{ "margin-bottom": "0.25rem" }}
            >
              {testing() ? "Sending..." : "Send Test"}
            </button>
          </div>
        </Show>
      </Show>
    </div>
  );
};

export default SmtpTab;
