import {
  type Component,
  createSignal,
  createEffect,
  Show,
  For,
  onMount,
} from "solid-js";
import SearchableSelect from "./SearchableSelect";
import type {
  ConfigParameter,
  FetchedOption,
  UpdateCheckResult,
} from "../types/bindings";
import { fetchOptions } from "../api/templates";
import { fetchCurseForgeFiles } from "../api/curseforge";

export interface UpdateDialogProps {
  /** The version parameter definition (is_version === true), if any. */
  versionParam: ConfigParameter | null;
  /** Current parameter values from the server. */
  parameterValues: Record<string, string>;
  /** Cached update-check result, if available. */
  updateCheckResult: UpdateCheckResult | null;
  /** The currently installed version string. */
  installedVersion: string | null;
  /** Whether an action is already in-flight. */
  busy: boolean;
  /** Called with the chosen version (or null if no version param). */
  onConfirm: (versionOverride: string | null) => void;
  /** Called when the user cancels. */
  onCancel: () => void;
}

const UpdateDialog: Component<UpdateDialogProps> = (props) => {
  const [versions, setVersions] = createSignal<FetchedOption[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [fetchError, setFetchError] = createSignal<string | null>(null);
  const [selectedVersion, setSelectedVersion] = createSignal("");
  const [manualMode, setManualMode] = createSignal(false);

  // Derive useful values
  const currentVersion = () =>
    props.installedVersion ??
    (props.versionParam
      ? (props.parameterValues[props.versionParam.name] ??
        props.versionParam.default ??
        "")
      : "");

  const latestVersion = () =>
    props.updateCheckResult?.update_available
      ? props.updateCheckResult.latest_version
      : null;

  /** Look up a display label from the loaded versions list (returns null if not found). */
  const labelFromVersions = (
    value: string | null | undefined,
  ): string | null => {
    if (!value) return null;
    const opt = versions().find((o) => o.value === value);
    return opt && opt.label !== opt.value ? opt.label : null;
  };

  // Human-readable display names (e.g. CurseForge file names instead of IDs).
  // Priority: update-check display name → loaded versions list → raw value.
  const currentVersionDisplay = () =>
    props.updateCheckResult?.installed_version_display ??
    labelFromVersions(currentVersion()) ??
    currentVersion();

  const latestVersionDisplay = () =>
    props.updateCheckResult?.latest_version_display ??
    labelFromVersions(latestVersion()) ??
    latestVersion();

  /** Resolve a version value to its display label. */
  const displayForVersion = (value: string) => {
    const fromList = labelFromVersions(value);
    if (fromList) return fromList;
    // Fall back to known display names from the update check result
    if (
      value === currentVersion() &&
      props.updateCheckResult?.installed_version_display
    )
      return props.updateCheckResult.installed_version_display;
    if (
      value === latestVersion() &&
      props.updateCheckResult?.latest_version_display
    )
      return props.updateCheckResult.latest_version_display;
    return value;
  };

  /** Whether we have a way to fetch version options (options_from, CurseForge, etc.). */
  const canLoadVersions = () => {
    if (props.versionParam?.options_from) return true;
    if (
      props.versionParam?.param_type === "curse_forge_file_version" &&
      props.versionParam.curseforge_project_id
    )
      return true;
    return false;
  };

  // On mount, set initial selected version and fetch options
  onMount(() => {
    // Default to latest from update check, or current version
    const initial = latestVersion() ?? currentVersion();
    setSelectedVersion(initial);

    if (canLoadVersions()) {
      loadVersions();
    } else {
      // No known way to fetch versions → manual input mode
      setManualMode(true);
    }
  });

  const loadVersions = async () => {
    setLoading(true);
    setFetchError(null);
    try {
      let options: FetchedOption[];

      // CurseForge file version parameters use their own API
      if (
        props.versionParam?.param_type === "curse_forge_file_version" &&
        props.versionParam.curseforge_project_id
      ) {
        const resp = await fetchCurseForgeFiles(
          props.versionParam.curseforge_project_id,
        );
        options = resp.options.map((o) => ({
          value: o.value,
          label: o.label,
        }));
      } else {
        // Generic options_from fetch
        const of = props.versionParam?.options_from;
        if (!of) return;

        // Build substitution params from other parameter values
        const subs: Record<string, string> = {};
        for (const [k, v] of Object.entries(props.parameterValues)) {
          if (k !== props.versionParam!.name) {
            subs[k] = v;
          }
        }

        const resp = await fetchOptions({
          url: of.url,
          path: of.path,
          value_key: of.value_key,
          label_key: of.label_key,
          sort: of.sort,
          limit: of.limit,
          params: subs,
        });
        options = resp.options;
      }

      setVersions(options);

      // Successful fetch → switch back to dropdown mode so the
      // <select> renders instead of the manual text input.
      if (options.length > 0) {
        setManualMode(false);
      }

      // If the selected version isn't in the fetched list, keep it but note it
      const sel = selectedVersion();
      const inList = options.some((o) => o.value === sel);
      if (!inList && sel && options.length > 0) {
        // If we had a latest from update check that's not in the list,
        // fall back to first option
        if (!latestVersion() || latestVersion() !== sel) {
          setSelectedVersion(options[0].value);
        }
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : "Failed to load versions";
      setFetchError(msg);
      setManualMode(true);
    } finally {
      setLoading(false);
    }
  };

  const handleConfirm = () => {
    if (!props.versionParam) {
      props.onConfirm(null);
      return;
    }
    const version = selectedVersion().trim();
    if (!version) return;
    props.onConfirm(version);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      props.onCancel();
    } else if (e.key === "Enter" && !props.busy) {
      handleConfirm();
    }
  };

  // Close on backdrop click
  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onCancel();
    }
  };

  const isUpgrade = () => {
    const sel = selectedVersion();
    const cur = currentVersion();
    return sel && cur && sel !== cur;
  };

  const isCurrentSelected = () => {
    return selectedVersion() === currentVersion();
  };

  return (
    <div
      class="update-dialog-overlay"
      data-testid="update-dialog"
      style={{
        position: "fixed",
        top: "0",
        left: "0",
        right: "0",
        bottom: "0",
        "background-color": "rgba(0, 0, 0, 0.55)",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        "z-index": "1000",
        "backdrop-filter": "blur(2px)",
      }}
      onClick={handleBackdropClick}
      onKeyDown={handleKeyDown}
    >
      <div
        style={{
          background: "var(--bg-card, #1e1e2e)",
          "border-radius": "14px",
          padding: "0",
          "max-width": "500px",
          width: "92%",
          "box-shadow":
            "0 12px 48px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.06)",
          border: "1px solid var(--border, #333)",
          overflow: "hidden",
          animation: "update-dialog-enter 0.2s ease-out",
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: "1.25rem 1.5rem 1rem",
            "border-bottom": "1px solid rgba(255, 255, 255, 0.06)",
            background:
              "linear-gradient(135deg, rgba(59, 130, 246, 0.08), rgba(139, 92, 246, 0.06))",
          }}
        >
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "0.6rem",
            }}
          >
            <span style={{ "font-size": "1.3rem" }}>🔄</span>
            <h3
              style={{
                margin: "0",
                "font-size": "1.1rem",
                "font-weight": "600",
                color: "var(--text, #e2e8f0)",
              }}
            >
              Update Server
            </h3>
          </div>
        </div>

        {/* Body */}
        <div style={{ padding: "1.25rem 1.5rem" }}>
          {/* Current version display */}
          <Show when={currentVersion()}>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.75rem",
                padding: "0.65rem 0.9rem",
                background: "rgba(100, 116, 139, 0.08)",
                border: "1px solid rgba(100, 116, 139, 0.15)",
                "border-radius": "8px",
                "margin-bottom": "1rem",
                "font-size": "0.85rem",
              }}
            >
              <span
                style={{
                  color: "#94a3b8",
                  "font-size": "0.8rem",
                  "white-space": "nowrap",
                }}
              >
                Current version
              </span>
              <span
                style={{
                  color: "#e2e8f0",
                  "font-weight": "600",
                  "font-family": "'SF Mono', 'Cascadia Code', monospace",
                  "font-size": "0.9rem",
                  background: "rgba(100, 116, 139, 0.15)",
                  padding: "2px 8px",
                  "border-radius": "4px",
                }}
              >
                {currentVersionDisplay()}
              </span>
            </div>
          </Show>

          {/* Latest version nudge */}
          <Show when={latestVersion() && latestVersion() !== currentVersion()}>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.6rem",
                padding: "0.55rem 0.9rem",
                background: "rgba(234, 179, 8, 0.08)",
                border: "1px solid rgba(234, 179, 8, 0.25)",
                "border-radius": "8px",
                "margin-bottom": "1rem",
                "font-size": "0.83rem",
                color: "#eab308",
              }}
            >
              <span>⬆</span>
              <span>
                Latest available:{" "}
                <strong
                  style={{
                    "font-family": "'SF Mono', 'Cascadia Code', monospace",
                  }}
                >
                  {latestVersionDisplay()}
                </strong>
              </span>
            </div>
          </Show>

          {/* Version selector */}
          <Show
            when={props.versionParam}
            fallback={
              <p
                style={{
                  color: "#9ca3af",
                  "font-size": "0.88rem",
                  "line-height": "1.55",
                  margin: "0.5rem 0 0.25rem",
                }}
              >
                This server does not have a version parameter. The update
                pipeline will re-run with the current configuration.
              </p>
            }
          >
            <div class="form-group" style={{ "margin-bottom": "0" }}>
              <label
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "0.5rem",
                  "margin-bottom": "0.4rem",
                  "font-size": "0.85rem",
                  color: "#cbd5e1",
                }}
              >
                Update to version
                <Show when={loading()}>
                  <span
                    class="btn-spinner"
                    style={{
                      width: "12px",
                      height: "12px",
                      "border-width": "2px",
                    }}
                  />
                </Show>
              </label>

              <Show
                when={!manualMode() && versions().length > 0}
                fallback={
                  <div>
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "0.5rem",
                      }}
                    >
                      <input
                        type="text"
                        value={selectedVersion()}
                        onInput={(e) =>
                          setSelectedVersion(e.currentTarget.value)
                        }
                        placeholder={
                          props.versionParam?.default ?? "Enter version"
                        }
                        style={{
                          flex: "1",
                          "font-family":
                            "'SF Mono', 'Cascadia Code', monospace",
                          "font-size": "0.9rem",
                        }}
                        autofocus
                      />
                      <Show when={canLoadVersions()}>
                        <button
                          class="btn btn-sm"
                          onClick={loadVersions}
                          disabled={loading()}
                          style={{
                            "white-space": "nowrap",
                            "min-width": "fit-content",
                          }}
                        >
                          {loading() ? "Loading…" : "Load versions"}
                        </button>
                      </Show>
                    </div>
                    <Show when={fetchError()}>
                      <small
                        style={{
                          color: "#ef4444",
                          "font-size": "0.78rem",
                          "margin-top": "0.3rem",
                          display: "block",
                        }}
                      >
                        ⚠ {fetchError()}
                      </small>
                    </Show>
                  </div>
                }
              >
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "0.5rem",
                  }}
                >
                  <SearchableSelect
                    options={versions().map((opt) => {
                      const isCurrent = opt.value === currentVersion();
                      const isLatest = opt.value === latestVersion();
                      const parts: string[] = [];
                      if (isCurrent) parts.push("current");
                      if (isLatest) parts.push("latest");
                      const suffix =
                        parts.length > 0 ? ` (${parts.join(", ")})` : "";
                      const base =
                        opt.label !== opt.value
                          ? `${opt.label} (${opt.value})`
                          : opt.value;
                      return { value: opt.value, label: `${base}${suffix}` };
                    })}
                    value={selectedVersion()}
                    onChange={(v) => setSelectedVersion(v)}
                    placeholder="Select version…"
                  />

                  <button
                    class="btn btn-sm"
                    onClick={loadVersions}
                    disabled={loading()}
                    title="Refresh version list"
                    style={{
                      "white-space": "nowrap",
                      "min-width": "fit-content",
                      padding: "4px 8px",
                    }}
                  >
                    {loading() ? "…" : "↻"}
                  </button>

                  <button
                    class="btn btn-sm"
                    onClick={() => setManualMode(true)}
                    title="Enter version manually"
                    style={{
                      "white-space": "nowrap",
                      "min-width": "fit-content",
                      padding: "4px 8px",
                      "font-size": "0.75rem",
                    }}
                  >
                    ✏️
                  </button>
                </div>
              </Show>

              {/* Version change summary */}
              <Show when={props.versionParam && selectedVersion().trim()}>
                <div
                  style={{
                    "margin-top": "0.65rem",
                    "font-size": "0.82rem",
                    color: "#94a3b8",
                    display: "flex",
                    "align-items": "center",
                    gap: "0.4rem",
                  }}
                >
                  <Show when={isUpgrade()}>
                    <span style={{ color: "#22c55e" }}>⬆</span>
                    <span>
                      <span
                        style={{
                          "font-family":
                            "'SF Mono', 'Cascadia Code', monospace",
                          color: "#9ca3af",
                          "text-decoration": "line-through",
                          "font-size": "0.8rem",
                        }}
                      >
                        {currentVersionDisplay()}
                      </span>
                      <span style={{ margin: "0 0.35rem" }}>→</span>
                      <span
                        style={{
                          "font-family":
                            "'SF Mono', 'Cascadia Code', monospace",
                          color: "#22c55e",
                          "font-weight": "600",
                        }}
                      >
                        {displayForVersion(selectedVersion())}
                      </span>
                    </span>
                  </Show>
                  <Show when={isCurrentSelected()}>
                    <span style={{ color: "#64748b" }}>
                      ℹ Same version — will re-run the update pipeline
                    </span>
                  </Show>
                </div>
              </Show>
            </div>
          </Show>
        </div>

        {/* Footer */}
        <div
          style={{
            padding: "1rem 1.5rem 1.25rem",
            "border-top": "1px solid rgba(255, 255, 255, 0.06)",
            display: "flex",
            "justify-content": "flex-end",
            gap: "0.6rem",
          }}
        >
          <button
            class="btn"
            onClick={props.onCancel}
            disabled={props.busy}
            style={{
              "min-width": "80px",
            }}
          >
            Cancel
          </button>
          <button
            class="btn btn-primary"
            data-testid="update-dialog-confirm"
            onClick={handleConfirm}
            disabled={
              props.busy ||
              loading() ||
              (!!props.versionParam && !selectedVersion().trim())
            }
            style={{
              "min-width": "120px",
              background: isUpgrade()
                ? "linear-gradient(135deg, #3b82f6, #8b5cf6)"
                : undefined,
              "border-color": isUpgrade() ? "transparent" : undefined,
            }}
          >
            <Show when={props.busy} fallback={<>🔄 Update</>}>
              <span class="btn-spinner" /> Updating…
            </Show>
          </button>
        </div>
      </div>

      {/* Inline animation keyframes */}
      <style>{`
        @keyframes update-dialog-enter {
          from {
            opacity: 0;
            transform: scale(0.95) translateY(8px);
          }
          to {
            opacity: 1;
            transform: scale(1) translateY(0);
          }
        }
      `}</style>
    </div>
  );
};

export default UpdateDialog;
