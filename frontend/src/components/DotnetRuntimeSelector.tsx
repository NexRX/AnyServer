import { type Component, createSignal, Show, For } from "solid-js";
import { getDotnetRuntimes, getDotnetEnv } from "../api/client";
import type { DotnetRuntime } from "../types/bindings";

export interface DotnetRuntimeSelectorProps {
  currentBinary: string;
  currentEnv: Record<string, string>;
  onSelect: (envVars: Record<string, string>) => void;
  serverDir?: string;
}

const DotnetRuntimeSelector: Component<DotnetRuntimeSelectorProps> = (
  props,
) => {
  const [runtimes, setRuntimes] = createSignal<DotnetRuntime[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [detected, setDetected] = createSignal(false);
  const [expanded, setExpanded] = createSignal(false);
  const [applyingRuntime, setApplyingRuntime] = createSignal<string | null>(
    null,
  );

  const detect = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await getDotnetRuntimes();
      setRuntimes(resp.runtimes);
      setDetected(true);
      if (resp.runtimes.length > 0) {
        setExpanded(true);
      }
    } catch (e: any) {
      setError(e.message || "Failed to detect .NET runtimes");
    } finally {
      setLoading(false);
    }
  };

  const applyRuntime = async (rt: DotnetRuntime) => {
    setApplyingRuntime(rt.installation_root);
    setError(null);
    try {
      const envVars = await getDotnetEnv(rt.installation_root, props.serverDir);
      props.onSelect(envVars);
    } catch (e: any) {
      setError(
        e.message || "Failed to generate environment variables for runtime",
      );
    } finally {
      setApplyingRuntime(null);
    }
  };

  const formatRuntime = (rt: DotnetRuntime): string => {
    const parts: string[] = [];
    parts.push(`.NET ${rt.major_version}`);
    parts.push(`(${rt.version})`);
    if (rt.runtime_name) {
      const short = rt.runtime_name
        .replace("Microsoft.", "")
        .replace(".App", "");
      parts.push(`— ${short}`);
    }
    if (rt.is_default) parts.push("★ default");
    return parts.join(" ");
  };

  const isCurrentRuntime = (rt: DotnetRuntime): boolean => {
    const dotnetRoot = props.currentEnv["DOTNET_ROOT"];
    if (!dotnetRoot) return false;
    // Check if the DOTNET_ROOT matches this runtime's installation
    return (
      dotnetRoot === rt.installation_root ||
      dotnetRoot.startsWith(rt.installation_root)
    );
  };

  const hasAnyDotnetEnv = (): boolean => {
    return !!(
      props.currentEnv["DOTNET_ROOT"] ||
      props.currentEnv["DOTNET_BUNDLE_EXTRACT_BASE_DIR"]
    );
  };

  const clearDotnetEnv = () => {
    props.onSelect({
      DOTNET_ROOT: "",
      DOTNET_BUNDLE_EXTRACT_BASE_DIR: "",
    });
  };

  // Group runtimes by installation
  const groupedRuntimes = () => {
    const groups = new Map<string, DotnetRuntime[]>();
    runtimes().forEach((rt) => {
      const key = rt.installation_root;
      if (!groups.has(key)) {
        groups.set(key, []);
      }
      groups.get(key)!.push(rt);
    });
    return Array.from(groups.entries());
  };

  return (
    <div
      style={{
        "margin-top": "0.5rem",
        "margin-bottom": "0.25rem",
      }}
    >
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "0.5rem",
        }}
      >
        <button
          type="button"
          class="btn btn-sm"
          onClick={() => {
            if (!detected()) {
              detect();
            } else {
              setExpanded(!expanded());
            }
          }}
          disabled={loading()}
          style={{
            "font-size": "0.78rem",
            padding: "0.25rem 0.6rem",
          }}
        >
          <Show
            when={loading()}
            fallback={
              <>
                🔵{" "}
                {!detected()
                  ? "Detect .NET Runtimes"
                  : expanded()
                    ? "Hide Runtimes"
                    : "Show Runtimes"}
              </>
            }
          >
            <span class="btn-spinner" /> Scanning...
          </Show>
        </button>

        <Show when={detected() && !loading()}>
          <button
            type="button"
            class="btn btn-sm"
            onClick={detect}
            style={{
              "font-size": "0.78rem",
              padding: "0.25rem 0.5rem",
            }}
            title="Re-scan for .NET installations"
          >
            🔄 Re-scan
          </button>
        </Show>

        <Show when={hasAnyDotnetEnv()}>
          <button
            type="button"
            class="btn btn-sm"
            onClick={clearDotnetEnv}
            style={{
              "font-size": "0.78rem",
              padding: "0.25rem 0.5rem",
              background: "var(--danger-bg)",
              color: "var(--danger)",
              border: "1px solid var(--danger)",
            }}
            title="Clear .NET environment variables"
          >
            ✕ Clear
          </button>
        </Show>

        <Show when={detected() && runtimes().length > 0}>
          <span
            style={{
              "font-size": "0.75rem",
              color: "var(--text-dim)",
            }}
          >
            {runtimes().length} runtime{runtimes().length !== 1 ? "s" : ""}{" "}
            found
          </span>
        </Show>
      </div>

      <Show when={error()}>
        <div
          style={{
            "margin-top": "0.4rem",
            padding: "0.4rem 0.6rem",
            background: "var(--danger-bg)",
            border: "1px solid var(--danger)",
            "border-radius": "var(--radius-sm)",
            "font-size": "0.8rem",
            color: "var(--danger)",
          }}
        >
          {error()}
        </div>
      </Show>

      <Show when={expanded() && detected()}>
        <div
          style={{
            "margin-top": "0.5rem",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-sm)",
            overflow: "hidden",
          }}
        >
          <Show
            when={runtimes().length > 0}
            fallback={
              <div
                style={{
                  padding: "0.75rem",
                  "text-align": "center",
                  color: "var(--warning)",
                  "font-size": "0.82rem",
                  background: "var(--warning-bg)",
                }}
              >
                ⚠ No .NET installations detected on the system.
                <br />
                <span
                  style={{ "font-size": "0.75rem", color: "var(--text-dim)" }}
                >
                  Install .NET runtime (e.g.{" "}
                  <code style={{ "font-size": "0.72rem" }}>
                    apt install dotnet-runtime-6.0
                  </code>
                  ) and re-scan.
                </span>
              </div>
            }
          >
            <For each={groupedRuntimes()}>
              {([installationRoot, rts]) => {
                const isActive = () => rts.some((rt) => isCurrentRuntime(rt));
                const representativeRuntime = rts[0];
                const isApplying = () => applyingRuntime() === installationRoot;

                return (
                  <button
                    type="button"
                    onClick={() => applyRuntime(representativeRuntime)}
                    disabled={isApplying()}
                    style={{
                      display: "flex",
                      "align-items": "flex-start",
                      gap: "0.5rem",
                      width: "100%",
                      padding: "0.6rem 0.75rem",
                      border: "none",
                      "border-bottom": "1px solid var(--border)",
                      background: isActive()
                        ? "var(--primary-bg)"
                        : "var(--bg-input)",
                      color: "var(--text)",
                      cursor: isApplying() ? "wait" : "pointer",
                      "font-size": "0.82rem",
                      "text-align": "left",
                      transition: "background var(--transition)",
                      opacity: isApplying() ? "0.7" : "1",
                    }}
                    onMouseEnter={(e) => {
                      if (!isActive() && !isApplying())
                        e.currentTarget.style.background = "var(--bg-hover)";
                    }}
                    onMouseLeave={(e) => {
                      if (!isActive() && !isApplying())
                        e.currentTarget.style.background = "var(--bg-input)";
                    }}
                  >
                    <span
                      style={{
                        "flex-shrink": "0",
                        width: "1.2em",
                        "text-align": "center",
                        color: isActive()
                          ? "var(--primary)"
                          : "var(--text-dim)",
                        "margin-top": "0.15rem",
                      }}
                    >
                      {isActive() ? "●" : "○"}
                    </span>
                    <div style={{ flex: "1", "min-width": "0" }}>
                      <div
                        style={{
                          "font-weight": "500",
                          "margin-bottom": "0.3rem",
                        }}
                      >
                        <For each={rts}>
                          {(rt, index) => (
                            <>
                              <Show when={index() > 0}>
                                <span
                                  style={{
                                    margin: "0 0.3rem",
                                    color: "var(--text-dim)",
                                  }}
                                >
                                  +
                                </span>
                              </Show>
                              <span
                                style={{
                                  display: "inline-flex",
                                  "align-items": "center",
                                  gap: "0.3rem",
                                  "flex-wrap": "wrap",
                                }}
                              >
                                <span>.NET {rt.major_version}</span>
                                <span
                                  style={{
                                    "font-size": "0.7rem",
                                    color: "var(--text-muted)",
                                    "font-weight": "400",
                                  }}
                                >
                                  ({rt.version})
                                </span>
                                <Show
                                  when={
                                    rt.runtime_name !== "Microsoft.NETCore.App"
                                  }
                                >
                                  <span
                                    style={{
                                      "font-size": "0.65rem",
                                      padding: "0.1rem 0.35rem",
                                      "border-radius": "3px",
                                      background: "var(--info-bg)",
                                      color: "var(--info)",
                                      "font-weight": "600",
                                    }}
                                  >
                                    {rt.runtime_name
                                      .replace("Microsoft.", "")
                                      .replace(".App", "")}
                                  </span>
                                </Show>
                                <Show when={rt.is_default}>
                                  <span
                                    style={{
                                      "font-size": "0.65rem",
                                      padding: "0.1rem 0.35rem",
                                      "border-radius": "3px",
                                      background: "var(--success-bg)",
                                      color: "var(--success)",
                                      "font-weight": "600",
                                      "letter-spacing": "0.02em",
                                    }}
                                  >
                                    DEFAULT
                                  </span>
                                </Show>
                              </span>
                            </>
                          )}
                        </For>
                      </div>
                      <div
                        style={{
                          "font-size": "0.7rem",
                          color: "var(--text-dim)",
                          "margin-top": "0.2rem",
                        }}
                      >
                        <code
                          style={{
                            "font-size": "0.68rem",
                            "font-family": "var(--mono)",
                          }}
                        >
                          {installationRoot}
                        </code>
                      </div>
                    </div>
                    <Show when={isActive()}>
                      <span
                        style={{
                          "font-size": "0.7rem",
                          color: "var(--primary)",
                          "font-weight": "600",
                          "flex-shrink": "0",
                          "margin-top": "0.15rem",
                        }}
                      >
                        ACTIVE
                      </span>
                    </Show>
                    <Show when={isApplying()}>
                      <span
                        class="btn-spinner"
                        style={{ "flex-shrink": "0" }}
                      />
                    </Show>
                  </button>
                );
              }}
            </For>
          </Show>
        </div>

        <div
          style={{
            "margin-top": "0.35rem",
            "font-size": "0.72rem",
            color: "var(--text-dim)",
            "line-height": "1.4",
          }}
        >
          Selecting a runtime automatically sets{" "}
          <code style={{ "font-size": "0.68rem" }}>DOTNET_ROOT</code> and{" "}
          <code style={{ "font-size": "0.68rem" }}>
            DOTNET_BUNDLE_EXTRACT_BASE_DIR
          </code>{" "}
          environment variables for your server.
        </div>

        <Show when={hasAnyDotnetEnv()}>
          <div
            style={{
              "margin-top": "0.5rem",
              padding: "0.5rem",
              background: "var(--info-bg)",
              border: "1px solid var(--info)",
              "border-radius": "var(--radius-sm)",
              "font-size": "0.75rem",
              color: "var(--text)",
            }}
          >
            <div
              style={{
                "font-weight": "600",
                "margin-bottom": "0.25rem",
                color: "var(--info)",
              }}
            >
              ℹ️ .NET Environment Variables Set:
            </div>
            <Show when={props.currentEnv["DOTNET_ROOT"]}>
              <div
                style={{
                  "font-family": "var(--mono)",
                  "font-size": "0.7rem",
                  "margin-top": "0.15rem",
                }}
              >
                DOTNET_ROOT={props.currentEnv["DOTNET_ROOT"]}
              </div>
            </Show>
            <Show when={props.currentEnv["DOTNET_BUNDLE_EXTRACT_BASE_DIR"]}>
              <div
                style={{
                  "font-family": "var(--mono)",
                  "font-size": "0.7rem",
                  "margin-top": "0.15rem",
                }}
              >
                DOTNET_BUNDLE_EXTRACT_BASE_DIR=
                {props.currentEnv["DOTNET_BUNDLE_EXTRACT_BASE_DIR"]}
              </div>
            </Show>
          </div>
        </Show>
      </Show>
    </div>
  );
};

export function isDotnetBinary(binary: string): boolean {
  const trimmed = binary.trim().toLowerCase();
  if (!trimmed) return false;

  // Common .NET binary names
  if (trimmed === "dotnet" || trimmed === "dotnet.exe") return true;
  if (trimmed.endsWith("/dotnet") || trimmed.endsWith("\\dotnet")) return true;

  // Check for common .NET server binaries
  const dotnetExtensions = [".dll", ".exe"];
  const hasDotnetExt = dotnetExtensions.some((ext) => trimmed.endsWith(ext));

  // Common .NET game server patterns
  const dotnetPatterns = [
    "tshock",
    ".server",
    "terraria",
    "dotnet",
    "corehost",
  ];
  const matchesPattern = dotnetPatterns.some((pattern) =>
    trimmed.includes(pattern),
  );

  return hasDotnetExt || matchesPattern;
}

export default DotnetRuntimeSelector;
