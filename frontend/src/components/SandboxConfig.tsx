import {
  type Component,
  createSignal,
  createResource,
  Show,
  For,
  onMount,
} from "solid-js";
import {
  getSandboxProfile,
  updateSandboxProfile,
  resetSandboxProfile,
} from "../api/client";
import type {
  SandboxProfile,
  SandboxCapabilities,
  UpdateSandboxProfileRequest,
} from "../types/bindings";
import Loader from "./Loader";

interface SandboxConfigProps {
  serverId: string;
}

const TOOLTIP_MAP: Record<string, string> = {
  enabled:
    "Master switch for all process isolation. When disabled, the server runs with no sandboxing at all. Only disable this if you have a specific reason — it significantly reduces security.",
  landlock_enabled:
    "Landlock (Linux 5.13+) restricts filesystem access to an allow-list of paths. The server's data directory is always read-write; system paths like /usr, /lib, /etc are read-only. If the kernel doesn't support Landlock, this toggle is silently ignored.",
  no_new_privs:
    "PR_SET_NO_NEW_PRIVS irreversibly prevents the process and its children from gaining new privileges through suid/sgid binaries or file capabilities. This is also a prerequisite for Landlock to function. Disabling this weakens the entire sandbox.",
  fd_cleanup:
    "Marks all file descriptors beyond stdin/stdout/stderr (fd 0/1/2) as close-on-exec before the child process runs. This prevents the server from accessing AnyServer's database connections, listening sockets, or other internal file handles.",
  non_dumpable:
    "Sets PR_SET_DUMPABLE=0 which prevents other processes on the system from attaching via ptrace or reading /proc/<pid>/mem. Hardens against local information-disclosure attacks where a compromised process tries to inspect the server's memory.",
  namespace_isolation:
    "Runs the server process in its own PID namespace (cannot see or signal other processes) and mount namespace (filesystem mount changes are private). Requires unprivileged user namespaces to be available. Network namespaces are intentionally NOT used — servers typically need network access.",
  pids_max:
    "RLIMIT_NPROC caps the maximum number of child processes (threads + forks) the server may create. Provides fork-bomb protection. CAUTION: This is a per-UID limit, not per-process. Setting it too low can affect AnyServer itself and other processes running as the same OS user. Set to 0 for no limit.",
  network_isolation:
    "Runs the server in its own network namespace, completely isolating it from the host network. Reserved for future use — most game servers and applications need network access, so this defaults to off.",
  seccomp_mode:
    "Seccomp BPF syscall filtering. 'off' = no filter (default). 'basic' = blocks obviously dangerous syscalls. 'strict' = only allows a curated set of syscalls. Reserved for future implementation.",
  extra_read_paths:
    "Additional host paths the server process may read (but not write). Useful for custom JDK, Python, or other runtime installations outside the default system paths (/usr, /lib, /etc, etc.).",
  extra_rw_paths:
    "Additional host paths the server process may read AND write. Use sparingly — every entry widens the blast radius if the server process is compromised.",
};

