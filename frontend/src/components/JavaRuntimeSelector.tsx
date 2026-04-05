import { type Component, createSignal, Show, For } from "solid-js";
import { getJavaRuntimes, getJavaEnv } from "../api/client";
import type { JavaRuntime } from "../types/bindings";

export interface JavaRuntimeSelectorProps {
  currentBinary: string;
  currentEnv: Record<string, string>;
  onEnvChange: (envVars: Record<string, string>) => void;
}

const JavaRuntimeSelector: Component<JavaRuntimeSelectorProps> = (props) => {
  const [runtimes, setRuntimes] = createSignal<JavaRuntime[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [detected, setDetected] = createSignal(false);
  const [expanded, setExpanded] = createSignal(false);
  const [applying, setApplying] = createSignal<string | null>(null);

  const detect = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await getJavaRuntimes();
      setRuntimes(resp.runtimes);
      setDetected(true);
      if (resp.runtimes.length > 0) {
        setExpanded(true);
      }
    } catch (e: any) {
      setError(e.message || "Failed to detect Java runtimes");
    } finally {
      setLoading(false);
    }
  };

  const applyRuntime = async (rt: JavaRuntime) => {
    setApplying(rt.java_home);
    setError(null);
    try {
      const envVars = await getJavaEnv(rt.java_home);
      props.onEnvChange(envVars);
    } catch (e: any) {
      setError(
        e.message || "Failed to generate environment variables for runtime",
      );
    } finally {
      setApplying(null);
    }
  };

  const clearJavaEnv = () => {
    props.onEnvChange({ JAVA_HOME: "" });
  };

  const formatRuntime = (rt: JavaRuntime): string => {
    const parts: string[] = [];
    parts.push(`Java ${rt.major_version}`);
    parts.push(`(${rt.version})`);
    if (rt.runtime_name && rt.runtime_name !== "Unknown Java Runtime") {
      const short = rt.runtime_name
        .replace("Runtime Environment", "RE")
        .replace("64-Bit Server VM", "")
        .trim();
      if (short) parts.push(`— ${short}`);
    }
    if (rt.is_default) parts.push("★ default");
    return parts.join(" ");
  };

  const isSelected = (rt: JavaRuntime): boolean => {
    const javaHome = props.currentEnv["JAVA_HOME"];
    if (!javaHome) return false;
    return javaHome === rt.java_home;
  };

  const isSystemDefault = (): boolean => {
    const javaHome = props.currentEnv["JAVA_HOME"];
    return !javaHome || javaHome.trim() === "";
  };

  const hasJavaHomeEnv = (): boolean => {
    return !!props.currentEnv["JAVA_HOME"];
  };

  const currentJavaHome = (): string | undefined => {
    return props.currentEnv["JAVA_HOME"];
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
          "flex-wrap": "wrap",
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
                ☕{" "}
                {!detected()
                  ? "Detect Java Runtimes"
                  : expanded()
                    ? "Hide Java Runtimes"
                    : "Show Java Runtimes"}
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
            title="Re-scan for Java installations"
          >
            🔄 Re-scan
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

        <Show when={hasJavaHomeEnv()}>
          <button
            type="button"
            class="btn btn-sm"
            onClick={clearJavaEnv}
            style={{
              "font-size": "0.72rem",
              padding: "0.2rem 0.5rem",
              color: "var(--danger)",
              "border-color": "var(--danger)",
            }}
            title="Remove JAVA_HOME and use system default"
          >
            ✕ Clear JAVA_HOME
          </button>
        </Show>
      </div>

      {/* Show current JAVA_HOME if set */}
      <Show when={hasJavaHomeEnv()}>
        <div
          style={{
            "margin-top": "0.35rem",
            padding: "0.35rem 0.6rem",
            background: "var(--primary-bg)",
            border: "1px solid var(--primary)",
            "border-radius": "var(--radius-sm)",
            "font-size": "0.75rem",
            color: "var(--text)",
            display: "flex",
            "align-items": "center",
            gap: "0.4rem",
          }}
        >
          <span style={{ "font-weight": "600", color: "var(--primary)" }}>
            JAVA_HOME
          </span>
          <code
            style={{
              "font-size": "0.72rem",
              "font-family": "var(--mono)",
              color: "var(--text-dim)",
            }}
          >
            {currentJavaHome()}
          </code>
        </div>
      </Show>

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
          {/* System Default option */}
          <button
            type="button"
            onClick={clearJavaEnv}
            style={{
              display: "flex",
              "align-items": "center",
              gap: "0.5rem",
              width: "100%",
              padding: "0.5rem 0.75rem",
              border: "none",
              "border-bottom": "1px solid var(--border)",
              background: isSystemDefault()
                ? "var(--primary-bg)"
                : "var(--bg-input)",
              color: "var(--text)",
              cursor: "pointer",
              "font-size": "0.82rem",
              "text-align": "left",
              transition: "background var(--transition)",
            }}
            onMouseEnter={(e) => {
              if (!isSystemDefault())
                e.currentTarget.style.background = "var(--bg-hover)";
            }}
            onMouseLeave={(e) => {
              if (!isSystemDefault())
                e.currentTarget.style.background = "var(--bg-input)";
            }}
          >
            <span
              style={{
                "flex-shrink": "0",
                width: "1.2em",
                "text-align": "center",
              }}
            >
              {isSystemDefault() ? "●" : "○"}
            </span>
            <div style={{ flex: "1", "min-width": "0" }}>
              <div style={{ "font-weight": "500" }}>System Default (PATH)</div>
              <div
                style={{
                  "font-size": "0.75rem",
                  color: "var(--text-dim)",
                  "font-family": "var(--mono)",
                }}
              >
                No JAVA_HOME override — uses whichever java is on PATH
                <Show when={runtimes().find((r) => r.is_default)}>
                  {" "}
                  → {runtimes().find((r) => r.is_default)!.path}
                </Show>
              </div>
            </div>
            <Show when={isSystemDefault()}>
              <span
                style={{
                  "font-size": "0.7rem",
                  color: "var(--primary)",
                  "font-weight": "600",
                  "flex-shrink": "0",
                }}
              >
                ACTIVE
              </span>
            </Show>
          </button>

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
                ⚠ No Java installations detected on the system.
                <br />
                <span
                  style={{ "font-size": "0.75rem", color: "var(--text-dim)" }}
                >
                  Install a JDK (e.g.{" "}
                  <code style={{ "font-size": "0.72rem" }}>
                    apt install openjdk-21-jre-headless
                  </code>
                  ) and re-scan.
                </span>
              </div>
            }
          >
            <For each={runtimes()}>
              {(rt) => {
                const selected = () => isSelected(rt);
                const isApplying = () => applying() === rt.java_home;
                return (
                  <button
                    type="button"
                    onClick={() => applyRuntime(rt)}
                    disabled={isApplying()}
                    style={{
                      display: "flex",
                      "align-items": "center",
                      gap: "0.5rem",
                      width: "100%",
                      padding: "0.5rem 0.75rem",
                      border: "none",
                      "border-bottom": "1px solid var(--border)",
                      background: selected()
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
                      if (!selected())
                        e.currentTarget.style.background = "var(--bg-hover)";
                    }}
                    onMouseLeave={(e) => {
                      if (!selected())
                        e.currentTarget.style.background = "var(--bg-input)";
                    }}
                  >
                    <span
                      style={{
                        "flex-shrink": "0",
                        width: "1.2em",
                        "text-align": "center",
                        color: selected()
                          ? "var(--primary)"
                          : "var(--text-dim)",
                        "margin-top": "0.15rem",
                      }}
                    >
                      {isApplying() ? (
                        <span class="btn-spinner" />
                      ) : selected() ? (
                        "●"
                      ) : (
                        "○"
                      )}
                    </span>
                    <div style={{ flex: "1", "min-width": "0" }}>
                      <div
                        style={{
                          "font-weight": "500",
                          display: "flex",
                          "align-items": "center",
                          gap: "0.4rem",
                          "flex-wrap": "wrap",
                        }}
                      >
                        <span>Java {rt.major_version}</span>
                        <span
                          style={{
                            "font-size": "0.72rem",
                            color: "var(--text-muted)",
                            "font-weight": "400",
                          }}
                        >
                          ({rt.version})
                        </span>
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
                        <Show when={selected()}>
                          <span
                            style={{
                              "font-size": "0.65rem",
                              padding: "0.1rem 0.35rem",
                              "border-radius": "3px",
                              background: "var(--primary-bg)",
                              color: "var(--primary)",
                              "font-weight": "600",
                              "letter-spacing": "0.02em",
                            }}
                          >
                            SELECTED
                          </span>
                        </Show>
                      </div>
                      <div
                        style={{
                          "font-size": "0.72rem",
                          color: "var(--text-dim)",
                          "margin-top": "0.1rem",
                        }}
                      >
                        <Show
                          when={
                            rt.runtime_name &&
                            rt.runtime_name !== "Unknown Java Runtime"
                          }
                        >
                          <span>{rt.runtime_name}</span>
                          <span style={{ margin: "0 0.3rem" }}>·</span>
                        </Show>
                        <code
                          style={{
                            "font-size": "0.7rem",
                            "font-family": "var(--mono)",
                          }}
                        >
                          {rt.java_home}
                        </code>
                      </div>
                    </div>
                    <Show when={selected()}>
                      <span
                        style={{
                          "font-size": "0.7rem",
                          color: "var(--primary)",
                          "font-weight": "600",
                          "flex-shrink": "0",
                        }}
                      >
                        ACTIVE
                      </span>
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
          Selecting a runtime sets{" "}
          <code style={{ "font-size": "0.68rem" }}>JAVA_HOME</code> in the
          server's environment variables. The backend automatically prepends{" "}
          <code style={{ "font-size": "0.68rem" }}>$JAVA_HOME/bin</code> to{" "}
          <code style={{ "font-size": "0.68rem" }}>PATH</code> at launch, so
          shell scripts and wrappers that call{" "}
          <code style={{ "font-size": "0.68rem" }}>java</code> will use the
          selected runtime.
        </div>
      </Show>
    </div>
  );
};

export function isJavaBinary(binary: string): boolean {
  const trimmed = binary.trim().toLowerCase();
  if (!trimmed) return false;
  if (trimmed === "java" || trimmed === "java.exe") return true;
  if (trimmed.endsWith("/java") || trimmed.endsWith("\\java")) return true;
  if (
    (trimmed.includes("jdk") || trimmed.includes("jre")) &&
    (trimmed.includes("java") || trimmed.includes("bin"))
  )
    return true;
  return false;
}

export default JavaRuntimeSelector;
