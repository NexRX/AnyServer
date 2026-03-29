import {
  type Component,
  createSignal,
  createResource,
  Show,
} from "solid-js";
import Loader from "../Loader";
import { getSandboxCapabilities, toggleSandboxFeature } from "../../api/client";

const SandboxFeatureTab: Component = () => {
  const [capabilities, { refetch }] = createResource(getSandboxCapabilities);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);
  const [toggling, setToggling] = createSignal(false);

  const handleToggle = async () => {
    const caps = capabilities();
    if (!caps) return;
    const newValue = !caps.feature_enabled;
    const action = newValue ? "enable" : "disable";
    if (
      !confirm(
        `Are you sure you want to ${action} sandbox management site-wide?\n\n` +
          (newValue
            ? "This will allow admins to configure per-server security sandbox settings."
            : "This will hide per-server sandbox controls. Existing profiles are preserved."),
      )
    )
      return;

    setToggling(true);
    setError(null);
    setSuccess(null);
    try {
      await toggleSandboxFeature({ enabled: newValue });
      setSuccess(`Sandbox management ${newValue ? "enabled" : "disabled"}`);
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to toggle");
    } finally {
      setToggling(false);
    }
  };

  const capItem = (label: string, available: boolean, tooltip: string) => (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        gap: "0.75rem",
        padding: "0.6rem 0.75rem",
        background: "var(--bg-card)",
        "border-radius": "var(--radius-sm)",
        border: `1px solid ${available ? "var(--success)" : "var(--border)"}`,
      }}
      title={tooltip}
    >
      <span style={{ "font-size": "1.1rem" }}>{available ? "✅" : "❌"}</span>
      <div>
        <div style={{ "font-weight": "600", "font-size": "0.85rem" }}>
          {label}
        </div>
        <div
          style={{
            "font-size": "0.75rem",
            color: "var(--text-dim)",
            "max-width": "400px",
          }}
        >
          {tooltip}
        </div>
      </div>
    </div>
  );

  return (
    <div>
      <h2 style={{ "margin-bottom": "0.5rem" }}>Sandbox Management</h2>
      <p
        style={{
          color: "var(--text-muted)",
          "font-size": "0.85rem",
          "margin-bottom": "1.5rem",
        }}
      >
        Control the site-wide feature flag for per-server security sandbox
        configuration. When enabled, admins can fine-tune isolation settings on
        each server's detail page.
      </p>

      <Show when={error()}>
        {(err) => <div class="error-msg">{err()}</div>}
      </Show>
      <Show when={success()}>
        {(msg) => (
          <div
            style={{
              background: "var(--success-bg)",
              border: "1px solid var(--success)",
              "border-radius": "var(--radius-sm)",
              padding: "0.75rem",
              color: "var(--success)",
              "margin-bottom": "1rem",
              "font-size": "0.9rem",
            }}
          >
            {msg()}
          </div>
        )}
      </Show>

      <Show when={capabilities.loading}>
        <Loader message="Loading sandbox capabilities" />
      </Show>

      <Show when={capabilities()}>
        {(caps) => (
          <>
            {/* Feature flag toggle */}
            <div
              style={{
                background: caps().feature_enabled
                  ? "var(--success-bg)"
                  : "var(--bg-elevated)",
                border: `1px solid ${caps().feature_enabled ? "var(--success)" : "var(--border)"}`,
                "border-radius": "var(--radius)",
                padding: "1.25rem",
                "margin-bottom": "1.5rem",
                display: "flex",
                "justify-content": "space-between",
                "align-items": "center",
              }}
            >
              <div>
                <div style={{ "font-weight": "600", "font-size": "1rem" }}>
                  {caps().feature_enabled ? "🛡️ Enabled" : "⚠️ Disabled"}
                </div>
                <div
                  style={{
                    "font-size": "0.8rem",
                    color: "var(--text-muted)",
                    "margin-top": "0.25rem",
                  }}
                >
                  {caps().feature_enabled
                    ? "Per-server sandbox controls are visible to admins."
                    : "Per-server sandbox controls are hidden. Default settings apply."}
                </div>
              </div>
              <button
                class={`btn ${caps().feature_enabled ? "btn-secondary" : "btn-primary"}`}
                onClick={handleToggle}
                disabled={toggling()}
              >
                {toggling()
                  ? "..."
                  : caps().feature_enabled
                    ? "Disable"
                    : "Enable"}
              </button>
            </div>

            {/* Host capabilities */}
            <h3
              style={{
                "margin-bottom": "0.75rem",
                color: "var(--text)",
                "text-transform": "none",
                "font-size": "1rem",
              }}
            >
              Host Capabilities
            </h3>
            <div
              style={{
                display: "grid",
                "grid-template-columns":
                  "repeat(auto-fill, minmax(320px, 1fr))",
                gap: "0.5rem",
                "margin-bottom": "1rem",
              }}
            >
              {capItem(
                `Landlock${caps().landlock_abi_version ? ` (ABI v${caps().landlock_abi_version})` : ""}`,
                caps().landlock_available,
                "Restricts filesystem access to an allow-list of paths. Requires Linux 5.13+.",
              )}
              {capItem(
                "Namespace Isolation",
                caps().namespaces_available,
                "PID + mount namespaces — hides other processes and privatizes mount points.",
              )}
              {capItem(
                "NO_NEW_PRIVS",
                caps().no_new_privs_available,
                "Prevents privilege escalation through suid/sgid binaries. Always available on Linux.",
              )}
              {capItem(
                "FD Cleanup",
                caps().fd_cleanup_available,
                "Closes inherited file descriptors beyond stdin/stdout/stderr.",
              )}
              {capItem(
                "Non-Dumpable",
                caps().non_dumpable_available,
                "Prevents ptrace attachment and /proc/pid/mem reads.",
              )}
              {capItem(
                "RLIMIT_NPROC",
                caps().rlimit_nproc_available,
                "Fork-bomb protection — caps the number of child processes. Per-UID limit.",
              )}
            </div>
          </>
        )}
      </Show>
    </div>
  );
};

export default SandboxFeatureTab;
