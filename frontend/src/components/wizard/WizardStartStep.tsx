import { type Component, Show, For, createSignal, createMemo } from "solid-js";
import type { ServerConfig } from "../../types/bindings";
import ParamRefHint from "../ParamRefHint";
import JavaRuntimeSelector, { isJavaBinary } from "../JavaRuntimeSelector";
import DotnetRuntimeSelector, {
  isDotnetBinary,
} from "../DotnetRuntimeSelector";

interface Props {
  config: ServerConfig;
  parameterNames: string[];
  argsText: string;
  envText: string;
  onPatchConfig: (updates: Partial<ServerConfig>) => void;
  onSetArgsText: (text: string) => void;
  onSetEnvText: (text: string) => void;
  parseNumber: (value: string, fallback: number) => number;
}

const WizardStartStep: Component<Props> = (props) => {
  // Determine if helpers should be shown based on binary auto-detection or manual toggle
  const showJavaHelper = createMemo(
    () => isJavaBinary(props.config.binary) || props.config.enable_java_helper,
  );
  const showDotnetHelper = createMemo(
    () =>
      isDotnetBinary(props.config.binary) || props.config.enable_dotnet_helper,
  );

  return (
    <div class="wizard-step-content">
      <Show when={props.parameterNames.length > 0}>
        <div class="param-availability-banner">
          💡 Available parameters:{" "}
          <For each={props.parameterNames}>
            {(name, i) => (
              <>
                <code class="param-ref-hint-name param-ref-hint-name-ok">
                  {"${"}
                  {name}
                  {"}"}
                </code>
                {i() < props.parameterNames.length - 1 ? " " : ""}
              </>
            )}
          </For>
        </div>
      </Show>

      <h4>Process</h4>

      <div class="form-group">
        <label for="wiz-binary">Binary Path *</label>
        <input
          id="wiz-binary"
          type="text"
          value={props.config.binary}
          onInput={(e) =>
            props.onPatchConfig({ binary: e.currentTarget.value })
          }
          placeholder="/usr/bin/java or relative/path/to/binary"
        />
        <small>
          Absolute path to the executable, or relative to the server's data
          directory. Supports {"${param}"} substitution.
        </small>
        <ParamRefHint
          value={props.config.binary}
          parameterNames={props.parameterNames}
          showAvailable
        />
        <Show when={showJavaHelper()}>
          <JavaRuntimeSelector
            currentBinary={props.config.binary}
            onSelect={(path) => props.onPatchConfig({ binary: path })}
          />
        </Show>
        <Show when={showDotnetHelper()}>
          <DotnetRuntimeSelector
            currentBinary={props.config.binary}
            currentEnv={props.config.env}
            onSelect={(envVars) => {
              const merged = { ...props.config.env };
              for (const [key, value] of Object.entries(envVars)) {
                if (value === "") {
                  delete merged[key];
                } else {
                  merged[key] = value;
                }
              }
              props.onPatchConfig({ env: merged });
              // Update the env text to reflect the changes
              const newEnvText = Object.entries(merged)
                .map(([k, v]) => `${k}=${v}`)
                .join("\n");
              props.onSetEnvText(newEnvText);
            }}
          />
        </Show>
      </div>

      <div class="form-group">
        <label for="wiz-args">Arguments</label>
        <input
          id="wiz-args"
          type="text"
          value={props.argsText}
          onInput={(e) => {
            const text = e.currentTarget.value;
            props.onSetArgsText(text);
            const args = text.split(/\s+/).filter((a) => a.length > 0);
            props.onPatchConfig({ args });
          }}
          placeholder="--port 25565 --max-players 20"
        />
        <small>
          Space-separated list of arguments. Supports {"${param}"} substitution.
        </small>
        <ParamRefHint
          value={props.argsText}
          parameterNames={props.parameterNames}
        />
      </div>

      <div class="form-group">
        <label for="wiz-env">Environment Variables</label>
        <textarea
          id="wiz-env"
          value={props.envText}
          onInput={(e) => {
            const text = e.currentTarget.value;
            props.onSetEnvText(text);
            const env: Record<string, string> = {};
            text
              .split("\n")
              .map((l) => l.trim())
              .filter((l) => l.length > 0 && l.includes("="))
              .forEach((l) => {
                const idx = l.indexOf("=");
                const k = l.slice(0, idx).trim();
                const v = l.slice(idx + 1);
                if (k) env[k] = v;
              });
            props.onPatchConfig({ env });
          }}
          placeholder={"JAVA_HOME=/usr/lib/jvm/java-17\nSERVER_MODE=production"}
          rows={4}
        />
        <small>One variable per line in KEY=VALUE format.</small>
        <ParamRefHint
          value={props.envText}
          parameterNames={props.parameterNames}
        />
      </div>

      <h4>Lifecycle</h4>

      <div class="form-group">
        <label for="wiz-stop-cmd">Stop Command</label>
        <input
          id="wiz-stop-cmd"
          type="text"
          value={props.config.stop_command ?? ""}
          onInput={(e) =>
            props.onPatchConfig({
              stop_command: e.currentTarget.value || null,
            })
          }
          placeholder='e.g. "stop" for Minecraft servers'
        />
        <small>
          Sent to the process's stdin to request a graceful shutdown. If empty,
          SIGTERM is used.
        </small>
        <ParamRefHint
          value={props.config.stop_command}
          parameterNames={props.parameterNames}
        />
      </div>

      <div class="form-row">
        <div class="form-group">
          <label for="wiz-stop-timeout">Stop Timeout (seconds)</label>
          <input
            id="wiz-stop-timeout"
            type="number"
            min="1"
            value={props.config.stop_timeout_secs}
            onInput={(e) =>
              props.onPatchConfig({
                stop_timeout_secs: props.parseNumber(e.currentTarget.value, 10),
              })
            }
          />
        </div>
        <div class="form-group">
          <label for="wiz-restart-delay">Restart Delay (seconds)</label>
          <input
            id="wiz-restart-delay"
            type="number"
            min="0"
            value={props.config.restart_delay_secs}
            onInput={(e) =>
              props.onPatchConfig({
                restart_delay_secs: props.parseNumber(e.currentTarget.value, 5),
              })
            }
          />
        </div>
      </div>

      <div class="form-row">
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={props.config.auto_start}
            onChange={(e) =>
              props.onPatchConfig({ auto_start: e.currentTarget.checked })
            }
          />
          Auto-start when AnyServer boots
        </label>

        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={props.config.auto_restart}
            onChange={(e) =>
              props.onPatchConfig({ auto_restart: e.currentTarget.checked })
            }
          />
          Auto-restart on crash
        </label>
      </div>

      <Show when={props.config.auto_restart}>
        <div class="form-group">
          <label for="wiz-max-restarts">Max Restart Attempts</label>
          <input
            id="wiz-max-restarts"
            type="number"
            min="0"
            value={props.config.max_restart_attempts}
            onInput={(e) =>
              props.onPatchConfig({
                max_restart_attempts: props.parseNumber(
                  e.currentTarget.value,
                  0,
                ),
              })
            }
          />
          <small>0 = unlimited restart attempts.</small>
        </div>
      </Show>

      <h4>SFTP Access</h4>
      <p
        style={{
          "font-size": "0.85rem",
          color: "#9ca3af",
          "margin-bottom": "1rem",
        }}
      >
        Optional credentials for SFTP file access, jailed to this server's
        directory.
      </p>

      <div class="form-row">
        <div class="form-group">
          <label for="wiz-sftp-user">SFTP Username</label>
          <input
            id="wiz-sftp-user"
            type="text"
            value={props.config.sftp_username ?? ""}
            onInput={(e) =>
              props.onPatchConfig({
                sftp_username: e.currentTarget.value || null,
              })
            }
            placeholder="Leave empty to disable SFTP"
          />
        </div>
        <div class="form-group">
          <label for="wiz-sftp-pass">SFTP Password</label>
          <input
            id="wiz-sftp-pass"
            type="password"
            value={props.config.sftp_password ?? ""}
            onInput={(e) =>
              props.onPatchConfig({
                sftp_password: e.currentTarget.value || null,
              })
            }
            placeholder={props.config.sftp_username ? "Required" : ""}
          />
        </div>
      </div>

      <h4 style={{ "margin-top": "2rem" }}>Runtime Helpers</h4>
      <p
        style={{
          "font-size": "0.85rem",
          color: "#9ca3af",
          "margin-bottom": "1rem",
        }}
      >
        Enable runtime helpers for custom binaries that require Java or .NET
        underneath. When enabled, the runtime selector is always shown above,
        allowing you to configure the appropriate runtime environment.
      </p>

      <div class="form-row">
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={props.config.enable_java_helper}
            onChange={(e) =>
              props.onPatchConfig({
                enable_java_helper: e.currentTarget.checked,
              })
            }
          />
          Enable Java Runtime Helper
        </label>

        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={props.config.enable_dotnet_helper}
            onChange={(e) =>
              props.onPatchConfig({
                enable_dotnet_helper: e.currentTarget.checked,
              })
            }
          />
          Enable .NET Runtime Helper
        </label>
      </div>

      <Show
        when={
          props.config.enable_java_helper || props.config.enable_dotnet_helper
        }
      >
        <div
          style={{
            "margin-top": "0.5rem",
            padding: "0.75rem",
            background: "rgba(59, 130, 246, 0.1)",
            border: "1px solid rgba(59, 130, 246, 0.3)",
            "border-radius": "0.25rem",
            "font-size": "0.8rem",
            color: "var(--text-muted)",
            "line-height": "1.5",
          }}
        >
          💡 <strong>Tip:</strong> These toggles are useful when your binary
          path doesn't match auto-detection patterns (e.g., custom wrapper
          scripts, launchers, or non-standard executable names) but the
          underlying process requires a Java or .NET runtime. The appropriate
          runtime selector will appear above for configuration.
        </div>
      </Show>
    </div>
  );
};

export default WizardStartStep;
