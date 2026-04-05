import {
  type Component,
  Show,
  For,
  createSignal,
  createEffect,
} from "solid-js";
import { A } from "@solidjs/router";
import type {
  StepAction,
  StopSignal,
  ArchiveFormat,
  FileOperation,
} from "../types/bindings";
import ParamRefHint from "./ParamRefHint";
import { useIntegrationStatus } from "../context/integrations";
import { useAuth } from "../context/auth";

const ACTION_TYPES = [
  { value: "download", label: "Download" },
  { value: "extract", label: "Extract Archive" },
  { value: "move", label: "Move" },
  { value: "copy", label: "Copy" },
  { value: "delete", label: "Delete" },
  { value: "create_dir", label: "Create Directory" },
  { value: "run_command", label: "Run Command" },
  { value: "write_file", label: "Write File" },
  { value: "edit_file", label: "Edit File" },
  { value: "set_permissions", label: "Set Permissions" },
  { value: "glob", label: "Glob / Rename" },
  { value: "set_env", label: "Set Environment" },
  { value: "set_working_dir", label: "Set Working Directory" },
  { value: "set_stop_command", label: "Set Stop Command" },
  { value: "set_stop_signal", label: "Set Stop Signal" },
  { value: "send_input", label: "Send Input (stdin)" },
  { value: "send_signal", label: "Send Signal" },
  { value: "sleep", label: "Sleep / Wait" },
  { value: "wait_for_output", label: "Wait for Console Output" },
  { value: "resolve_variable", label: "Resolve Variable (API)" },
  {
    value: "download_github_release_asset",
    label: "Download GitHub Release Asset",
  },
  {
    value: "download_curse_forge_file",
    label: "Download CurseForge Server Pack",
  },
  { value: "steam_cmd_install", label: "SteamCMD Install" },
  { value: "steam_cmd_update", label: "SteamCMD Update" },
] as const;

const ARCHIVE_FORMATS: { value: ArchiveFormat; label: string }[] = [
  { value: "auto", label: "Auto-detect" },
  { value: "zip", label: "ZIP" },
  { value: "tar_gz", label: "tar.gz" },
  { value: "tar_bz2", label: "tar.bz2" },
  { value: "tar_xz", label: "tar.xz" },
  { value: "tar", label: "tar" },
];

const FILE_OPERATION_TYPES = [
  { value: "overwrite", label: "Overwrite" },
  { value: "append", label: "Append" },
  { value: "prepend", label: "Prepend" },
  { value: "find_replace", label: "Find & Replace" },
  { value: "regex_replace", label: "Regex Replace" },
  { value: "insert_after", label: "Insert After" },
  { value: "insert_before", label: "Insert Before" },
  { value: "replace_line", label: "Replace Line" },
] as const;

function defaultAction(type: string): StepAction {
  switch (type) {
    case "download":
      return {
        type: "download",
        url: "",
        destination: ".",
        filename: null,
        executable: false,
      };
    case "extract":
      return {
        type: "extract",
        source: "",
        destination: null,
        format: "auto" as ArchiveFormat,
      };
    case "move":
      return { type: "move", source: "", destination: "" };
    case "copy":
      return { type: "copy", source: "", destination: "", recursive: true };
    case "delete":
      return { type: "delete", path: "", recursive: false };
    case "create_dir":
      return { type: "create_dir", path: "" };
    case "run_command":
      return {
        type: "run_command",
        command: "",
        args: [],
        working_dir: null,
        env: {},
      };
    case "write_file":
      return { type: "write_file", path: "", content: "" };
    case "edit_file":
      return {
        type: "edit_file",
        path: "",
        operation: { type: "overwrite", content: "" } as FileOperation,
      };
    case "set_permissions":
      return { type: "set_permissions", path: "", mode: "755" };
    case "glob":
      return { type: "glob", pattern: "", destination: "" };
    case "set_env":
      return { type: "set_env", variables: {} };
    case "set_working_dir":
      return { type: "set_working_dir", path: "" };
    case "set_stop_command":
      return { type: "set_stop_command", command: "" };
    case "set_stop_signal":
      return { type: "set_stop_signal", signal: "sigterm" as StopSignal };
    case "send_input":
      return { type: "send_input", text: "" };
    case "send_signal":
      return { type: "send_signal", signal: "sigterm" as StopSignal };
    case "sleep":
      return { type: "sleep", seconds: 5 };
    case "wait_for_output":
      return { type: "wait_for_output", pattern: "", timeout_secs: 30 };
    case "resolve_variable":
      return {
        type: "resolve_variable",
        url: "",
        path: null,
        pick: "last" as any,
        value_key: null,
        variable: "",
      };
    case "download_github_release_asset":
      return {
        type: "download_github_release_asset",
        tag_param: "",
        asset_matcher: "",
        destination: ".",
        filename: null,
        executable: false,
      };
    case "download_curse_forge_file":
      return {
        type: "download_curse_forge_file",
        file_param: "",
        destination: ".",
        filename: null,
        executable: false,
      };
    case "steam_cmd_install":
    case "steam_cmd_update":
      return {
        type: type as "steam_cmd_install" | "steam_cmd_update",
        app_id: null,
        anonymous: true,
        extra_args: [],
      };
    default:
      return defaultAction("download");
  }
}