const SandboxConfig: Component<SandboxConfigProps> = (props) => {
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [resetting, setResetting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);

  const [profile, setProfile] = createSignal<SandboxProfile | null>(null);
  const [capabilities, setCapabilities] =
    createSignal<SandboxCapabilities | null>(null);

  // Local form state (mirrors profile)
  const [enabled, setEnabled] = createSignal(true);
  const [landlockEnabled, setLandlockEnabled] = createSignal(true);
  const [noNewPrivs, setNoNewPrivs] = createSignal(true);
  const [fdCleanup, setFdCleanup] = createSignal(true);
  const [nonDumpable, setNonDumpable] = createSignal(true);
  const [namespaceIsolation, setNamespaceIsolation] = createSignal(true);
  const [pidsMax, setPidsMax] = createSignal(0);
  const [extraReadPaths, setExtraReadPaths] = createSignal<string[]>([]);
  const [extraRwPaths, setExtraRwPaths] = createSignal<string[]>([]);
  const [networkIsolation, setNetworkIsolation] = createSignal(false);
  const [seccompMode, setSeccompMode] = createSignal("off");

  const loadProfile = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await getSandboxProfile(props.serverId);
      setProfile(resp.profile);
      setCapabilities(resp.capabilities);
      syncFormFromProfile(resp.profile);
    } catch (e: unknown) {
      setError(
        e instanceof Error ? e.message : "Failed to load sandbox profile",
      );
    } finally {
      setLoading(false);
    }
  };

  const syncFormFromProfile = (p: SandboxProfile) => {
    setEnabled(p.enabled);
    setLandlockEnabled(p.landlock_enabled);
    setNoNewPrivs(p.no_new_privs);
    setFdCleanup(p.fd_cleanup);
    setNonDumpable(p.non_dumpable);
    setNamespaceIsolation(p.namespace_isolation);
    setPidsMax(Number(p.pids_max));
    setExtraReadPaths([...p.extra_read_paths]);
    setExtraRwPaths([...p.extra_rw_paths]);
    setNetworkIsolation(p.network_isolation);
    setSeccompMode(p.seccomp_mode);
  };

  onMount(() => {
    loadProfile();
  });

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      const req: UpdateSandboxProfileRequest = {
        enabled: enabled(),
        landlock_enabled: landlockEnabled(),
        no_new_privs: noNewPrivs(),
        fd_cleanup: fdCleanup(),
        non_dumpable: nonDumpable(),
        namespace_isolation: namespaceIsolation(),
        pids_max: pidsMax() as unknown as bigint,
        extra_read_paths: extraReadPaths().filter((p) => p.trim().length > 0),
        extra_rw_paths: extraRwPaths().filter((p) => p.trim().length > 0),
        network_isolation: networkIsolation(),
        seccomp_mode: seccompMode(),
      };
      const resp = await updateSandboxProfile(props.serverId, req);
      setProfile(resp.profile);
      setCapabilities(resp.capabilities);
      syncFormFromProfile(resp.profile);
      setSuccess(
        "Sandbox profile saved. Changes take effect on next server start.",
      );
    } catch (e: unknown) {
      setError(
        e instanceof Error ? e.message : "Failed to save sandbox profile",
      );
    } finally {
      setSaving(false);
    }
  };

  const handleReset = async () => {
    if (
      !confirm(
        "Reset the sandbox profile to defaults? All custom settings will be lost.",
      )
    )
      return;

    setResetting(true);
    setError(null);
    setSuccess(null);
    try {
      const resp = await resetSandboxProfile(props.serverId);
      setProfile(resp.profile);
      setCapabilities(resp.capabilities);
      syncFormFromProfile(resp.profile);
      setSuccess("Sandbox profile reset to defaults.");
    } catch (e: unknown) {
      setError(
        e instanceof Error ? e.message : "Failed to reset sandbox profile",
      );
    } finally {
      setResetting(false);
    }
  };

  const addPath = (getter: () => string[], setter: (v: string[]) => void) => {
    setter([...getter(), ""]);
  };

  const removePath = (
    getter: () => string[],
    setter: (v: string[]) => void,
    idx: number,
  ) => {
    setter(getter().filter((_, i) => i !== idx));
  };

  const updatePath = (
    getter: () => string[],
    setter: (v: string[]) => void,
    idx: number,
    val: string,
  ) => {
    const copy = [...getter()];
    copy[idx] = val;
    setter(copy);
  };

  // ─── Tooltip Component ───
  const Tooltip: Component<{ text: string }> = (tp) => (
    <span
      title={tp.text}
      style={{
        display: "inline-flex",
        "align-items": "center",
        "justify-content": "center",
        width: "16px",
        height: "16px",
        "border-radius": "50%",
        background: "var(--bg-elevated)",
        border: "1px solid var(--border)",
        color: "var(--text-dim)",
        "font-size": "0.65rem",
        "font-weight": "700",
        cursor: "help",
        "margin-left": "0.35rem",
        "flex-shrink": "0",
      }}
    >
      ?
    </span>
  );

  // ─── Toggle Switch Row ───
  const ToggleRow: Component<{
    label: string;
    tooltipKey: string;
    checked: boolean;
    onChange: (v: boolean) => void;
    available?: boolean;
    disabled?: boolean;
  }> = (tr) => {
    const isDisabled = () =>
      tr.disabled || !enabled() || tr.available === false;

    return (
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          padding: "0.65rem 0.85rem",
          background: isDisabled() ? "var(--bg)" : "var(--bg-card)",
          "border-radius": "var(--radius-sm)",
          border: `1px solid ${tr.checked && !isDisabled() ? "var(--success)" : "var(--border)"}`,
          opacity: isDisabled() ? "0.55" : "1",
          transition: "all var(--transition)",
        }}
      >
        <div
          style={{
            display: "flex",
            "align-items": "center",
            gap: "0.25rem",
          }}
        >
          <span
            style={{
              "font-weight": "600",
              "font-size": "0.85rem",
              color: isDisabled() ? "var(--text-dim)" : "var(--text)",
            }}
          >
            {tr.label}
          </span>
          <Tooltip text={TOOLTIP_MAP[tr.tooltipKey] ?? ""} />
          <Show when={tr.available === false}>
            <span
              style={{
                "font-size": "0.7rem",
                color: "var(--warning)",
                "margin-left": "0.5rem",
                "font-style": "italic",
              }}
            >
              (not available on this host)
            </span>
          </Show>
        </div>

        <label
          style={{
            position: "relative",
            display: "inline-block",
            width: "42px",
            height: "24px",
            cursor: isDisabled() ? "not-allowed" : "pointer",
            "flex-shrink": "0",
          }}
        >
          <input
            type="checkbox"
            checked={tr.checked}
            disabled={isDisabled()}
            onChange={(e) => tr.onChange(e.currentTarget.checked)}
            style={{
              opacity: "0",
              width: "0",
              height: "0",
              position: "absolute",
            }}
          />
          <span
            style={{
              position: "absolute",
              top: "0",
              left: "0",
              right: "0",
              bottom: "0",
              "border-radius": "24px",
              background:
                tr.checked && !isDisabled()
                  ? "var(--success)"
                  : "var(--bg-input)",
              border: `1px solid ${tr.checked && !isDisabled() ? "var(--success)" : "var(--border)"}`,
              transition: "all var(--transition)",
            }}
          />
          <span
            style={{
              position: "absolute",
              top: "3px",
              left: tr.checked ? "20px" : "3px",
              width: "16px",
              height: "16px",
              "border-radius": "50%",
              background:
                tr.checked && !isDisabled() ? "#fff" : "var(--text-dim)",
              transition: "all var(--transition)",
            }}
          />
        </label>
      </div>
    );
  };

  // ─── Path List Editor ───
  const PathListEditor: Component<{
    label: string;
    tooltipKey: string;
    paths: () => string[];
    setPaths: (v: string[]) => void;
  }> = (pe) => (
    <div
      style={{
        "margin-top": "0.75rem",
        padding: "0.75rem 0.85rem",
        background: "var(--bg-card)",
        "border-radius": "var(--radius-sm)",
        border: "1px solid var(--border)",
        opacity: enabled() ? "1" : "0.55",
      }}
    >
      <div
        style={{
          display: "flex",
          "justify-content": "space-between",
          "align-items": "center",
          "margin-bottom": "0.5rem",
        }}
      >
        <div style={{ display: "flex", "align-items": "center" }}>
          <span
            style={{
              "font-weight": "600",
              "font-size": "0.85rem",
              color: "var(--text)",
            }}
          >
            {pe.label}
          </span>
          <Tooltip text={TOOLTIP_MAP[pe.tooltipKey] ?? ""} />
        </div>
        <button
          type="button"
          class="btn btn-sm btn-secondary"
          onClick={() => addPath(pe.paths, pe.setPaths)}
          disabled={!enabled()}
          style={{ "font-size": "0.7rem", padding: "0.15rem 0.5rem" }}
        >
          + Add Path
        </button>
      </div>

      <For each={pe.paths()}>
        {(path, idx) => (
          <div
            style={{
              display: "flex",
              gap: "0.4rem",
              "align-items": "center",
              "margin-bottom": "0.3rem",
            }}
          >
            <input
              type="text"
              value={path}
              disabled={!enabled()}
              onInput={(e) =>
                updatePath(pe.paths, pe.setPaths, idx(), e.currentTarget.value)
              }
              placeholder="/path/to/directory"
              style={{
                flex: "1",
                padding: "0.35rem 0.5rem",
                background: "var(--bg-input)",
                color: "var(--text)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                "font-family": "var(--mono)",
                "font-size": "0.8rem",
              }}
            />
            <button
              type="button"
              class="btn btn-sm"
              disabled={!enabled()}
              onClick={() => removePath(pe.paths, pe.setPaths, idx())}
              style={{
                background: "var(--danger-bg)",
                color: "var(--danger)",
                border: "1px solid var(--danger)",
                padding: "0.2rem 0.4rem",
                "font-size": "0.7rem",
                cursor: "pointer",
              }}
            >
              ✕
            </button>
          </div>
        )}
      </For>

      <Show when={pe.paths().length === 0}>
        <p
          style={{
            "font-size": "0.75rem",
            color: "var(--text-dim)",
            "font-style": "italic",
            margin: "0",
          }}
        >
          No extra paths configured.
        </p>
      </Show>
    </div>
  );

  // ─── Render ───

  return (
    <div>
      <Show when={loading()}>
        <Loader message="Loading sandbox profile" />
      </Show>

      <Show when={!loading() && capabilities()}>
        {(caps) => (
          <>
            <Show when={!caps().feature_enabled}>
              <div
                style={{
                  background: "var(--warning-bg)",
                  border: "1px solid var(--warning)",
                  "border-radius": "var(--radius)",
                  padding: "1rem 1.25rem",
                  color: "var(--warning)",
                  "margin-bottom": "1rem",
                }}
              >
                <strong>⚠️ Sandbox management is disabled site-wide.</strong>
                <div
                  style={{
                    "font-size": "0.85rem",
                    "margin-top": "0.25rem",
                    color: "var(--text-muted)",
                  }}
                >
                  The site owner must enable this feature in Admin → Sandbox
                  before per-server settings can be changed.
                </div>
              </div>
            </Show>

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
                    "font-size": "0.85rem",
                  }}
                >
                  {msg()}
                </div>
              )}
            </Show>

            <div
              style={{
                display: "flex",
                "justify-content": "space-between",
                "align-items": "center",
                "margin-bottom": "1rem",
              }}
            >
              <div>
                <h3
                  style={{
                    "margin-bottom": "0.15rem",
                    color: "var(--text)",
                    "text-transform": "none",
                    "font-size": "1rem",
                    "letter-spacing": "0",
                  }}
                >
                  🛡️ Security Sandbox
                </h3>
                <p
                  style={{
                    "font-size": "0.8rem",
                    color: "var(--text-dim)",
                    margin: "0",
                  }}
                >
                  Changes take effect on next server start.
                </p>
              </div>
              <div style={{ display: "flex", gap: "0.5rem" }}>
                <button
                  class="btn btn-sm btn-secondary"
                  onClick={handleReset}
                  disabled={resetting() || !caps().feature_enabled}
                  style={{ "font-size": "0.8rem" }}
                >
                  {resetting() ? "..." : "Reset Defaults"}
                </button>
                <button
                  class="btn btn-sm btn-primary"
                  onClick={handleSave}
                  disabled={saving() || !caps().feature_enabled}
                  style={{ "font-size": "0.8rem" }}
                >
                  {saving() ? "Saving..." : "Save Changes"}
                </button>
              </div>
            </div>

            {/* Master switch */}
            <div style={{ "margin-bottom": "0.75rem" }}>
              <ToggleRow
                label="Isolation Enabled"
                tooltipKey="enabled"
                checked={enabled()}
                onChange={setEnabled}
                disabled={!caps().feature_enabled}
              />
            </div>

            {/* Individual toggles */}
            <div
              style={{
                display: "grid",
                "grid-template-columns":
                  "repeat(auto-fill, minmax(320px, 1fr))",
                gap: "0.5rem",
                "margin-bottom": "0.75rem",
              }}
            >
              <ToggleRow
                label="Landlock Filesystem"
                tooltipKey="landlock_enabled"
                checked={landlockEnabled()}
                onChange={setLandlockEnabled}
                available={caps().landlock_available}
                disabled={!caps().feature_enabled}
              />
              <ToggleRow
                label="NO_NEW_PRIVS"
                tooltipKey="no_new_privs"
                checked={noNewPrivs()}
                onChange={setNoNewPrivs}
                available={caps().no_new_privs_available}
                disabled={!caps().feature_enabled}
              />
              <ToggleRow
                label="FD Cleanup"
                tooltipKey="fd_cleanup"
                checked={fdCleanup()}
                onChange={setFdCleanup}
                available={caps().fd_cleanup_available}
                disabled={!caps().feature_enabled}
              />
              <ToggleRow
                label="Non-Dumpable"
                tooltipKey="non_dumpable"
                checked={nonDumpable()}
                onChange={setNonDumpable}
                available={caps().non_dumpable_available}
                disabled={!caps().feature_enabled}
              />
              <ToggleRow
                label="PID + Mount Namespaces"
                tooltipKey="namespace_isolation"
                checked={namespaceIsolation()}
                onChange={setNamespaceIsolation}
                available={caps().namespaces_available}
                disabled={!caps().feature_enabled}
              />
              <ToggleRow
                label="Network Isolation"
                tooltipKey="network_isolation"
                checked={networkIsolation()}
                onChange={setNetworkIsolation}
                disabled={!caps().feature_enabled}
              />
            </div>

            {/* Numeric: pids_max */}
            <div
              style={{
                padding: "0.65rem 0.85rem",
                background: enabled() ? "var(--bg-card)" : "var(--bg)",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                "margin-bottom": "0.75rem",
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
                opacity: enabled() ? "1" : "0.55",
              }}
            >
              <div style={{ display: "flex", "align-items": "center" }}>
                <span
                  style={{
                    "font-weight": "600",
                    "font-size": "0.85rem",
                    color: enabled() ? "var(--text)" : "var(--text-dim)",
                  }}
                >
                  Max Child Processes (RLIMIT_NPROC)
                </span>
                <Tooltip text={TOOLTIP_MAP["pids_max"]} />
              </div>
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "0.5rem",
                }}
              >
                <input
                  type="number"
                  min="0"
                  value={pidsMax()}
                  disabled={!enabled() || !caps().feature_enabled}
                  onInput={(e) =>
                    setPidsMax(
                      Math.max(0, parseInt(e.currentTarget.value) || 0),
                    )
                  }
                  style={{
                    width: "100px",
                    padding: "0.3rem 0.5rem",
                    background: "var(--bg-input)",
                    color: "var(--text)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    "font-family": "var(--mono)",
                    "font-size": "0.85rem",
                    "text-align": "right",
                  }}
                />
                <span
                  style={{
                    "font-size": "0.75rem",
                    color: "var(--text-dim)",
                  }}
                >
                  {pidsMax() === 0 ? "(no limit)" : ""}
                </span>
              </div>
            </div>

            {/* Seccomp mode */}
            <div
              style={{
                padding: "0.65rem 0.85rem",
                background: enabled() ? "var(--bg-card)" : "var(--bg)",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                "margin-bottom": "0.75rem",
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
                opacity: enabled() ? "1" : "0.55",
              }}
            >
              <div style={{ display: "flex", "align-items": "center" }}>
                <span
                  style={{
                    "font-weight": "600",
                    "font-size": "0.85rem",
                    color: enabled() ? "var(--text)" : "var(--text-dim)",
                  }}
                >
                  Seccomp BPF Mode
                </span>
                <Tooltip text={TOOLTIP_MAP["seccomp_mode"]} />
              </div>
              <select
                value={seccompMode()}
                disabled={!enabled() || !caps().feature_enabled}
                onChange={(e) => setSeccompMode(e.currentTarget.value)}
                style={{
                  padding: "0.3rem 0.5rem",
                  background: "var(--bg-input)",
                  color: "var(--text)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  "font-size": "0.85rem",
                }}
              >
                <option value="off">Off</option>
                <option value="basic">Basic</option>
                <option value="strict">Strict</option>
              </select>
            </div>

            {/* Extra paths */}
            <PathListEditor
              label="Extra Read-Only Paths"
              tooltipKey="extra_read_paths"
              paths={extraReadPaths}
              setPaths={setExtraReadPaths}
            />
            <PathListEditor
              label="Extra Read-Write Paths"
              tooltipKey="extra_rw_paths"
              paths={extraRwPaths}
              setPaths={setExtraRwPaths}
            />

            {/* Bottom save bar */}
            <div
              style={{
                display: "flex",
                "justify-content": "flex-end",
                gap: "0.5rem",
                "margin-top": "1.25rem",
                "padding-top": "1rem",
                "border-top": "1px solid var(--border)",
              }}
            >
              <button
                class="btn btn-secondary"
                onClick={handleReset}
                disabled={resetting() || !caps().feature_enabled}
              >
                {resetting() ? "Resetting..." : "Reset to Defaults"}
              </button>
              <button
                class="btn btn-primary"
                onClick={handleSave}
                disabled={saving() || !caps().feature_enabled}
              >
                {saving() ? "Saving..." : "💾 Save Sandbox Profile"}
              </button>
            </div>
          </>
        )}
      </Show>
    </div>
  );
};

export default SandboxConfig;
