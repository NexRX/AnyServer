import { type Component, Show } from "solid-js";
import type { ServerConfig, EffectivePermission } from "../../types/bindings";
import ConfigEditor from "../ConfigEditor";
import ParameterEditor from "../ParameterEditor";
import AlertMuteSection from "../AlertMuteSection";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface ServerConfigTabProps {
  /** The current server ID. */
  serverId: string;
  /** The current server config. */
  config: ServerConfig;
  /** The current parameter values. */
  parameterValues: Record<string, string>;
  /** The user's effective permission on this server. */
  permission: EffectivePermission;
  /** Whether an action is currently in flight (disables destructive buttons). */
  actionBusy: boolean;
  /** The ID of the currently active action, if any. */
  activeAction: string | null;
  /** Whether the current user is a global admin. */
  isGlobalAdmin: boolean;
  /** Called when the user saves the config editor form. */
  onConfigSave: (config: ServerConfig) => void;
  /** Called when the user saves parameter values. */
  onParamSave: (values: Record<string, string>) => void;
  /** Called when the user clicks "Kill Orphaned Processes". */
  onKillProcesses: () => void;
  /** Called when the user clicks "Reset Server". */
  onReset: () => void;
  /** Called when the user clicks "Delete Server". */
  onDelete: () => void;
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function hasMinLevel(level: string, minLevel: string): boolean {
  const RANK: Record<string, number> = {
    viewer: 0,
    operator: 1,
    manager: 2,
    admin: 3,
    owner: 4,
  };
  return (RANK[level] ?? 0) >= (RANK[minLevel] ?? 0);
}

// ─── Component ──────────────────────────────────────────────────────────────

const ServerConfigTab: Component<ServerConfigTabProps> = (props) => {
  const hasParameters = () => (props.config.parameters?.length ?? 0) > 0;

  const canSeeAlertMute = () =>
    hasMinLevel(props.permission.level, "manager") ||
    props.permission.is_global_admin;

  const canSeeDangerZone = () =>
    hasMinLevel(props.permission.level, "admin") ||
    props.permission.is_global_admin;

  return (
    <>
      {/* Config editor with optional inline parameter editor */}
      <ConfigEditor
        config={props.config}
        onSave={props.onConfigSave}
        serverId={props.serverId}
        showAlertMute={false}
      >
        <Show when={hasParameters()}>
          <ParameterEditor
            title="Pipeline Parameters"
            parameters={props.config.parameters ?? []}
            values={props.parameterValues}
            onSave={props.onParamSave}
            bare
          />
        </Show>
      </ConfigEditor>

      {/* Alert mute section (manager+) */}
      <Show when={canSeeAlertMute()}>
        <AlertMuteSection serverId={props.serverId} />
      </Show>

      {/* Danger zone (admin+) */}
      <Show when={canSeeDangerZone()}>
        <div class="danger-zone">
          <h3 class="danger-zone-title">Administration</h3>
          <p class="danger-zone-description">
            These actions are destructive and cannot be undone. Only server
            admins and owners can see this section.
          </p>

          <div class="danger-zone-actions">
            {/* Kill orphaned processes */}
            <div class="danger-zone-item">
              <div class="danger-zone-item-info">
                <h4>Kill Orphaned Processes</h4>
                <p>
                  Forcefully terminate all OS processes running inside this
                  server's data directory. Use this to clean up zombie processes
                  that survived a bad shutdown (e.g. a Minecraft server holding{" "}
                  <code>session.lock</code>).
                </p>
              </div>
              <button
                class="btn btn-danger-outline"
                onClick={props.onKillProcesses}
                disabled={props.actionBusy}
              >
                <Show
                  when={props.activeAction === "kill-processes"}
                  fallback={"💀"}
                >
                  <span class="btn-spinner" />
                </Show>{" "}
                Kill Processes
              </button>
            </div>

            {/* Reset server */}
            <div class="danger-zone-item">
              <div class="danger-zone-item-info">
                <h4>Reset Server</h4>
                <p>
                  Stop the server, kill any orphaned processes, delete{" "}
                  <strong>all</strong> server files, and mark the server as
                  uninstalled. You will need to re-run the install pipeline
                  afterwards. Configuration and pipeline settings are preserved.
                </p>
              </div>
              <button
                class="btn btn-danger"
                onClick={props.onReset}
                disabled={props.actionBusy}
              >
                <Show
                  when={props.activeAction === "reset"}
                  fallback={"🔄"}
                >
                  <span class="btn-spinner" />
                </Show>{" "}
                Reset Server
              </button>
            </div>

            {/* Delete server (global admin only) */}
            <Show when={props.isGlobalAdmin}>
              <div class="danger-zone-item">
                <div class="danger-zone-item-info">
                  <h4>Delete Server</h4>
                  <p>
                    Permanently remove this server, all of its files,
                    configuration, and permissions. This action is{" "}
                    <strong>irreversible</strong>.
                  </p>
                </div>
                <button
                  class="btn btn-danger"
                  onClick={props.onDelete}
                  disabled={props.actionBusy}
                >
                  <Show
                    when={props.activeAction === "delete"}
                    fallback={"🗑"}
                  >
                    <span class="btn-spinner" />
                  </Show>{" "}
                  Delete Server
                </button>
              </div>
            </Show>
          </div>
        </div>
      </Show>
    </>
  );
};

export default ServerConfigTab;
