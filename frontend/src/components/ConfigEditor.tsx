import {
  type ParentComponent,
  createSignal,
  createEffect,
  onMount,
  Show,
} from "solid-js";
import {
  createTemplate,
  validateSteamApp,
  getSteamCmdStatus,
} from "../api/client";
import MarkdownRenderer from "./MarkdownRenderer";
import AlertMuteSection from "./AlertMuteSection";
import type { ServerConfig } from "../types/bindings";

interface Props {
  config: ServerConfig;
  onSave?: (config: ServerConfig) => void;
  onChange?: (config: ServerConfig) => void;
  serverId?: string;
  showAlertMute?: boolean;
}

const ConfigEditor: ParentComponent<Props> = (props) => {
  const [templateDialogOpen, setTemplateDialogOpen] = createSignal(false);
  const [templateName, setTemplateName] = createSignal("");
  const [templateDesc, setTemplateDesc] = createSignal("");
  const [templateSaving, setTemplateSaving] = createSignal(false);
  const [templateError, setTemplateError] = createSignal<string | null>(null);
  const [templateDescTab, setTemplateDescTab] = createSignal<
    "write" | "preview"
  >("write");
  const [steamcmdAvailable, setSteamcmdAvailable] = createSignal<
    boolean | null
  >(null);
  const [steamcmdPath, setSteamcmdPath] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      const status = await getSteamCmdStatus();
      setSteamcmdAvailable(status.available);
      setSteamcmdPath(status.path ?? null);
    } catch {
      // Silently ignore — the indicator just won't show
    }
  });

  const [name, setName] = createSignal(props.config.name);
  const [steamAppId, setSteamAppId] = createSignal<string>(
    props.config.steam_app_id != null ? String(props.config.steam_app_id) : "",
  );
  const [steamAppName, setSteamAppName] = createSignal<string | null>(null);
  const [steamAppValidating, setSteamAppValidating] = createSignal(false);
  const [steamAppError, setSteamAppError] = createSignal<string | null>(null);
  const [autoStart, setAutoStart] = createSignal(props.config.auto_start);
  const [autoRestart, setAutoRestart] = createSignal(props.config.auto_restart);
  const [maxRestartAttempts, setMaxRestartAttempts] = createSignal(
    props.config.max_restart_attempts,
  );
  const [restartDelaySecs, setRestartDelaySecs] = createSignal(
    props.config.restart_delay_secs,
  );
  const [sftpUsername, setSftpUsername] = createSignal(
    props.config.sftp_username ?? "",
  );
  const [sftpPassword, setSftpPassword] = createSignal(
    props.config.sftp_password ?? "",
  );
  const [saved, setSaved] = createSignal(false);

  createEffect(() => {
    const c = props.config;
    setName(c.name);
    setSteamAppId(c.steam_app_id != null ? String(c.steam_app_id) : "");
    setAutoStart(c.auto_start);
    setAutoRestart(c.auto_restart);
    setMaxRestartAttempts(c.max_restart_attempts);
    setRestartDelaySecs(c.restart_delay_secs);
    setSftpUsername(c.sftp_username ?? "");
    setSftpPassword(c.sftp_password ?? "");
  });

  const parseSteamAppId = (): number | null => {
    const raw = steamAppId().trim();
    if (!raw) return null;
    const n = parseInt(raw, 10);
    return isNaN(n) || n <= 0 ? null : n;
  };

  const handleSteamAppIdBlur = async () => {
    const id = parseSteamAppId();
    setSteamAppName(null);
    setSteamAppError(null);
    if (id == null) return;
    setSteamAppValidating(true);
    try {
      const resp = await validateSteamApp(id);
      if (resp.valid && resp.app) {
        setSteamAppName(resp.app.name);
        setSteamAppError(null);
      } else {
        setSteamAppName(null);
        setSteamAppError(resp.error ?? "Invalid app ID");
      }
    } catch (e: any) {
      setSteamAppError(e.message || "Validation failed");
    } finally {
      setSteamAppValidating(false);
    }
  };

  const buildConfig = (): ServerConfig => {
    return {
      ...props.config,
      name: name(),
      steam_app_id: parseSteamAppId(),
      auto_start: autoStart(),
      auto_restart: autoRestart(),
      max_restart_attempts: maxRestartAttempts(),
      restart_delay_secs: restartDelaySecs(),
      sftp_username: sftpUsername().trim() || null,
      sftp_password: sftpPassword() || null,
    };
  };

  const emitChange = () => {
    setSaved(false);
    props.onChange?.(buildConfig());
  };

  const handleSave = () => {
    const config = buildConfig();
    props.onSave?.(config);
    props.onChange?.(config);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const handleExportJson = () => {
    const json = JSON.stringify(buildConfig(), null, 2);
    navigator.clipboard.writeText(json).then(
      () => alert("Configuration JSON copied to clipboard."),
      () => {
        prompt("Copy this JSON:", json);
      },
    );
  };

  const handleSaveAsTemplate = async () => {
    const tName = templateName().trim();
    if (!tName) {
      setTemplateError("Template name is required.");
      return;
    }
    setTemplateSaving(true);
    setTemplateError(null);
    try {
      await createTemplate({
        name: tName,
        description: templateDesc().trim() || null,
        config: buildConfig(),
      });
      setTemplateDialogOpen(false);
      setTemplateName("");
      setTemplateDesc("");
      setTemplateDescTab("write");
      alert(`Template "${tName}" saved successfully!`);
    } catch (e: any) {
      setTemplateError(e.message || "Failed to save template.");
    } finally {
      setTemplateSaving(false);
    }
  };

  const parseNumber = (value: string, fallback: number): number => {
    const n = parseInt(value, 10);
    return isNaN(n) || n < 0 ? fallback : n;
  };

  return (
    <div class="config-editor">
      <h3>Server Settings</h3>

      <div class="form-group">
        <label for="cfg-name">Server Name *</label>
        <input
          id="cfg-name"
          name="name"
          type="text"
          value={name()}
          onInput={(e) => {
            setName(e.currentTarget.value);
            emitChange();
          }}
          placeholder="My Game Server"
        />
      </div>

      <h3>SteamCMD</h3>
      <p
        style={{
          "font-size": "0.85rem",
          color: "#9ca3af",
          "margin-bottom": "0.5rem",
        }}
      >
        If this server is installed via SteamCMD, enter the Steam application
        ID. Pipeline steps can then use <code>SteamCMD Install</code> and{" "}
        <code>SteamCMD Update</code> actions.
      </p>
      <Show when={steamcmdAvailable() !== null}>
        <div
          class="field-status-inline"
          style={{
            display: "flex",
            "align-items": "center",
            gap: "0.4rem",
            "margin-bottom": "1rem",
            padding: "0.35rem 0.65rem",
            "border-radius": "6px",
            "font-size": "0.82rem",
            background: steamcmdAvailable()
              ? "rgba(34, 197, 94, 0.1)"
              : "rgba(239, 68, 68, 0.1)",
            color: steamcmdAvailable() ? "#22c55e" : "#f87171",
          }}
        >
          <Show
            when={steamcmdAvailable()}
            fallback={
              <>
                <span>⚠️ SteamCMD not found on host</span>
                <a
                  href="https://developer.valvesoftware.com/wiki/SteamCMD#Downloading_SteamCMD"
                  target="_blank"
                  rel="noopener noreferrer"
                  style={{
                    color: "#f87171",
                    "margin-left": "0.5rem",
                    "text-decoration": "underline",
                    "font-size": "0.8rem",
                  }}
                >
                  Install guide →
                </a>
              </>
            }
          >
            <span>
              ✓ SteamCMD available{steamcmdPath() ? ` (${steamcmdPath()})` : ""}
            </span>
          </Show>
        </div>
      </Show>
      <div class="form-group">
        <label for="cfg-steam-app-id">Steam App ID</label>
        <div
          style={{ display: "flex", "align-items": "center", gap: "0.5rem" }}
        >
          <input
            id="cfg-steam-app-id"
            name="steam_app_id"
            type="number"
            min="1"
            value={steamAppId()}
            onInput={(e) => {
              setSteamAppId(e.currentTarget.value);
              setSteamAppName(null);
              setSteamAppError(null);
              emitChange();
            }}
            onBlur={handleSteamAppIdBlur}
            placeholder="e.g. 896660"
            style={{ "max-width": "12rem" }}
          />
          <Show when={steamAppValidating()}>
            <span style={{ color: "#9ca3af", "font-size": "0.85rem" }}>
              Validating…
            </span>
          </Show>
          <Show when={steamAppName()}>
            <span
              style={{
                color: "#22c55e",
                "font-size": "0.85rem",
                display: "flex",
                "align-items": "center",
                gap: "0.25rem",
              }}
            >
              ✓ {steamAppName()}
            </span>
          </Show>
          <Show when={steamAppError()}>
            <span style={{ color: "#f87171", "font-size": "0.85rem" }}>
              ✗ {steamAppError()}
            </span>
          </Show>
        </div>
        <small>
          Leave empty if this server is not installed via SteamCMD. The ID is
          validated against the Steam store API.
        </small>
      </div>

      {props.children}

      <Show when={props.showAlertMute && props.serverId}>
        <AlertMuteSection serverId={props.serverId!} />
      </Show>

      <h3>Lifecycle</h3>

      <div class="form-row">
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={autoStart()}
            onChange={(e) => {
              setAutoStart(e.currentTarget.checked);
              emitChange();
            }}
          />
          Auto-start when AnyServer boots
        </label>

        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={autoRestart()}
            onChange={(e) => {
              setAutoRestart(e.currentTarget.checked);
              emitChange();
            }}
          />
          Auto-restart on crash
        </label>
      </div>

      <Show when={autoRestart()}>
        <div class="form-row">
          <div class="form-group">
            <label for="cfg-max-restarts">Max Restart Attempts</label>
            <input
              id="cfg-max-restarts"
              type="number"
              min="0"
              value={maxRestartAttempts()}
              onInput={(e) => {
                setMaxRestartAttempts(parseNumber(e.currentTarget.value, 0));
                emitChange();
              }}
            />
            <small>0 = unlimited restart attempts.</small>
          </div>

          <div class="form-group">
            <label for="cfg-restart-delay">Restart Delay (seconds)</label>
            <input
              id="cfg-restart-delay"
              type="number"
              min="0"
              value={restartDelaySecs()}
              onInput={(e) => {
                setRestartDelaySecs(parseNumber(e.currentTarget.value, 5));
                emitChange();
              }}
            />
          </div>
        </div>
      </Show>

      <h3>SFTP Access</h3>
      <p
        style={{
          "font-size": "0.85rem",
          color: "#9ca3af",
          "margin-bottom": "1rem",
        }}
      >
        Configure credentials for SFTP file access. Connections are jailed to
        this server's data directory.
      </p>

      <div class="form-row">
        <div class="form-group">
          <label for="cfg-sftp-user">SFTP Username</label>
          <input
            id="cfg-sftp-user"
            name="sftp_username"
            type="text"
            value={sftpUsername()}
            onInput={(e) => {
              setSftpUsername(e.currentTarget.value);
              emitChange();
            }}
            placeholder="Leave empty to disable SFTP"
          />
        </div>

        <div class="form-group">
          <label for="cfg-sftp-pass">SFTP Password</label>
          <input
            id="cfg-sftp-pass"
            name="sftp_password"
            type="password"
            value={sftpPassword()}
            onInput={(e) => {
              setSftpPassword(e.currentTarget.value);
              emitChange();
            }}
            placeholder={sftpUsername() ? "Required" : ""}
          />
        </div>
      </div>

      <Show when={templateDialogOpen()}>
        <div class="template-save-dialog">
          <h3>Save as Template</h3>
          <p
            style={{
              "font-size": "0.85rem",
              color: "#9ca3af",
              "margin-bottom": "1rem",
            }}
          >
            Save this configuration as a reusable template. Parameters, install
            steps, and update steps will all be included.
          </p>
          <Show when={templateError()}>
            <div class="error-msg" style={{ "margin-bottom": "0.75rem" }}>
              {templateError()}
            </div>
          </Show>
          <div class="form-group">
            <label>Template Name *</label>
            <input
              type="text"
              value={templateName()}
              onInput={(e) => setTemplateName(e.currentTarget.value)}
              placeholder="e.g. Minecraft Paper Server"
            />
          </div>
          <div class="form-group">
            <label>
              Description{" "}
              <small
                style={{ "font-weight": "normal", color: "var(--text-dim)" }}
              >
                (Markdown supported)
              </small>
            </label>
            <div class="markdown-preview-toggle">
              <button
                class={templateDescTab() === "write" ? "active" : ""}
                onClick={() => setTemplateDescTab("write")}
              >
                Write
              </button>
              <button
                class={templateDescTab() === "preview" ? "active" : ""}
                onClick={() => setTemplateDescTab("preview")}
              >
                Preview
              </button>
            </div>
            <Show
              when={templateDescTab() === "write"}
              fallback={
                <div class="markdown-preview-pane">
                  <Show
                    when={templateDesc().trim()}
                    fallback={
                      <p class="markdown-preview-empty">Nothing to preview</p>
                    }
                  >
                    <MarkdownRenderer content={templateDesc()} />
                  </Show>
                </div>
              }
            >
              <textarea
                value={templateDesc()}
                onInput={(e) => setTemplateDesc(e.currentTarget.value)}
                placeholder="Describe what this template sets up. You can use **bold**, *italic*, [links](url), `code`, lists, and more."
                rows={4}
              />
            </Show>
          </div>
          <div
            style={{
              display: "flex",
              gap: "0.75rem",
              "margin-top": "1rem",
            }}
          >
            <button
              class="btn btn-primary"
              onClick={handleSaveAsTemplate}
              disabled={templateSaving()}
            >
              {templateSaving() ? "Saving..." : "Save Template"}
            </button>
            <button
              class="btn"
              onClick={() => {
                setTemplateDialogOpen(false);
                setTemplateName("");
                setTemplateDesc("");
                setTemplateDescTab("write");
                setTemplateError(null);
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      </Show>

      <div
        style={{
          "margin-top": "1.5rem",
          display: "flex",
          "align-items": "center",
          gap: "0.75rem",
        }}
      >
        <Show when={props.onSave}>
          <button class="btn btn-primary" onClick={handleSave}>
            Save Configuration
          </button>
        </Show>
        <button class="btn" onClick={handleExportJson}>
          Export JSON
        </button>
        <Show when={!templateDialogOpen()}>
          <button class="btn" onClick={() => setTemplateDialogOpen(true)}>
            💾 Save as Template
          </button>
        </Show>
        <Show when={saved()}>
          <span style={{ color: "#22c55e", "font-size": "0.85rem" }}>
            ✓ Saved
          </span>
        </Show>
      </div>
    </div>
  );
};

export default ConfigEditor;
