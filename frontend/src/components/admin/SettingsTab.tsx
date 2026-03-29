import {
  type Component,
  createSignal,
  Show,
} from "solid-js";
import { updateSettings } from "../../api/client";
import { useAuth } from "../../context/auth";

const SettingsTab: Component = () => {
  const auth = useAuth();
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal(false);
  const [sandboxMode, setSandboxMode] = createSignal(
    auth.settings()?.run_command_sandbox || "auto",
  );
  const [timeoutSecs, setTimeoutSecs] = createSignal(
    auth.settings()?.run_command_default_timeout_secs || 300,
  );
  const [useNamespaces, setUseNamespaces] = createSignal(
    auth.settings()?.run_command_use_namespaces ?? true,
  );

  const currentSettings = () => ({
    registration_enabled: auth.isRegistrationEnabled(),
    allow_run_commands: auth.isRunCommandsAllowed(),
    run_command_sandbox: sandboxMode(),
    run_command_default_timeout_secs: timeoutSecs(),
    run_command_use_namespaces: useNamespaces(),
  });

  const saveSettings = async (
    overrides: Partial<ReturnType<typeof currentSettings>>,
    afterSave?: (s: ReturnType<typeof currentSettings>) => void,
  ) => {
    setSaving(true);
    setError(null);
    setSuccess(false);

    try {
      const newSettings = await updateSettings({
        ...currentSettings(),
        ...overrides,
      });
      auth.setSettings(newSettings);
      afterSave?.(newSettings);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to update settings.");
    } finally {
      setSaving(false);
    }
  };

  const handleToggleRegistration = async () => {
    const current = auth.isRegistrationEnabled();
    const action = current ? "disable" : "enable";

    if (
      !confirm(`Are you sure you want to ${action} user self-registration?`)
    ) {
      return;
    }

    saveSettings({ registration_enabled: !current });
  };

  const handleToggleRunCommands = async () => {
    const current = auth.isRunCommandsAllowed();
    const action = current ? "disable" : "enable";

    if (
      !confirm(
        `Are you sure you want to ${action} RunCommand pipeline steps?\n\n` +
          `${!current ? "⚠️ WARNING: This allows templates to execute arbitrary shell commands with the privileges of the AnyServer process. Only enable this if you trust all templates you import." : "This will prevent templates from executing shell commands during installation, update, and other pipeline phases."}`,
      )
    ) {
      return;
    }

    saveSettings({ allow_run_commands: !current });
  };

  const handleSandboxModeChange = async (mode: string) => {
    saveSettings({ run_command_sandbox: mode }, () => setSandboxMode(mode));
  };

  const handleTimeoutChange = async (secs: number) => {
    if (secs < 1 || secs > 3600) {
      setError("Timeout must be between 1 and 3600 seconds");
      return;
    }

    saveSettings({ run_command_default_timeout_secs: secs }, () =>
      setTimeoutSecs(secs),
    );
  };

  const handleToggleNamespaces = async () => {
    const current = useNamespaces();
    saveSettings({ run_command_use_namespaces: !current }, () =>
      setUseNamespaces(!current),
    );
  };

  return (
    <div class="admin-settings">
      <h2>Application Settings</h2>

      <Show when={error()}>
        {(err) => <div class="error-msg">{err()}</div>}
      </Show>

      <div class="admin-setting-row">
        <div class="admin-setting-info">
          <h4>User Self-Registration</h4>
          <p>
            When enabled, anyone can create an account on the login page. New
            users will have the regular "user" role with no server access until
            an admin or server owner grants them permissions.
          </p>
        </div>
        <div class="admin-setting-control">
          <button
            class={`btn ${auth.isRegistrationEnabled() ? "btn-danger" : "btn-success"}`}
            onClick={handleToggleRegistration}
            disabled={saving()}
          >
            {saving()
              ? "Saving..."
              : auth.isRegistrationEnabled()
                ? "Disable Registration"
                : "Enable Registration"}
          </button>
          <Show when={success()}>
            <span
              style={{
                color: "var(--success)",
                "font-size": "0.85rem",
                "margin-left": "0.5rem",
              }}
            >
              ✓ Saved
            </span>
          </Show>
        </div>
      </div>

      <div class="admin-setting-row">
        <div class="admin-setting-info">
          <h4>Pipeline Command Execution</h4>
          <p>
            When enabled, templates can execute shell commands via RunCommand
            pipeline steps during installation, updates, and other operations.
          </p>
          <p style={{ color: "var(--warning)", "font-size": "0.85rem" }}>
            ⚠️ <strong>Security Risk:</strong> RunCommand steps execute with
            full privileges of the AnyServer process. Only enable this if you
            trust all templates you import.
          </p>
        </div>
        <div class="admin-setting-control">
          <button
            class={`btn ${auth.isRunCommandsAllowed() ? "btn-danger" : "btn-success"}`}
            onClick={handleToggleRunCommands}
            disabled={saving()}
          >
            {saving()
              ? "Saving..."
              : auth.isRunCommandsAllowed()
                ? "Disable RunCommand"
                : "Enable RunCommand"}
          </button>
          <Show when={success()}>
            <span
              style={{
                color: "var(--success)",
                "font-size": "0.85rem",
                "margin-left": "0.5rem",
              }}
            >
              ✓ Saved
            </span>
          </Show>
        </div>
      </div>

      <div class="admin-setting-row">
        <div class="admin-setting-info">
          <h4>RunCommand Sandbox Mode</h4>
          <p>Controls how RunCommand pipeline steps are isolated:</p>
          <ul
            style={{
              "list-style": "none",
              padding: "0",
              "font-size": "0.85rem",
              color: "var(--text-muted)",
              "margin-top": "0.5rem",
            }}
          >
            <li>
              <strong style={{ color: "var(--text)" }}>auto:</strong> Apply all
              available isolation (Landlock, etc.), gracefully fall back if
              unavailable
            </li>
            <li>
              <strong style={{ color: "var(--text)" }}>off:</strong> No
              sandboxing (not recommended)
            </li>
            <li>
              <strong style={{ color: "var(--text)" }}>strict:</strong> Require
              Landlock or fail
            </li>
          </ul>
        </div>
        <div class="admin-setting-control">
          <select
            value={sandboxMode()}
            onChange={(e) => handleSandboxModeChange(e.currentTarget.value)}
            disabled={saving()}
            style={{
              padding: "0.5rem",
              "border-radius": "0.25rem",
              border: "1px solid var(--border)",
              background: "var(--bg-secondary)",
              color: "var(--text)",
            }}
          >
            <option value="auto">Auto</option>
            <option value="off">Off</option>
            <option value="strict">Strict</option>
          </select>
          <Show when={success()}>
            <span
              style={{
                color: "var(--success)",
                "font-size": "0.85rem",
                "margin-left": "0.5rem",
              }}
            >
              ✓ Saved
            </span>
          </Show>
        </div>
      </div>

      <div class="admin-setting-row">
        <div class="admin-setting-info">
          <h4>RunCommand Default Timeout</h4>
          <p>
            Maximum execution time (in seconds) for RunCommand steps. Commands
            that exceed this timeout will be killed. Range: 1-3600 seconds.
          </p>
        </div>
        <div class="admin-setting-control">
          <input
            type="number"
            min="1"
            max="3600"
            value={timeoutSecs()}
            onInput={(e) => {
              const val = parseInt(e.currentTarget.value);
              if (!isNaN(val)) {
                setTimeoutSecs(val);
              }
            }}
            onBlur={() => handleTimeoutChange(timeoutSecs())}
            disabled={saving()}
            style={{
              padding: "0.5rem",
              "border-radius": "0.25rem",
              border: "1px solid var(--border)",
              background: "var(--bg-secondary)",
              color: "var(--text)",
              width: "100px",
            }}
          />
          <span style={{ "margin-left": "0.5rem" }}>seconds</span>
          <Show when={success()}>
            <span
              style={{
                color: "var(--success)",
                "font-size": "0.85rem",
                "margin-left": "0.5rem",
              }}
            >
              ✓ Saved
            </span>
          </Show>
        </div>
      </div>

      <div class="admin-setting-row">
        <div class="admin-setting-info">
          <h4>Use Namespace Isolation</h4>
          <p>
            When enabled, RunCommand steps run in isolated PID and mount
            namespaces (Linux only). This provides additional process isolation
            on top of Landlock sandboxing.
          </p>
          <p style={{ color: "var(--text-muted)", "font-size": "0.85rem" }}>
            Note: Requires Linux kernel support. Gracefully falls back if
            unavailable.
          </p>
        </div>
        <div class="admin-setting-control">
          <button
            class={`btn ${useNamespaces() ? "btn-success" : "btn-secondary"}`}
            onClick={handleToggleNamespaces}
            disabled={saving()}
          >
            {saving() ? "Saving..." : useNamespaces() ? "Enabled" : "Disabled"}
          </button>
          <Show when={success()}>
            <span
              style={{
                color: "var(--success)",
                "font-size": "0.85rem",
                "margin-left": "0.5rem",
              }}
            >
              ✓ Saved
            </span>
          </Show>
        </div>
      </div>

      <div class="admin-setting-row">
        <div class="admin-setting-info">
          <h4>Current Status</h4>
          <ul
            style={{
              "list-style": "none",
              padding: "0",
              "font-size": "0.9rem",
              color: "var(--text-muted)",
            }}
          >
            <li>
              Setup complete:{" "}
              <strong style={{ color: "var(--text)" }}>
                {auth.isSetupComplete() ? "Yes" : "No"}
              </strong>
            </li>
            <li>
              Registration enabled:{" "}
              <strong style={{ color: "var(--text)" }}>
                {auth.isRegistrationEnabled() ? "Yes" : "No"}
              </strong>
            </li>
            <li>
              RunCommand allowed:{" "}
              <strong style={{ color: "var(--text)" }}>
                {auth.isRunCommandsAllowed() ? "Yes" : "No"}
              </strong>
            </li>
            <li>
              Sandbox mode:{" "}
              <strong style={{ color: "var(--text)" }}>
                {auth.settings()?.run_command_sandbox || "auto"}
              </strong>
            </li>
            <li>
              Default timeout:{" "}
              <strong style={{ color: "var(--text)" }}>
                {auth.settings()?.run_command_default_timeout_secs || 300}s
              </strong>
            </li>
            <li>
              Namespace isolation:{" "}
              <strong style={{ color: "var(--text)" }}>
                {auth.settings()?.run_command_use_namespaces ? "Yes" : "No"}
              </strong>
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
};

export default SettingsTab;
