import {
  type Component,
  createSignal,
  createMemo,
  Show,
} from "solid-js";
import type {
  ServerConfig,
  PipelineStep,
  ConfigParameter,
} from "../../types/bindings";
import PipelineEditor from "../PipelineEditor";
import ParameterDefinitionEditor from "../ParameterDefinitionEditor";
import ParamRefHint from "../ParamRefHint";
import JavaRuntimeSelector, { isJavaBinary } from "../JavaRuntimeSelector";
import DotnetRuntimeSelector, { isDotnetBinary } from "../DotnetRuntimeSelector";
import { updateServer } from "../../api/client";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface ServerPipelinesTabProps {
  /** The current server ID. */
  serverId: string;
  /** The current server config (from the fetched server data). */
  config: ServerConfig;
  /** The current parameter values (from the fetched server data). */
  parameterValues: Record<string, string>;
  /** The server_dir field (used by DotnetRuntimeSelector). */
  serverDir: string | undefined;
  /** Called after a successful pipeline save to refresh server data. */
  onRefetch: () => void;
  /** Show an error toast. */
  onError: (msg: string) => void;
  /** Show a success toast. */
  onSuccess: (msg: string) => void;
  /** Called when the user wants to open the save-as-template dialog. */
  onSaveAsTemplate: () => void;
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function parseEnvText(text: string): Record<string, string> {
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
  return env;
}

function envToText(env: Record<string, string>): string {
  return Object.entries(env)
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
}

// ─── Component ──────────────────────────────────────────────────────────────

const ServerPipelinesTab: Component<ServerPipelinesTabProps> = (props) => {
  // ── Pipeline step editing state ──
  const [editStopSteps, setEditStopSteps] = createSignal<PipelineStep[] | null>(null);
  const [editStartSteps, setEditStartSteps] = createSignal<PipelineStep[] | null>(null);
  const [editInstallSteps, setEditInstallSteps] = createSignal<PipelineStep[] | null>(null);
  const [editUpdateSteps, setEditUpdateSteps] = createSignal<PipelineStep[] | null>(null);
  const [editUninstallSteps, setEditUninstallSteps] = createSignal<PipelineStep[] | null>(null);
  const [editParameters, setEditParameters] = createSignal<ConfigParameter[] | null>(null);

  // ── Start configuration editing state ──
  const [editBinary, setEditBinary] = createSignal<string | null>(null);
  const [editArgsText, setEditArgsText] = createSignal<string | null>(null);
  const [editEnvText, setEditEnvText] = createSignal<string | null>(null);
  const [editWorkingDir, setEditWorkingDir] = createSignal<string | null>(null);
  const [editStopCommand, setEditStopCommand] = createSignal<string | null>(null);
  const [editStopSignal, setEditStopSignal] = createSignal<string | null>(null);
  const [editStopTimeout, setEditStopTimeout] = createSignal<number | null>(null);

  // ── UI state ──
  const [pipelineSaved, setPipelineSaved] = createSignal(false);
  const [startConfigCollapsed, setStartConfigCollapsed] = createSignal(false);

  // ── Initialize editing state from server config on first render ──
  // We use null to mean "no local edits yet — use server value". On first
  // render of the pipelines tab, ServerDetail used to eagerly populate these.
  // Now we do it lazily: each field falls back to props.config.*.
  const initIfNeeded = () => {
    if (editStopSteps() === null) setEditStopSteps([...props.config.stop_steps]);
    if (editStartSteps() === null) setEditStartSteps([...props.config.start_steps]);
    if (editInstallSteps() === null) setEditInstallSteps([...props.config.install_steps]);
    if (editUpdateSteps() === null) setEditUpdateSteps([...props.config.update_steps]);
    if (editUninstallSteps() === null) setEditUninstallSteps([...props.config.uninstall_steps]);
    if (editParameters() === null) setEditParameters([...props.config.parameters]);
    if (editBinary() === null) setEditBinary(props.config.binary);
    if (editArgsText() === null) setEditArgsText(props.config.args.join(" "));
    if (editEnvText() === null) setEditEnvText(envToText(props.config.env));
    if (editWorkingDir() === null) setEditWorkingDir(props.config.working_dir ?? "");
    if (editStopCommand() === null) setEditStopCommand(props.config.stop_command ?? "");
    if (editStopSignal() === null) setEditStopSignal(props.config.stop_signal ?? "sigterm");
    if (editStopTimeout() === null) setEditStopTimeout(props.config.stop_timeout_secs);
  };

  // Run on mount
  initIfNeeded();

  // ── Derived state ──

  const hasPipelineChanges = () =>
    editStopSteps() !== null ||
    editStartSteps() !== null ||
    editInstallSteps() !== null ||
    editUpdateSteps() !== null ||
    editUninstallSteps() !== null ||
    editParameters() !== null ||
    editBinary() !== null ||
    editArgsText() !== null ||
    editEnvText() !== null ||
    editWorkingDir() !== null ||
    editStopCommand() !== null ||
    editStopSignal() !== null ||
    editStopTimeout() !== null;

  const parameterNames = createMemo(() => {
    const params = editParameters() ?? props.config.parameters ?? [];
    return params
      .map((p: ConfigParameter) => p.name)
      .filter((n: string) => n.trim() !== "");
  });

  // ── Current env as object (for DotnetRuntimeSelector) ──
  const currentEnvObject = (): Record<string, string> => {
    const text = editEnvText();
    if (text !== null) {
      return parseEnvText(text);
    }
    return props.config.env;
  };

  // ── Handlers ──

  const markDirty = () => setPipelineSaved(false);

  const handlePipelineSave = async () => {
    try {
      let args = props.config.args;
      if (editArgsText() !== null) {
        args = editArgsText()!
          .split(/\s+/)
          .filter((a) => a.length > 0);
      }

      let env = props.config.env;
      if (editEnvText() !== null) {
        env = parseEnvText(editEnvText()!);
      }

      const updatedConfig: ServerConfig = {
        ...props.config,
        binary: editBinary() ?? props.config.binary,
        args,
        env,
        working_dir:
          editWorkingDir() !== null
            ? editWorkingDir()!.trim() || null
            : props.config.working_dir,
        stop_command:
          editStopCommand() !== null
            ? editStopCommand()!.trim() || null
            : props.config.stop_command,
        stop_signal:
          (editStopSignal() as ServerConfig["stop_signal"]) ??
          props.config.stop_signal,
        stop_timeout_secs:
          editStopTimeout() ?? props.config.stop_timeout_secs,
        parameters: editParameters() ?? props.config.parameters,
        stop_steps: editStopSteps() ?? props.config.stop_steps,
        start_steps: editStartSteps() ?? props.config.start_steps,
        install_steps: editInstallSteps() ?? props.config.install_steps,
        update_steps: editUpdateSteps() ?? props.config.update_steps,
        uninstall_steps: editUninstallSteps() ?? props.config.uninstall_steps,
      };

      await updateServer(props.serverId, {
        config: updatedConfig,
        parameter_values: props.parameterValues,
      });

      props.onRefetch();

      // Reset local editing state (back to "no edits")
      setEditStopSteps(null);
      setEditStartSteps(null);
      setEditInstallSteps(null);
      setEditUpdateSteps(null);
      setEditUninstallSteps(null);
      setEditParameters(null);
      setEditBinary(null);
      setEditArgsText(null);
      setEditEnvText(null);
      setEditWorkingDir(null);
      setEditStopCommand(null);
      setEditStopSignal(null);
      setEditStopTimeout(null);

      setPipelineSaved(true);
      setTimeout(() => setPipelineSaved(false), 2000);
      props.onSuccess("Pipeline configuration saved");
    } catch (e: any) {
      props.onError(`Pipeline save failed: ${e.message || e}`);
    }
  };

  const handleDotnetEnvMerge = (envVars: Record<string, string>) => {
    const currentText =
      editEnvText() ?? envToText(props.config.env);
    const currentEnv = parseEnvText(currentText);

    const merged = { ...currentEnv };
    for (const [key, value] of Object.entries(envVars)) {
      if (value === "") {
        delete merged[key];
      } else {
        merged[key] = value;
      }
    }

    setEditEnvText(envToText(merged));
    markDirty();
  };

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div class="pipelines-tab">
      {/* Action bar */}
      <div class="pipeline-action-bar">
        <button
          class="btn btn-primary"
          onClick={handlePipelineSave}
          disabled={!hasPipelineChanges()}
          title={
            hasPipelineChanges()
              ? "Save pipeline configuration"
              : "No changes to save"
          }
        >
          Save Pipeline
        </button>
        <button class="btn" onClick={props.onSaveAsTemplate}>
          💾 Save as Template
        </button>
        <Show when={pipelineSaved()}>
          <span class="pipeline-saved-indicator">✓ Saved</span>
        </Show>
      </div>

      {/* Parameter definitions */}
      <div class="pipeline-section">
        <ParameterDefinitionEditor
          parameters={editParameters() ?? props.config.parameters}
          onChange={(params) => {
            setEditParameters([...params]);
            markDirty();
          }}
        />
      </div>

      {/* Start configuration */}
      <div class="pipeline-section">
        <div
          class="pipeline-editor-header pipeline-editor-header--clickable"
          onClick={() => setStartConfigCollapsed(!startConfigCollapsed())}
        >
          <div class="pipeline-editor-header-left">
            <span class="pipeline-editor-chevron">
              {startConfigCollapsed() ? "▶" : "▼"}
            </span>
            <h3 class="pipeline-editor-title">Start Configuration</h3>
            <Show when={startConfigCollapsed()}>
              <span class="pipeline-step-type-badge">
                {(editBinary() ?? props.config.binary) || "no binary"}
              </span>
            </Show>
          </div>
        </div>

        <Show when={!startConfigCollapsed()}>
          <div class="pipeline-editor-body">
            <p class="pipeline-editor-description">
              The binary, arguments, and environment used to launch the server
              process. Supports{" "}
              <code class="pipeline-editor-code">{"${param}"}</code>{" "}
              substitution.
            </p>

            {/* Binary path */}
            <div class="form-group">
              <label>Binary Path *</label>
              <input
                type="text"
                value={editBinary() ?? props.config.binary}
                onInput={(e) => {
                  setEditBinary(e.currentTarget.value);
                  markDirty();
                }}
                placeholder="/usr/bin/java or relative/path/to/binary"
              />
              <small>
                Absolute path to the executable, or relative to the server's
                data directory.
              </small>
              <ParamRefHint
                value={editBinary() ?? props.config.binary}
                parameterNames={parameterNames()}
              />
              <Show when={isJavaBinary(editBinary() ?? props.config.binary)}>
                <JavaRuntimeSelector
                  currentBinary={editBinary() ?? props.config.binary}
                  onSelect={(path) => {
                    setEditBinary(path);
                    markDirty();
                  }}
                />
              </Show>
              <Show when={isDotnetBinary(editBinary() ?? props.config.binary)}>
                <DotnetRuntimeSelector
                  currentBinary={editBinary() ?? props.config.binary}
                  currentEnv={currentEnvObject()}
                  onSelect={handleDotnetEnvMerge}
                  serverDir={props.serverDir}
                />
              </Show>
            </div>

            {/* Arguments */}
            <div class="form-group">
              <label>Arguments</label>
              <input
                type="text"
                value={editArgsText() ?? props.config.args.join(" ")}
                onInput={(e) => {
                  setEditArgsText(e.currentTarget.value);
                  markDirty();
                }}
                placeholder="--port 25565 --max-players 20"
              />
              <small>Space-separated command-line arguments.</small>
              <ParamRefHint
                value={editArgsText() ?? props.config.args.join(" ")}
                parameterNames={parameterNames()}
              />
            </div>

            {/* Environment variables */}
            <div class="form-group">
              <label>Environment Variables</label>
              <textarea
                value={editEnvText() ?? envToText(props.config.env)}
                onInput={(e) => {
                  setEditEnvText(e.currentTarget.value);
                  markDirty();
                }}
                placeholder="JAVA_HOME=/usr/lib/jvm/java-17&#10;SERVER_MODE=production"
                rows={3}
              />
              <small>One per line in KEY=VALUE format.</small>
            </div>

            {/* Working dir + Stop command */}
            <div class="form-row">
              <div class="form-group">
                <label>Working Directory</label>
                <input
                  type="text"
                  value={editWorkingDir() ?? props.config.working_dir ?? ""}
                  onInput={(e) => {
                    setEditWorkingDir(e.currentTarget.value);
                    markDirty();
                  }}
                  placeholder="Leave empty for server data root"
                />
                <small>Relative to the server's data directory.</small>
              </div>
              <div class="form-group">
                <label>Stop Command</label>
                <input
                  type="text"
                  value={editStopCommand() ?? props.config.stop_command ?? ""}
                  onInput={(e) => {
                    setEditStopCommand(e.currentTarget.value);
                    markDirty();
                  }}
                  placeholder='e.g. "stop" for Minecraft'
                />
                <small>Sent to stdin for graceful shutdown. Empty = SIGTERM.</small>
              </div>
            </div>

            {/* Stop signal */}
            <div class="form-group">
              <label>Stop Signal</label>
              <select
                value={editStopSignal() ?? props.config.stop_signal ?? "sigterm"}
                onChange={(e) => {
                  setEditStopSignal(e.currentTarget.value);
                  markDirty();
                }}
              >
                <option value="sigterm">
                  SIGTERM — standard graceful termination (default)
                </option>
                <option value="sigint">SIGINT — equivalent to Ctrl+C</option>
              </select>
              <small>
                When no stop command is configured, this signal is sent to the
                process group during graceful shutdown. Some servers (e.g. those
                wrapped in shell scripts) respond to Ctrl+C but not SIGTERM.
              </small>
            </div>

            {/* Stop timeout */}
            <div class="form-group">
              <label>Stop Timeout (seconds)</label>
              <input
                type="number"
                min="1"
                class="pipeline-stop-timeout-input"
                value={editStopTimeout() ?? props.config.stop_timeout_secs}
                onInput={(e) => {
                  const n = parseInt(e.currentTarget.value, 10);
                  setEditStopTimeout(isNaN(n) || n < 1 ? 10 : n);
                  markDirty();
                }}
              />
              <small>
                How long to wait before force-killing after stop command.
              </small>
            </div>
          </div>
        </Show>
      </div>

      {/* Stop pipeline */}
      <div class="pipeline-section">
        <PipelineEditor
          label="Stop Pipeline"
          description="Steps executed when stopping the server. Use Send Input to type commands (e.g. 'stop'), Sleep to wait, and Send Signal to send SIGTERM/SIGINT. If defined, this replaces the simple stop command / signal above. After all steps, the server is force-killed if still running."
          steps={editStopSteps() ?? props.config.stop_steps}
          onChange={(steps) => {
            setEditStopSteps([...steps]);
            markDirty();
          }}
          parameterNames={parameterNames()}
        />
      </div>

      {/* Pre-start steps */}
      <div class="pipeline-section">
        <PipelineEditor
          label="Pre-start Steps"
          description="Optional steps that run before the binary is launched each time the server starts (e.g. regenerate configs, check for updates)."
          steps={editStartSteps() ?? props.config.start_steps}
          onChange={(steps) => {
            setEditStartSteps([...steps]);
            markDirty();
          }}
          parameterNames={parameterNames()}
        />
      </div>

      {/* Install pipeline */}
      <div class="pipeline-section">
        <PipelineEditor
          label="Install Pipeline"
          description="Steps executed during first-time server setup. Download binaries, extract archives, write config files, etc."
          steps={editInstallSteps() ?? props.config.install_steps}
          onChange={(steps) => {
            setEditInstallSteps([...steps]);
            markDirty();
          }}
          parameterNames={parameterNames()}
        />
      </div>

      {/* Update pipeline */}
      <div class="pipeline-section">
        <PipelineEditor
          label="Update Pipeline"
          description="Steps executed when updating the server (e.g. download a new version, re-extract, apply patches)."
          steps={editUpdateSteps() ?? props.config.update_steps}
          onChange={(steps) => {
            setEditUpdateSteps([...steps]);
            markDirty();
          }}
          parameterNames={parameterNames()}
        />
      </div>

      {/* Uninstall pipeline */}
      <div class="pipeline-section">
        <PipelineEditor
          label="Uninstall Pipeline"
          description="Optional cleanup steps that run when uninstalling (e.g. remove downloaded files, reset state). Marks the server as not installed on success."
          steps={editUninstallSteps() ?? props.config.uninstall_steps}
          onChange={(steps) => {
            setEditUninstallSteps([...steps]);
            markDirty();
          }}
          parameterNames={parameterNames()}
        />
      </div>
    </div>
  );
};

export default ServerPipelinesTab;