function defaultFileOperation(type: string): FileOperation {
  switch (type) {
    case "overwrite":
      return { type: "overwrite", content: "" };
    case "append":
      return { type: "append", content: "" };
    case "prepend":
      return { type: "prepend", content: "" };
    case "find_replace":
      return { type: "find_replace", find: "", replace: "", all: true };
    case "regex_replace":
      return { type: "regex_replace", pattern: "", replace: "", all: true };
    case "insert_after":
      return { type: "insert_after", pattern: "", content: "" };
    case "insert_before":
      return { type: "insert_before", pattern: "", content: "" };
    case "replace_line":
      return { type: "replace_line", pattern: "", content: "", all: false };
    default:
      return { type: "overwrite", content: "" };
  }
}

interface Props {
  action: StepAction;
  onChange: (action: StepAction) => void;
  parameterNames?: string[];
  /** Full parameter definitions, used to filter by type for specific step actions */
  parameters?: Array<{ name: string; param_type: string }>;
}

const StepActionEditor: Component<Props> = (props) => {
  const integrations = useIntegrationStatus();
  const auth = useAuth();
  const paramNames = () => props.parameterNames ?? [];

  const [argsText, setArgsText] = createSignal("");
  const [envText, setEnvText] = createSignal("");
  const [setEnvVarsText, setSetEnvVarsText] = createSignal("");

  createEffect(() => {
    const a = props.action;
    if (a.type === "run_command") {
      setArgsText(a.args.join("\n"));
      setEnvText(
        Object.entries(a.env)
          .map(([k, v]) => `${k}=${v}`)
          .join("\n"),
      );
    }
    if (a.type === "set_env") {
      setSetEnvVarsText(
        Object.entries(a.variables)
          .map(([k, v]) => `${k}=${v}`)
          .join("\n"),
      );
    }
  });

  const handleTypeChange = (newType: string) => {
    props.onChange(defaultAction(newType));
  };

  const patch = (updates: Record<string, unknown>) => {
    props.onChange({ ...props.action, ...updates } as StepAction);
  };

  const patchFileOp = (updates: Record<string, unknown>) => {
    if (props.action.type !== "edit_file") return;
    const op = props.action.operation;
    props.onChange({
      ...props.action,
      operation: { ...op, ...updates } as FileOperation,
    });
  };

  const handleFileOpTypeChange = (newType: string) => {
    if (props.action.type !== "edit_file") return;
    props.onChange({
      ...props.action,
      operation: defaultFileOperation(newType),
    });
  };

  // ─── Render sub-editors per action type ───

  const renderDownload = () => {
    const a = () => props.action as Extract<StepAction, { type: "download" }>;
    return (
      <>
        <div class="form-group">
          <label>URL *</label>
          <input
            type="text"
            value={a().url}
            onInput={(e) => patch({ url: e.currentTarget.value })}
            placeholder="https://example.com/server.jar"
          />
          <ParamRefHint
            value={a().url}
            parameterNames={paramNames()}
            showAvailable
          />
        </div>
        <div class="form-group">
          <label>Destination Directory</label>
          <input
            type="text"
            value={a().destination}
            onInput={(e) => patch({ destination: e.currentTarget.value })}
            placeholder="."
          />
          <ParamRefHint value={a().destination} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Filename Override</label>
          <input
            type="text"
            value={a().filename ?? ""}
            onInput={(e) => patch({ filename: e.currentTarget.value || null })}
            placeholder="(auto-detect from URL)"
          />
          <ParamRefHint value={a().filename} parameterNames={paramNames()} />
        </div>
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={a().executable}
            onChange={(e) => patch({ executable: e.currentTarget.checked })}
          />
          Make executable (chmod +x)
        </label>
      </>
    );
  };

  const renderExtract = () => {
    const a = () => props.action as Extract<StepAction, { type: "extract" }>;
    return (
      <>
        <div class="form-group">
          <label>Source Archive *</label>
          <input
            type="text"
            value={a().source}
            onInput={(e) => patch({ source: e.currentTarget.value })}
            placeholder="server.tar.gz"
          />
          <small>Path to the archive, relative to server directory.</small>
          <ParamRefHint value={a().source} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Destination Directory</label>
          <input
            type="text"
            value={a().destination ?? ""}
            onInput={(e) =>
              patch({ destination: e.currentTarget.value || null })
            }
            placeholder="(server root)"
          />
          <ParamRefHint value={a().destination} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Archive Format</label>
          <select
            value={a().format}
            onChange={(e) =>
              patch({ format: e.currentTarget.value as ArchiveFormat })
            }
          >
            <For each={ARCHIVE_FORMATS}>
              {(f) => <option value={f.value}>{f.label}</option>}
            </For>
          </select>
        </div>
      </>
    );
  };

  const renderMove = () => {
    const a = () => props.action as Extract<StepAction, { type: "move" }>;
    return (
      <>
        <div class="form-group">
          <label>Source *</label>
          <input
            type="text"
            value={a().source}
            onInput={(e) => patch({ source: e.currentTarget.value })}
            placeholder="old/path/file.txt"
          />
          <ParamRefHint value={a().source} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Destination *</label>
          <input
            type="text"
            value={a().destination}
            onInput={(e) => patch({ destination: e.currentTarget.value })}
            placeholder="new/path/file.txt"
          />
          <ParamRefHint value={a().destination} parameterNames={paramNames()} />
        </div>
      </>
    );
  };

  const renderCopy = () => {
    const a = () => props.action as Extract<StepAction, { type: "copy" }>;
    return (
      <>
        <div class="form-group">
          <label>Source *</label>
          <input
            type="text"
            value={a().source}
            onInput={(e) => patch({ source: e.currentTarget.value })}
            placeholder="source/path"
          />
          <ParamRefHint value={a().source} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Destination *</label>
          <input
            type="text"
            value={a().destination}
            onInput={(e) => patch({ destination: e.currentTarget.value })}
            placeholder="dest/path"
          />
          <ParamRefHint value={a().destination} parameterNames={paramNames()} />
        </div>
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={a().recursive}
            onChange={(e) => patch({ recursive: e.currentTarget.checked })}
          />
          Copy recursively
        </label>
      </>
    );
  };

  const renderDelete = () => {
    const a = () => props.action as Extract<StepAction, { type: "delete" }>;
    return (
      <>
        <div class="form-group">
          <label>Path *</label>
          <input
            type="text"
            value={a().path}
            onInput={(e) => patch({ path: e.currentTarget.value })}
            placeholder="path/to/delete"
          />
          <ParamRefHint value={a().path} parameterNames={paramNames()} />
        </div>
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={a().recursive}
            onChange={(e) => patch({ recursive: e.currentTarget.checked })}
          />
          Delete recursively (directories)
        </label>
      </>
    );
  };

  const renderCreateDir = () => {
    const a = () => props.action as Extract<StepAction, { type: "create_dir" }>;
    return (
      <div class="form-group">
        <label>Directory Path *</label>
        <input
          type="text"
          value={a().path}
          onInput={(e) => patch({ path: e.currentTarget.value })}
          placeholder="data/configs"
        />
        <small>Creates all intermediate directories as needed.</small>
        <ParamRefHint value={a().path} parameterNames={paramNames()} />
      </div>
    );
  };

  const renderRunCommand = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "run_command" }>;
    return (
      <>
        <div class="form-group">
          <label>Command *</label>
          <input
            type="text"
            value={a().command}
            onInput={(e) => patch({ command: e.currentTarget.value })}
            placeholder="/bin/bash or java"
          />
          <small>The executable to run.</small>
          <ParamRefHint
            value={a().command}
            parameterNames={paramNames()}
            showAvailable
          />
        </div>
        <div class="form-group">
          <label>Arguments</label>
          <textarea
            value={argsText()}
            onInput={(e) => {
              const text = e.currentTarget.value;
              setArgsText(text);
              const args = text
                .split("\n")
                .map((l) => l.trim())
                .filter((l) => l.length > 0);
              patch({ args });
            }}
            placeholder={
              "One argument per line, e.g.:\n-jar\nserver.jar\n--nogui"
            }
            rows={4}
          />
          <small>One argument per line.</small>
        </div>
        <div class="form-group">
          <label>Working Directory</label>
          <input
            type="text"
            value={a().working_dir ?? ""}
            onInput={(e) =>
              patch({ working_dir: e.currentTarget.value || null })
            }
            placeholder="(server root)"
          />
          <ParamRefHint value={a().working_dir} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Environment Variables</label>
          <textarea
            value={envText()}
            onInput={(e) => {
              const text = e.currentTarget.value;
              setEnvText(text);
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
              patch({ env });
            }}
            placeholder={"KEY=VALUE\nJAVA_HOME=/usr/lib/jvm/java-17"}
            rows={3}
          />
          <small>One variable per line in KEY=VALUE format.</small>
        </div>
      </>
    );
  };

  const renderWriteFile = () => {
    const a = () => props.action as Extract<StepAction, { type: "write_file" }>;
    return (
      <>
        <div class="form-group">
          <label>File Path *</label>
          <input
            type="text"
            value={a().path}
            onInput={(e) => patch({ path: e.currentTarget.value })}
            placeholder="config/server.properties"
          />
          <ParamRefHint value={a().path} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Content *</label>
          <textarea
            value={a().content}
            onInput={(e) => patch({ content: e.currentTarget.value })}
            placeholder="File contents..."
            rows={8}
          />
          <ParamRefHint
            value={a().content}
            parameterNames={paramNames()}
            showAvailable
          />
        </div>
      </>
    );
  };

  const renderSetPermissions = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "set_permissions" }>;
    return (
      <>
        <div class="form-group">
          <label>File Path *</label>
          <input
            type="text"
            value={a().path}
            onInput={(e) => patch({ path: e.currentTarget.value })}
            placeholder="server.sh"
          />
          <ParamRefHint value={a().path} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Mode *</label>
          <input
            type="text"
            value={a().mode}
            onInput={(e) => patch({ mode: e.currentTarget.value })}
            placeholder="755"
          />
          <small>Octal permission string (e.g. 755, 644).</small>
        </div>
      </>
    );
  };

  const renderGlob = () => {
    const a = () => props.action as Extract<StepAction, { type: "glob" }>;
    return (
      <>
        <div class="form-group">
          <label>Glob Pattern *</label>
          <input
            type="text"
            value={a().pattern}
            onInput={(e) => patch({ pattern: e.currentTarget.value })}
            placeholder="server-*.jar"
          />
          <small>Matches files relative to the server directory.</small>
          <ParamRefHint value={a().pattern} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Destination *</label>
          <input
            type="text"
            value={a().destination}
            onInput={(e) => patch({ destination: e.currentTarget.value })}
            placeholder="server.jar"
          />
          <small>Rename the matched file(s) to this path.</small>
          <ParamRefHint value={a().destination} parameterNames={paramNames()} />
        </div>
      </>
    );
  };

  // ─── Start Pipeline Configuration Steps ───

  const renderSetEnv = () => {
    const a = () => props.action as Extract<StepAction, { type: "set_env" }>;
    return (
      <>
        <div class="form-group">
          <label>Environment Variables *</label>
          <textarea
            value={setEnvVarsText()}
            onInput={(e) => {
              const text = e.currentTarget.value;
              setSetEnvVarsText(text);
              const variables: Record<string, string> = {};
              text
                .split("\n")
                .map((l) => l.trim())
                .filter((l) => l.length > 0 && l.includes("="))
                .forEach((l) => {
                  const idx = l.indexOf("=");
                  const k = l.slice(0, idx).trim();
                  const v = l.slice(idx + 1);
                  if (k) variables[k] = v;
                });
              patch({ variables });
            }}
            placeholder={
              "JAVA_HOME=/usr/lib/jvm/java-17\nSERVER_MODE=production"
            }
            rows={4}
          />
          <small>
            One per line in KEY=VALUE format. These are merged into the server
            process environment. Only effective in start pipelines.
          </small>
          <ParamRefHint
            value={setEnvVarsText()}
            parameterNames={paramNames()}
          />
        </div>
      </>
    );
  };

  const renderSetWorkingDir = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "set_working_dir" }>;
    return (
      <>
        <div class="form-group">
          <label>Working Directory *</label>
          <input
            type="text"
            value={a().path}
            onInput={(e) => patch({ path: e.currentTarget.value })}
            placeholder="relative/path/from/server/dir"
          />
          <small>
            Relative to the server's data directory. Overrides the static
            working directory setting. Only effective in start pipelines.
          </small>
          <ParamRefHint value={a().path} parameterNames={paramNames()} />
        </div>
      </>
    );
  };

  const renderSetStopCommand = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "set_stop_command" }>;
    return (
      <>
        <div class="form-group">
          <label>Stop Command *</label>
          <input
            type="text"
            value={a().command}
            onInput={(e) => patch({ command: e.currentTarget.value })}
            placeholder='e.g. "stop" for Minecraft'
          />
          <small>
            Sent to the server's stdin for graceful shutdown. Overrides the
            static stop command setting. Only effective in start pipelines.
          </small>
          <ParamRefHint value={a().command} parameterNames={paramNames()} />
        </div>
      </>
    );
  };

  const renderSetStopSignal = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "set_stop_signal" }>;
    return (
      <>
        <div class="form-group">
          <label>Stop Signal *</label>
          <select
            value={a().signal}
            onChange={(e) =>
              patch({ signal: e.currentTarget.value as StopSignal })
            }
          >
            <option value="sigterm">
              SIGTERM — standard graceful termination (default)
            </option>
            <option value="sigint">SIGINT — equivalent to Ctrl+C</option>
          </select>
          <small>
            Sets the signal sent to the process group during graceful stop when
            no stop command is configured. Overrides the static stop signal
            setting. Only effective in start pipelines.
          </small>
        </div>
      </>
    );
  };

  const renderSendInput = () => {
    const a = () => props.action as Extract<StepAction, { type: "send_input" }>;
    return (
      <>
        <div class="form-group">
          <label>Text to Send *</label>
          <input
            type="text"
            value={a().text}
            onInput={(e) => patch({ text: e.currentTarget.value })}
            placeholder='e.g. "stop" for Minecraft'
          />
          <small>
            Sends this text to the running server's stdin followed by a newline.
            Primarily useful in stop pipelines (e.g. sending "stop" to a
            Minecraft server before killing the process).
          </small>
          <ParamRefHint value={a().text} parameterNames={paramNames()} />
        </div>
      </>
    );
  };

  const renderSendSignal = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "send_signal" }>;
    return (
      <>
        <div class="form-group">
          <label>Signal *</label>
          <select
            value={a().signal}
            onChange={(e) =>
              patch({ signal: e.currentTarget.value as StopSignal })
            }
          >
            <option value="sigterm">SIGTERM — graceful termination</option>
            <option value="sigint">SIGINT — Ctrl+C</option>
          </select>
          <small>
            Sends the selected signal to the server's process group. Use in stop
            pipelines after sending a stdin command and waiting.
          </small>
        </div>
      </>
    );
  };

  const renderSleep = () => {
    const a = () => props.action as Extract<StepAction, { type: "sleep" }>;
    return (
      <>
        <div class="form-group">
          <label>Duration (seconds) *</label>
          <input
            type="number"
            min="1"
            value={a().seconds}
            onInput={(e) => {
              const n = parseInt(e.currentTarget.value, 10);
              patch({ seconds: isNaN(n) || n < 1 ? 1 : n });
            }}
            style={{ "max-width": "8rem" }}
          />
          <small>
            Pauses execution for the specified number of seconds. Use in stop
            pipelines to give the server time to shut down gracefully between
            steps.
          </small>
        </div>
      </>
    );
  };

  const renderWaitForOutput = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "wait_for_output" }>;
    return (
      <>
        <div class="form-group">
          <label>Pattern to Match *</label>
          <input
            type="text"
            value={a().pattern}
            onInput={(e) => patch({ pattern: e.currentTarget.value })}
            placeholder='e.g. "Closing Server" or "Saving the game"'
          />
          <small>
            Waits until this text appears in the server's console output
            (case-insensitive substring match). Useful in stop pipelines to wait
            for a confirmation message before proceeding to the next step.
          </small>
          <ParamRefHint value={a().pattern} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Timeout (seconds) *</label>
          <input
            type="number"
            min="1"
            value={a().timeout_secs}
            onInput={(e) => {
              const n = parseInt(e.currentTarget.value, 10);
              patch({ timeout_secs: isNaN(n) || n < 1 ? 30 : n });
            }}
            style={{ "max-width": "8rem" }}
          />
          <small>
            Maximum time to wait for the pattern. If the timeout expires without
            a match, the pipeline continues to the next step.
          </small>
        </div>
      </>
    );
  };

  const renderResolveVariable = () => {
    const a = () =>
      props.action as Extract<StepAction, { type: "resolve_variable" }>;
    return (
      <>
        <div class="form-group">
          <label>API URL *</label>
          <input
            type="text"
            value={a().url}
            onInput={(e) => patch({ url: e.currentTarget.value })}
            placeholder="https://api.papermc.io/v2/projects/paper/versions/${mc_version}/builds"
          />
          <small>
            The URL to GET. Supports <code>{"${param}"}</code> variable
            substitution from parameters and previously resolved variables.
          </small>
          <ParamRefHint value={a().url} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>JSON Path</label>
          <input
            type="text"
            value={a().path ?? ""}
            onInput={(e) => patch({ path: e.currentTarget.value || null })}
            placeholder='e.g. "builds" or "data.versions"'
          />
          <small>
            Dot-separated path to navigate the JSON response. Leave empty to use
            the root.
          </small>
        </div>
        <div class="form-group">
          <label>Pick</label>
          <select
            value={a().pick}
            onChange={(e) => patch({ pick: e.currentTarget.value })}
          >
            <option value="last">Last (latest)</option>
            <option value="first">First</option>
          </select>
          <small>
            When the resolved value is an array, which element to pick.
          </small>
        </div>
        <div class="form-group">
          <label>Value Key</label>
          <input
            type="text"
            value={a().value_key ?? ""}
            onInput={(e) => patch({ value_key: e.currentTarget.value || null })}
            placeholder='e.g. "build" or "tag_name"'
          />
          <small>
            If the picked element is an object, which key to extract. Leave
            empty if the element is a string or number.
          </small>
        </div>
        <div class="form-group">
          <label>Variable Name *</label>
          <input
            type="text"
            value={a().variable}
            onInput={(e) => patch({ variable: e.currentTarget.value })}
            placeholder="e.g. paper_build"
          />
          <small>
            The resolved value is stored in this variable. Subsequent steps can
            reference it as <code>{"${variable_name}"}</code>.
          </small>
        </div>
      </>
    );
  };

  // ─── Edit File sub-editors ───

  const renderDownloadGithubReleaseAsset = () => {
    const a = props.action as Extract<
      StepAction,
      { type: "download_github_release_asset" }
    >;
    return (
      <div class="step-action-fields">
        {/* GitHub token not configured — soft info, not a blocker */}
        <Show when={!integrations.status().github_configured}>
          <div
            style={{
              display: "flex",
              "align-items": "flex-start",
              gap: "0.5rem",
              padding: "0.5rem 0.75rem",
              background: "rgba(250, 204, 21, 0.06)",
              border: "1px solid rgba(250, 204, 21, 0.2)",
              "border-radius": "0.375rem",
              "margin-bottom": "0.75rem",
              "font-size": "0.82rem",
              color: "#fde68a",
            }}
          >
            <span style={{ "flex-shrink": "0" }}>ℹ️</span>
            <div>
              <strong>No GitHub token configured.</strong> This step works fine
              for public repos — private repos and higher rate limits require a
              token.
              <Show when={auth.isAdmin()}>
                {" "}
                <A
                  href="/admin"
                  style={{
                    color: "#facc15",
                    "text-decoration": "underline",
                  }}
                  onClick={() => sessionStorage.setItem("admin_tab", "github")}
                >
                  Configure →
                </A>
              </Show>
            </div>
          </div>
        </Show>
        <div class="form-group">
          <label>Tag Parameter</label>
          <input
            type="text"
            value={a.tag_param}
            onInput={(e) => patch({ tag_param: e.currentTarget.value })}
            placeholder="parameter_name"
          />
          <small>
            Must reference a parameter of type "GitHub Release Tag" in the
            template's parameter list.
          </small>
        </div>
        <div class="form-group">
          <label>Asset Matcher</label>
          <input
            type="text"
            value={a.asset_matcher}
            onInput={(e) => patch({ asset_matcher: e.currentTarget.value })}
            placeholder="/pattern/ or exact-filename.zip"
          />
          <small>
            Exact filename or regex wrapped in forward slashes (e.g.{" "}
            <code>/server-.*\.jar$/</code>).
          </small>
          <ParamRefHint value={a.asset_matcher} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Destination</label>
          <input
            type="text"
            value={a.destination}
            onInput={(e) => patch({ destination: e.currentTarget.value })}
            placeholder="."
          />
          <ParamRefHint value={a.destination} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Filename Override (optional)</label>
          <input
            type="text"
            value={a.filename ?? ""}
            onInput={(e) => patch({ filename: e.currentTarget.value || null })}
            placeholder="(use original asset name)"
          />
          <ParamRefHint value={a.filename} parameterNames={paramNames()} />
        </div>
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={a.executable}
            onChange={(e) => patch({ executable: e.currentTarget.checked })}
          />
          Mark as executable (Unix only)
        </label>
      </div>
    );
  };

  const renderDownloadCurseForgeFile = () => {
    const a = props.action as Extract<
      StepAction,
      { type: "download_curse_forge_file" }
    >;

    // Filter to only CurseForge file version parameters
    const cfParams = () =>
      (props.parameters ?? []).filter(
        (p) => p.param_type === "curse_forge_file_version",
      );

    return (
      <div class="step-action-fields">
        {/* CurseForge integration not configured warning */}
        <Show when={!integrations.status().curseforge_configured}>
          <div
            style={{
              display: "flex",
              "align-items": "flex-start",
              gap: "0.5rem",
              padding: "0.6rem 0.75rem",
              background: "rgba(249, 115, 22, 0.08)",
              border: "1px solid rgba(249, 115, 22, 0.3)",
              "border-radius": "0.375rem",
              "margin-bottom": "0.75rem",
              "font-size": "0.85rem",
              color: "#fdba74",
            }}
          >
            <span style={{ "flex-shrink": "0" }}>🔶</span>
            <div>
              <strong>CurseForge API key not configured.</strong> This step will
              fail at runtime until an admin configures the API key.
              <Show
                when={auth.isAdmin()}
                fallback={
                  <span>
                    {" "}
                    Ask an admin to set it up in Admin Panel → CurseForge.
                  </span>
                }
              >
                {" "}
                <A
                  href="/admin"
                  style={{
                    color: "#fb923c",
                    "text-decoration": "underline",
                  }}
                  onClick={() =>
                    sessionStorage.setItem("admin_tab", "curseforge")
                  }
                >
                  Configure CurseForge API key →
                </A>
              </Show>
            </div>
          </div>
        </Show>
        <div class="form-group">
          <label>File Version Parameter</label>
          <Show
            when={cfParams().length > 0}
            fallback={
              <div>
                <input
                  type="text"
                  value={a.file_param}
                  onInput={(e) => patch({ file_param: e.currentTarget.value })}
                  placeholder="parameter_name"
                />
                <small style={{ color: "#f59e0b" }}>
                  No CurseForge File Version parameters defined yet. Add one in
                  the Parameters section above, or type a parameter name
                  manually.
                </small>
              </div>
            }
          >
            <select
              value={a.file_param}
              onChange={(e) => patch({ file_param: e.currentTarget.value })}
            >
              <option value="">— select parameter —</option>
              <For each={cfParams()}>
                {(p) => <option value={p.name}>{p.name}</option>}
              </For>
            </select>
            <small>
              Must reference a parameter of type "CurseForge File Version". The
              user's selected file ID will be used to resolve and download the
              server pack.
            </small>
          </Show>
        </div>
        <div class="form-group">
          <label>Destination</label>
          <input
            type="text"
            value={a.destination}
            onInput={(e) => patch({ destination: e.currentTarget.value })}
            placeholder="."
          />
          <ParamRefHint value={a.destination} parameterNames={paramNames()} />
        </div>
        <div class="form-group">
          <label>Filename Override (optional)</label>
          <input
            type="text"
            value={a.filename ?? ""}
            onInput={(e) => patch({ filename: e.currentTarget.value || null })}
            placeholder="(use original filename from CurseForge)"
          />
          <small>
            Leave blank to use the server pack's original filename. Set a value
            to rename the downloaded file.
          </small>
          <ParamRefHint value={a.filename} parameterNames={paramNames()} />
        </div>
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={a.executable}
            onChange={(e) => patch({ executable: e.currentTarget.checked })}
          />
          Mark as executable (Unix only)
        </label>
      </div>
    );
  };

  const renderSteamCmdInstall = () => {
    const a = props.action as Extract<
      StepAction,
      { type: "steam_cmd_install" }
    >;
    return (
      <>
        <div class="form-group">
          <label>App ID Override</label>
          <input
            type="number"
            min="1"
            value={a.app_id ?? ""}
            onInput={(e) => {
              const v = e.currentTarget.value.trim();
              const n = parseInt(v, 10);
              patch({ app_id: v === "" || isNaN(n) || n <= 0 ? null : n });
            }}
            placeholder="Leave empty to use server's Steam App ID"
          />
          <small>
            If empty, the server config's <code>steam_app_id</code> is used.
          </small>
        </div>
        <div class="form-group">
          <label class="checkbox-label">
            <input
              type="checkbox"
              checked={a.anonymous}
              onChange={(e) => patch({ anonymous: e.currentTarget.checked })}
            />
            Anonymous login
          </label>
          <small>Most dedicated servers allow anonymous login.</small>
        </div>
        <div class="form-group">
          <label>Extra Arguments</label>
          <input
            type="text"
            value={(a.extra_args ?? []).join(" ")}
            onInput={(e) => {
              const text = e.currentTarget.value;
              const args = text.trim() ? text.split(/\s+/) : [];
              patch({ extra_args: args });
            }}
            placeholder="e.g. -beta experimental"
          />
          <small>
            Additional arguments passed to SteamCMD before <code>+quit</code>.
            Supports <code>{"${param}"}</code> variable references.
          </small>
        </div>
      </>
    );
  };

  const renderFileOperation = () => {
    if (props.action.type !== "edit_file") return null;
    const op = () =>
      (props.action as Extract<StepAction, { type: "edit_file" }>).operation;

    return (
      <div class="step-action-subeditor">
        <div class="form-group">
          <label>Operation Type</label>
          <select
            value={op().type}
            onChange={(e) => handleFileOpTypeChange(e.currentTarget.value)}
          >
            <For each={FILE_OPERATION_TYPES}>
              {(t) => <option value={t.value}>{t.label}</option>}
            </For>
          </select>
        </div>

        {/* Overwrite / Append / Prepend — just content */}
        <Show
          when={
            op().type === "overwrite" ||
            op().type === "append" ||
            op().type === "prepend"
          }
        >
          <div class="form-group">
            <label>Content *</label>
            <textarea
              value={(op() as { content: string }).content}
              onInput={(e) => patchFileOp({ content: e.currentTarget.value })}
              rows={6}
              placeholder="Content to write..."
            />
          </div>
        </Show>

        {/* Find & Replace */}
        <Show when={op().type === "find_replace"}>
          <div class="form-group">
            <label>Find *</label>
            <input
              type="text"
              value={
                (op() as Extract<FileOperation, { type: "find_replace" }>).find
              }
              onInput={(e) => patchFileOp({ find: e.currentTarget.value })}
              placeholder="Text to find"
            />
          </div>
          <div class="form-group">
            <label>Replace With *</label>
            <input
              type="text"
              value={
                (op() as Extract<FileOperation, { type: "find_replace" }>)
                  .replace
              }
              onInput={(e) => patchFileOp({ replace: e.currentTarget.value })}
              placeholder="Replacement text"
            />
          </div>
          <label class="checkbox-label">
            <input
              type="checkbox"
              checked={
                (op() as Extract<FileOperation, { type: "find_replace" }>).all
              }
              onChange={(e) => patchFileOp({ all: e.currentTarget.checked })}
            />
            Replace all occurrences
          </label>
        </Show>

        {/* Regex Replace */}
        <Show when={op().type === "regex_replace"}>
          <div class="form-group">
            <label>Regex Pattern *</label>
            <input
              type="text"
              value={
                (op() as Extract<FileOperation, { type: "regex_replace" }>)
                  .pattern
              }
              onInput={(e) => patchFileOp({ pattern: e.currentTarget.value })}
              placeholder="\\d+\\.\\d+\\.\\d+"
            />
            <small>
              Rust regex syntax. Supports capture groups ($1, $2, etc.).
            </small>
          </div>
          <div class="form-group">
            <label>Replace With *</label>
            <input
              type="text"
              value={
                (op() as Extract<FileOperation, { type: "regex_replace" }>)
                  .replace
              }
              onInput={(e) => patchFileOp({ replace: e.currentTarget.value })}
              placeholder="Replacement (supports $1, $2...)"
            />
          </div>
          <label class="checkbox-label">
            <input
              type="checkbox"
              checked={
                (op() as Extract<FileOperation, { type: "regex_replace" }>).all
              }
              onChange={(e) => patchFileOp({ all: e.currentTarget.checked })}
            />
            Replace all matches
          </label>
        </Show>

        {/* Insert After / Insert Before */}
        <Show
          when={op().type === "insert_after" || op().type === "insert_before"}
        >
          <div class="form-group">
            <label>Pattern (line match) *</label>
            <input
              type="text"
              value={(op() as { pattern: string }).pattern}
              onInput={(e) => patchFileOp({ pattern: e.currentTarget.value })}
              placeholder="Text to find within a line"
            />
            <small>
              The first line containing this text will be the anchor point.
            </small>
          </div>
          <div class="form-group">
            <label>Content to Insert *</label>
            <textarea
              value={(op() as { content: string }).content}
              onInput={(e) => patchFileOp({ content: e.currentTarget.value })}
              rows={4}
              placeholder="New line(s) to insert"
            />
          </div>
        </Show>

        {/* Replace Line */}
        <Show when={op().type === "replace_line"}>
          <div class="form-group">
            <label>Pattern (line match) *</label>
            <input
              type="text"
              value={
                (op() as Extract<FileOperation, { type: "replace_line" }>)
                  .pattern
              }
              onInput={(e) => patchFileOp({ pattern: e.currentTarget.value })}
              placeholder="Text to find within a line"
            />
          </div>
          <div class="form-group">
            <label>Replacement Line *</label>
            <input
              type="text"
              value={
                (op() as Extract<FileOperation, { type: "replace_line" }>)
                  .content
              }
              onInput={(e) => patchFileOp({ content: e.currentTarget.value })}
              placeholder="Full replacement line"
            />
          </div>
          <label class="checkbox-label">
            <input
              type="checkbox"
              checked={
                (op() as Extract<FileOperation, { type: "replace_line" }>).all
              }
              onChange={(e) => patchFileOp({ all: e.currentTarget.checked })}
            />
            Replace all matching lines
          </label>
        </Show>
      </div>
    );
  };

  const renderEditFile = () => {
    const a = () => props.action as Extract<StepAction, { type: "edit_file" }>;
    return (
      <>
        <div class="form-group">
          <label>File Path *</label>
          <input
            type="text"
            value={a().path}
            onInput={(e) => patch({ path: e.currentTarget.value })}
            placeholder="config/server.properties"
          />
          <ParamRefHint value={a().path} parameterNames={paramNames()} />
        </div>
        {renderFileOperation()}
      </>
    );
  };

  const actionLabel = (type: string) =>
    ACTION_TYPES.find((t) => t.value === type)?.label ?? type;

  return (
    <div class="step-action-editor">
      <div class="form-group">
        <label>Action Type</label>
        <select
          value={props.action.type}
          onChange={(e) => handleTypeChange(e.currentTarget.value)}
        >
          <For each={ACTION_TYPES}>
            {(t) => <option value={t.value}>{t.label}</option>}
          </For>
        </select>
      </div>

      <div class="step-action-fields">
        <Show when={props.action.type === "download"}>{renderDownload()}</Show>
        <Show when={props.action.type === "extract"}>{renderExtract()}</Show>
        <Show when={props.action.type === "move"}>{renderMove()}</Show>
        <Show when={props.action.type === "copy"}>{renderCopy()}</Show>
        <Show when={props.action.type === "delete"}>{renderDelete()}</Show>
        <Show when={props.action.type === "create_dir"}>
          {renderCreateDir()}
        </Show>
        <Show when={props.action.type === "run_command"}>
          {renderRunCommand()}
        </Show>
        <Show when={props.action.type === "write_file"}>
          {renderWriteFile()}
        </Show>
        <Show when={props.action.type === "edit_file"}>{renderEditFile()}</Show>
        <Show when={props.action.type === "set_permissions"}>
          {renderSetPermissions()}
        </Show>
        <Show when={props.action.type === "glob"}>{renderGlob()}</Show>
        <Show when={props.action.type === "set_env"}>{renderSetEnv()}</Show>
        <Show when={props.action.type === "set_working_dir"}>
          {renderSetWorkingDir()}
        </Show>
        <Show when={props.action.type === "set_stop_command"}>
          {renderSetStopCommand()}
        </Show>
        <Show when={props.action.type === "set_stop_signal"}>
          {renderSetStopSignal()}
        </Show>
        <Show when={props.action.type === "send_input"}>
          {renderSendInput()}
        </Show>
        <Show when={props.action.type === "send_signal"}>
          {renderSendSignal()}
        </Show>
        <Show when={props.action.type === "sleep"}>{renderSleep()}</Show>
        <Show when={props.action.type === "wait_for_output"}>
          {renderWaitForOutput()}
        </Show>
        <Show when={props.action.type === "resolve_variable"}>
          {renderResolveVariable()}
        </Show>
        <Show when={props.action.type === "download_github_release_asset"}>
          {renderDownloadGithubReleaseAsset()}
        </Show>
        <Show when={props.action.type === "download_curse_forge_file"}>
          {renderDownloadCurseForgeFile()}
        </Show>
        <Show
          when={
            props.action.type === "steam_cmd_install" ||
            props.action.type === "steam_cmd_update"
          }
        >
          {renderSteamCmdInstall()}
        </Show>
      </div>
    </div>
  );
};

export default StepActionEditor;
