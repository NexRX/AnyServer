import Loader from "../components/Loader";
import {
  type Component,
  createResource,
  createSignal,
  createEffect,
  createMemo,
  Show,
  Switch,
  Match,
  onCleanup,
} from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { useAuth } from "../context/auth";
import {
  getServer,
  startServer,
  stopServer,
  cancelStop,
  restartServer,
  deleteServer,
  updateServer,
  installServer,
  updateServerPipeline,
  uninstallServer,
  cancelPhase,
  killServer,
  createTemplate,
  resetServer,
  listDirectoryProcesses,
  killDirectoryProcesses,
  markInstalled,
} from "../api/client";
import ResourcesTab from "../components/ResourcesTab";
import Console from "../components/Console";
import ConnectionBanner from "../components/ConnectionBanner";
import StatusTooltip from "../components/StatusTooltip";
import RestartBanner from "../components/RestartBanner";
import FileManager from "../components/FileManager";
import UpdateDialog from "../components/UpdateDialog";
import SandboxConfig from "../components/SandboxConfig";
import ServerAccessManager from "../components/ServerAccessManager";
import { useServerConsole } from "../hooks/useServerConsole";
import { useToastNotifications } from "../hooks/useToastNotifications";
import { useUpdateCheck } from "../hooks/useUpdateCheck";
import { useResourceStats } from "../hooks/useResourceStats";
import {
  canControl as _canControl,
  canRunPipelines as _canRunPipelines,
  canEditConfig as _canEditConfig,
  canDeleteServer as _canDeleteServer,
  LEVEL_DESCRIPTIONS,
  LEVEL_COLORS,
} from "../utils/permissions";
import {
  FloatingToast,
  ShutdownPanel,
  InstallDialog,
  KillProcessesDialog,
  UpdateCheckBanner,
  SaveTemplateDialog,
  ServerPipelinesTab,
  ServerConfigTab,
} from "../components/server-detail";

import type {
  ServerConfig,
  ServerStatus,
  ServerRuntime,
  ConfigParameter,
} from "../types/bindings";

// ─── Types ──────────────────────────────────────────────────────────────────

type Tab =
  | "console"
  | "files"
  | "config"
  | "pipelines"
  | "resources"
  | "sandbox"
  | "access";

type ActionId =
  | "start"
  | "stop"
  | "restart"
  | "kill"
  | "install"
  | "update"
  | "uninstall"
  | "cancel"
  | "cancel-stop"
  | "delete"
  | "reset"
  | "kill-processes"
  | null;

// ─── Status helpers ─────────────────────────────────────────────────────────

const isRunning = (status: ServerStatus | undefined): boolean =>
  status === "running" || status === "starting";

const canStart = (status: ServerStatus | undefined): boolean =>
  status === "stopped" || status === "crashed";

const canRestart = (status: ServerStatus | undefined): boolean =>
  status === "running" || status === "starting";

const canStop = (status: ServerStatus | undefined): boolean =>
  status === "running" || status === "starting";

const canKill = (status: ServerStatus | undefined): boolean =>
  status === "running" || status === "starting" || status === "stopping";

const formatUptime = (
  startedAt: string | null | undefined,
  _tick: number,
): string => {
  if (!startedAt) return "";
  const start = new Date(startedAt).getTime();
  const now = Date.now();
  const diffSec = Math.floor((now - start) / 1000);
  if (diffSec < 60) return `${diffSec}s`;
  const mins = Math.floor(diffSec / 60);
  const secs = diffSec % 60;
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const remainMins = mins % 60;
  if (hours < 24) return `${hours}h ${remainMins}m`;
  const days = Math.floor(hours / 24);
  const remainHours = hours % 24;
  return `${days}d ${remainHours}h ${remainMins}m`;
};

// ─── Component ──────────────────────────────────────────────────────────────

const ServerDetail: Component = () => {
  const params = useParams<{ id: string }>();
  const navigate = useNavigate();
  const auth = useAuth();

  const serverId = createMemo(() => params.id);

  // ── Server data ──
  const [data, { refetch, mutate }] = createResource(serverId, async (id) => {
    if (!id) throw new Error("No server ID provided");
    return await getServer(id);
  });

  // ── Permission helpers ──
  const perm = () => data()?.permission;
  const canControl = () => {
    const p = perm();
    return p ? _canControl(p) : false;
  };
  const canRunPipelines = () => {
    const p = perm();
    return p ? _canRunPipelines(p) : false;
  };
  const canEditConfig = () => {
    const p = perm();
    return p ? _canEditConfig(p) : false;
  };
  const canDeleteServer = () => {
    const p = perm();
    return p ? _canDeleteServer(p) : false;
  };

  // ── UI state ──
  const [tab, setTab] = createSignal<Tab>("console");
  const [activeAction, setActiveAction] = createSignal<ActionId>(null);
  const [pipelineTriggeredAt, setPipelineTriggeredAt] = createSignal<
    number | null
  >(null);
  const [installDialogOpen, setInstallDialogOpen] = createSignal(false);
  const [updateDialogOpen, setUpdateDialogOpen] = createSignal(false);
  const [saveTemplateOpen, setSaveTemplateOpen] = createSignal(false);
  const [saveTemplateSubmitting, setSaveTemplateSubmitting] =
    createSignal(false);
  const [killProcessesDialog, setKillProcessesDialog] = createSignal<{
    processes: Array<{ pid: number; command: string; args: string[] }>;
    killing: boolean;
  } | null>(null);

  // ── Tick counter for uptime / shutdown panel ──
  const [tick, setTick] = createSignal(0);
  const tickInterval = setInterval(() => setTick((t) => t + 1), 1000);
  onCleanup(() => clearInterval(tickInterval));

  // ── Hooks ──
  const toast = useToastNotifications();
  const sc = useServerConsole(() => params.id, {
    onReconnect: () => refetch(),
  });
  const updateCheck = useUpdateCheck(serverId, toast.showError);
  const resourceStats = useResourceStats(serverId, tab);

  // ── Refetch when pipeline phase completes ──
  createEffect(() => {
    const progress = sc.phaseProgress();
    if (
      progress &&
      (progress.status === "completed" || progress.status === "failed")
    ) {
      setPipelineTriggeredAt(null);
      refetch();
    }
  });

  // ── Polling fallback: refetch while WS is disconnected during a pipeline ──
  // If the WebSocket drops mid-pipeline, the frontend never receives the
  // PhaseProgress "completed" event — so effectiveInstalled() stays stale and
  // the refetch above never fires.  This effect polls every 2 s whenever the
  // WS is not connected AND the last HTTP-fetched data shows a running
  // pipeline (or we recently triggered one).  The poll stops as soon as the
  // WS reconnects (which triggers its own refetch) or the pipeline finishes.
  {
    let pollTimer: ReturnType<typeof setInterval> | null = null;

    const startPoll = () => {
      if (pollTimer) return;
      pollTimer = setInterval(async () => {
        // Stop polling if WS came back or component is unmounting.
        if (sc.isConnected()) {
          stopPoll();
          return;
        }
        // Call the API directly instead of refetch() — refetch() doesn't
        // return a promise that resolves when data() is updated, so reading
        // data() afterwards may still see stale values.  Using getServer()
        // gives us a real Promise; we then mutate() the resource so the
        // reactive graph updates synchronously.
        try {
          const freshData = await getServer(params.id);
          mutate(freshData);
          const pp = freshData.phase_progress;
          if (!pp || pp.status === "completed" || pp.status === "failed") {
            setPipelineTriggeredAt(null);
          }
        } catch {
          // Network error — keep polling, next tick will retry.
        }
      }, 2000);
    };

    const stopPoll = () => {
      if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
      }
    };

    createEffect(() => {
      const connected = sc.isConnected();
      const phaseFromData = data()?.phase_progress;
      const phaseRunning = phaseFromData?.status === "running";
      const triggeredAt = pipelineTriggeredAt();

      // Also poll when we recently triggered a pipeline (within the last
      // 60 s) — the HTTP-fetched data may not yet show "running" because
      // the backend hasn't picked up the job yet.
      const recentlyTriggered =
        triggeredAt != null && Date.now() - triggeredAt < 60_000;

      if (!connected && (phaseRunning || recentlyTriggered)) {
        startPoll();
      } else {
        stopPoll();
      }
    });

    onCleanup(stopPoll);
  }

  // ── Derived: effective installed state ──
  const effectiveInstalled = (): boolean => {
    const d = data();
    const baseInstalled = d?.server.installed ?? false;
    const pp = sc.phaseProgress();
    if (pp?.status === "completed") {
      if (pp.phase === "install") return true;
      if (pp.phase === "uninstall") return false;
    }
    return baseInstalled;
  };

  // ── Derived: config helpers ──
  const hasInstallSteps = () =>
    (data()?.server.config?.install_steps?.length ?? 0) > 0;
  const hasUpdateSteps = () =>
    (data()?.server.config?.update_steps?.length ?? 0) > 0;
  const hasUninstallSteps = () =>
    (data()?.server.config?.uninstall_steps?.length ?? 0) > 0;
  const hasUpdateCheck = () => !!data()?.server.config?.update_check;
  const hasParameters = () =>
    (data()?.server.config?.parameters?.length ?? 0) > 0;
  const versionParam = () =>
    data()?.server.config?.parameters?.find((p) => p.is_version) ?? null;

  const isPhaseRunning = () => {
    const progress = sc.phaseProgress() ?? data()?.phase_progress;
    return progress?.status === "running";
  };

  // ── 404 redirect ──
  createEffect(() => {
    const err = data.error;
    if (err) {
      const status = (err as any).status;
      const message = (err as any).message || "";
      if (status === 404 || message.toLowerCase().includes("not found")) {
        toast.showError("Server not found. It may have been deleted.");
        setTimeout(() => navigate("/", { replace: true }), 2000);
      }
    }
  });

  // ── Generic action runner ──
  const doAction = async (
    id: ActionId,
    label: string,
    action: () => Promise<unknown>,
    successMsg?: string,
  ) => {
    if (activeAction()) return;
    setActiveAction(id);
    toast.dismissError();
    toast.dismissSuccess();
    try {
      await action();
      if (successMsg) toast.showSuccess(successMsg);
      refetch();
    } catch (e: any) {
      toast.showError(`${label} failed: ${e.message || e}`);
    } finally {
      setActiveAction(null);
    }
  };

  const doControl = (
    id: ActionId,
    label: string,
    action: () => Promise<unknown>,
  ) => doAction(id, label, action);

  const doPipeline = (
    id: ActionId,
    label: string,
    action: () => Promise<unknown>,
  ) => {
    setPipelineTriggeredAt(Date.now());
    return doAction(id, label, action, `${label} started`);
  };

  // ── Action handlers ──

  const handleStart = () => {
    const d = data();
    if (
      d &&
      !effectiveInstalled() &&
      (d.server.config.install_steps?.length ?? 0) > 0
    ) {
      setInstallDialogOpen(true);
      return;
    }
    doControl("start", "Start", () => startServer(params.id));
  };

  const handleInstallDialogInstall = () => {
    setInstallDialogOpen(false);
    doPipeline("install", "Install", () => installServer(params.id));
  };

  const handleInstallDialogMarkInstalled = async () => {
    setInstallDialogOpen(false);
    try {
      await markInstalled(params.id);
      toast.showSuccess("Server marked as installed");
      refetch();
    } catch (e: any) {
      toast.showError(`Mark as installed failed: ${e.message || e}`);
    }
  };

  const handleStop = () =>
    doControl("stop", "Stop", () => stopServer(params.id));
  const handleRestart = () =>
    doControl("restart", "Restart", () => restartServer(params.id));

  const handleKill = () => {
    if (
      !confirm(
        "Are you sure you want to kill this server? This will immediately terminate the process with no grace period.",
      )
    )
      return;
    if (activeAction() === "stop") setActiveAction(null);
    doControl("kill", "Kill", () => killServer(params.id));
  };

  const handleCancelStop = () =>
    doControl("cancel-stop" as ActionId, "Cancel Shutdown", () =>
      cancelStop(params.id),
    );

  const handleInstall = () =>
    doPipeline("install", "Install", () => installServer(params.id));

  const handleUpdate = () => setUpdateDialogOpen(true);

  const handleUpdateDialogConfirm = (versionOverride: string | null) => {
    setUpdateDialogOpen(false);
    const req =
      versionOverride != null
        ? {
            steps_override: null,
            parameter_overrides: {
              [versionParam()?.name ?? ""]: versionOverride,
            },
          }
        : null;
    doPipeline("update", "Update", () => updateServerPipeline(params.id, req));
  };

  const handleUninstall = () =>
    doPipeline("uninstall", "Uninstall", () => uninstallServer(params.id));
  const handleCancelPhase = () =>
    doPipeline("cancel", "Cancel pipeline", () => cancelPhase(params.id));

  const handleDelete = async () => {
    if (
      !confirm(
        "Are you sure you want to delete this server? This will permanently remove the server and all of its files. This cannot be undone.",
      )
    )
      return;
    setActiveAction("delete");
    toast.dismissError();
    try {
      await deleteServer(params.id);
      navigate("/", { replace: true });
    } catch (e: any) {
      toast.showError(`Delete failed: ${e.message || e}`);
      setActiveAction(null);
    }
  };

  const handleReset = async () => {
    if (
      !confirm(
        "Are you sure you want to reset this server? This will stop the server, kill any orphaned processes, delete ALL server files, and mark it as uninstalled. You will need to re-run the install pipeline afterwards.",
      )
    )
      return;
    setActiveAction("reset");
    toast.dismissError();
    toast.dismissSuccess();
    try {
      const result = await resetServer(params.id);
      const extra =
        result.killed_processes > 0
          ? ` (killed ${result.killed_processes} orphaned process${result.killed_processes > 1 ? "es" : ""})`
          : "";
      toast.showSuccess(
        `Server reset successfully${extra}. All files removed.`,
      );
      await refetch();
    } catch (e: any) {
      toast.showError(`Reset failed: ${e.message || e}`);
    } finally {
      setActiveAction(null);
    }
  };

  const handleKillProcesses = async () => {
    setActiveAction("kill-processes");
    toast.dismissError();
    toast.dismissSuccess();
    try {
      const result = await listDirectoryProcesses(params.id);
      if (result.count === 0) {
        toast.showSuccess(
          "No processes found running in the server directory.",
        );
        setActiveAction(null);
        return;
      }
      setKillProcessesDialog({ processes: result.processes, killing: false });
    } catch (e: any) {
      toast.showError(`Failed to list directory processes: ${e.message || e}`);
      setActiveAction(null);
    }
  };

  const handleKillProcessesConfirm = async () => {
    setKillProcessesDialog((prev) =>
      prev ? { ...prev, killing: true } : prev,
    );
    try {
      const result = await killDirectoryProcesses(params.id);
      setKillProcessesDialog(null);
      if (result.killed === 0 && result.failed === 0) {
        toast.showSuccess(
          "No processes found running in the server directory.",
        );
      } else {
        const succeeded = result.processes.filter((p) => p.success);
        const failed = result.processes.filter((p) => !p.success);
        let msg = `Killed ${result.killed} process${result.killed !== 1 ? "es" : ""}`;
        if (succeeded.length > 0) {
          msg += `: ${succeeded.map((p) => `${p.command} (PID ${p.pid})`).join(", ")}`;
        }
        if (failed.length > 0) {
          msg += `. Failed to kill: ${failed.map((p) => `${p.command} (PID ${p.pid})`).join(", ")}`;
        }
        toast.showSuccess(msg);
      }
    } catch (e: any) {
      setKillProcessesDialog(null);
      toast.showError(`Kill processes failed: ${e.message || e}`);
    } finally {
      setActiveAction(null);
    }
  };

  const handleKillProcessesCancel = () => {
    setKillProcessesDialog(null);
    setActiveAction(null);
  };

  const handleConfigSave = async (config: ServerConfig) => {
    toast.dismissError();
    try {
      const current = data();
      const paramVals = current?.server.parameter_values ?? {};
      await updateServer(params.id, { config, parameter_values: paramVals });
      await refetch();
      toast.showSuccess("Configuration saved");
    } catch (e: any) {
      toast.showError(`Config save failed: ${e.message || e}`);
    }
  };

  const handleParamSave = async (values: Record<string, string>) => {
    toast.dismissError();
    try {
      const current = data();
      if (!current) return;
      await updateServer(params.id, {
        config: current.server.config,
        parameter_values: values,
      });
      await refetch();
      toast.showSuccess("Parameters saved");
    } catch (e: any) {
      toast.showError(`Parameter save failed: ${e.message || e}`);
    }
  };

  const handleSaveAsTemplate = async (name: string, description: string) => {
    const current = data();
    if (!current) return;
    setSaveTemplateSubmitting(true);
    try {
      await createTemplate({
        name,
        description: description || null,
        config: current.server.config,
      });
      setSaveTemplateOpen(false);
      toast.showSuccess(`Template "${name}" saved`);
    } catch (e: any) {
      toast.showError(`Save template failed: ${e.message || e}`);
    } finally {
      setSaveTemplateSubmitting(false);
    }
  };

  let saveTemplateFormRef: HTMLDivElement | undefined;

  // ─── Render ───────────────────────────────────────────────────────────────

  return (
    <div class="server-detail">
      <Switch>
        <Match when={data.error}>
          <div class="error-msg error not-found" role="alert">
            {(data.error as any)?.status === 404
              ? "Server not found. Redirecting to dashboard..."
              : `Failed to load server: ${String((data.error as any)?.message ?? data.error)}`}
          </div>
        </Match>

        <Match when={data.loading && !data()}>
          <Loader message="Loading server" />
        </Match>

        <Match when={data()}>
          {(server) => {
            const status = () => (sc.runtime() ?? server().runtime).status;
            const runtime = () => sc.runtime() ?? server().runtime;

            return (
              <>
                <ConnectionBanner
                  state={sc.connectionState()}
                  reconnectInfo={sc.reconnectInfo()}
                />

                {/* ── Page header ── */}
                <div class="page-header">
                  <div>
                    <h1>{server().server.config.name}</h1>
                    <div class="server-detail-meta">
                      <span class={`status-badge status-${status()}`}>
                        {status()}
                      </span>
                      <StatusTooltip status={status()} />
                      <span
                        class="permission-badge"
                        style={{
                          color: LEVEL_COLORS[perm()?.level ?? "viewer"],
                        }}
                        title={LEVEL_DESCRIPTIONS[perm()?.level ?? "viewer"]}
                      >
                        {(perm()?.level ?? "viewer").charAt(0).toUpperCase() +
                          (perm()?.level ?? "viewer").slice(1)}
                        {perm()?.is_global_admin ? " (Global Admin)" : ""}
                      </span>
                      <Show when={runtime().pid}>
                        {(pid) => <span class="pid">PID: {pid()}</span>}
                      </Show>
                      <Show when={isRunning(status()) && runtime().started_at}>
                        <span class="pid">
                          Uptime: {formatUptime(runtime().started_at, tick())}
                        </span>
                      </Show>
                      <Show when={runtime().restart_count > 0}>
                        <span class="pid">
                          Restarts: {runtime().restart_count}
                        </span>
                      </Show>
                    </div>
                  </div>

                  {/* ── Action buttons ── */}
                  <div class="actions">
                    <Show when={hasInstallSteps()}>
                      <button
                        class="btn"
                        classList={{ "btn-forbidden": !canRunPipelines() }}
                        onClick={handleInstall}
                        disabled={
                          !!activeAction() ||
                          isRunning(status()) ||
                          isPhaseRunning() ||
                          !canRunPipelines()
                        }
                        title={
                          !canRunPipelines()
                            ? "You need Manager permissions or higher to run pipelines"
                            : effectiveInstalled()
                              ? "Re-run install pipeline"
                              : "Run install pipeline"
                        }
                      >
                        {activeAction() === "install" ? (
                          <span class="btn-spinner" />
                        ) : (
                          "📦"
                        )}{" "}
                        {effectiveInstalled() ? "Reinstall" : "Install"}
                      </button>
                    </Show>
                    <Show when={hasUpdateSteps()}>
                      <button
                        class="btn"
                        classList={{ "btn-forbidden": !canRunPipelines() }}
                        onClick={handleUpdate}
                        disabled={
                          !!activeAction() ||
                          isRunning(status()) ||
                          isPhaseRunning() ||
                          !canRunPipelines()
                        }
                        title={
                          !canRunPipelines()
                            ? "You need Manager permissions or higher to run pipelines"
                            : "Run update pipeline"
                        }
                      >
                        {activeAction() === "update" ? (
                          <span class="btn-spinner" />
                        ) : (
                          "🔄"
                        )}{" "}
                        Update
                      </button>
                    </Show>
                    <Show when={updateDialogOpen()}>
                      <UpdateDialog
                        versionParam={versionParam()}
                        parameterValues={server().server.parameter_values ?? {}}
                        updateCheckResult={updateCheck.updateCheckResult()}
                        installedVersion={
                          server().server.installed_version ?? null
                        }
                        busy={activeAction() === "update"}
                        onConfirm={handleUpdateDialogConfirm}
                        onCancel={() => setUpdateDialogOpen(false)}
                      />
                    </Show>
                    <Show when={hasUninstallSteps() && effectiveInstalled()}>
                      <button
                        class="btn btn-danger-outline"
                        classList={{ "btn-forbidden": !canRunPipelines() }}
                        onClick={handleUninstall}
                        disabled={
                          !!activeAction() ||
                          isRunning(status()) ||
                          isPhaseRunning() ||
                          !canRunPipelines()
                        }
                        title={
                          !canRunPipelines()
                            ? "You need Manager permissions or higher to run pipelines"
                            : "Run uninstall pipeline"
                        }
                      >
                        {activeAction() === "uninstall" ? (
                          <span class="btn-spinner" />
                        ) : (
                          "🗑"
                        )}{" "}
                        Uninstall
                      </button>
                    </Show>
                    <Show when={isPhaseRunning()}>
                      <button
                        class="btn btn-warning"
                        onClick={handleCancelPhase}
                        disabled={!!activeAction()}
                      >
                        {activeAction() === "cancel" ? (
                          <span class="btn-spinner" />
                        ) : (
                          "✕"
                        )}{" "}
                        Cancel Pipeline
                      </button>
                    </Show>
                    <button
                      class="btn btn-success"
                      classList={{ "btn-forbidden": !canControl() }}
                      onClick={handleStart}
                      disabled={
                        !!activeAction() || !canStart(status()) || !canControl()
                      }
                      title={
                        !canControl()
                          ? "You need Operator permissions or higher to start this server"
                          : undefined
                      }
                    >
                      {activeAction() === "start" ? (
                        <span class="btn-spinner" />
                      ) : (
                        "▶"
                      )}{" "}
                      Start
                    </button>
                    <button
                      class="btn btn-warning"
                      classList={{ "btn-forbidden": !canControl() }}
                      onClick={handleRestart}
                      disabled={
                        !!activeAction() ||
                        !canRestart(status()) ||
                        !canControl()
                      }
                      title={
                        !canControl()
                          ? "You need Operator permissions or higher to restart this server"
                          : undefined
                      }
                    >
                      {activeAction() === "restart" ? (
                        <span class="btn-spinner" />
                      ) : (
                        "↻"
                      )}{" "}
                      Restart
                    </button>
                    <button
                      class="btn btn-danger"
                      classList={{ "btn-forbidden": !canControl() }}
                      onClick={handleStop}
                      disabled={
                        !!activeAction() || !canStop(status()) || !canControl()
                      }
                      title={
                        !canControl()
                          ? "You need Operator permissions or higher to stop this server"
                          : undefined
                      }
                    >
                      {activeAction() === "stop" ? (
                        <span class="btn-spinner" />
                      ) : (
                        "■"
                      )}{" "}
                      Stop
                    </button>
                    <button
                      class="btn btn-danger"
                      classList={{ "btn-forbidden": !canControl() }}
                      onClick={handleKill}
                      disabled={
                        (!!activeAction() && activeAction() !== "stop") ||
                        !canKill(status()) ||
                        !canControl()
                      }
                      title={
                        !canControl()
                          ? "You need Operator permissions or higher to kill this server"
                          : "Immediately SIGKILL — no grace period"
                      }
                    >
                      {activeAction() === "kill" ? (
                        <span class="btn-spinner" />
                      ) : (
                        "⚡"
                      )}{" "}
                      Kill
                    </button>
                  </div>
                </div>

                {/* ── Install dialog ── */}
                <Show when={installDialogOpen()}>
                  <InstallDialog
                    canMarkInstalled={
                      server().permission.level === "admin" ||
                      server().permission.level === "owner" ||
                      server().permission.is_global_admin
                    }
                    onInstall={handleInstallDialogInstall}
                    onMarkInstalled={handleInstallDialogMarkInstalled}
                    onCancel={() => setInstallDialogOpen(false)}
                  />
                </Show>

                {/* ── Shutdown progress ── */}
                <Show when={status() === "stopping" && sc.stopProgress()}>
                  {(entry) => (
                    <ShutdownPanel
                      entry={entry()}
                      tick={tick()}
                      cancellingStop={activeAction() === "cancel-stop"}
                      onCancelStop={handleCancelStop}
                    />
                  )}
                </Show>

                {/* ── Restart banner ── */}
                <Show
                  when={
                    runtime().status === "crashed" && runtime().next_restart_at
                  }
                >
                  <RestartBanner
                    serverId={serverId()}
                    runtime={runtime()}
                    maxAttempts={server().server.config.max_restart_attempts}
                    restartDelaySecs={server().server.config.restart_delay_secs}
                    onCancelled={() => refetch()}
                    compact={false}
                  />
                </Show>

                {/* ── Inline toast messages ── */}
                <Show when={toast.actionSuccess()}>
                  {(msg) => (
                    <div
                      ref={toast.setSuccessInlineRef}
                      class="success-msg animate-slide-in server-toast-inline"
                      onMouseEnter={toast.pauseSuccessTimer}
                      onMouseLeave={toast.resumeSuccessTimer}
                    >
                      <span>{msg()}</span>
                      <button
                        class="dismiss-btn"
                        onClick={toast.dismissSuccess}
                        aria-label="Dismiss"
                      >
                        ✕
                      </button>
                      <div
                        class="toast-timer-bar toast-timer-bar--success"
                        style={{
                          "animation-duration": `${toast.successTimerDuration()}ms`,
                        }}
                        data-key={toast.successTimerKey()}
                      />
                    </div>
                  )}
                </Show>

                {/* ── Floating toasts (when inline is off-screen) ── */}
                <Show when={toast.actionError() && !toast.errorVisible()}>
                  <FloatingToast
                    message={toast.actionError()!}
                    type="error"
                    timerDuration={toast.errorTimerDuration()}
                    timerKey={toast.errorTimerKey()}
                    onDismiss={toast.dismissError}
                    onPause={toast.pauseErrorTimer}
                    onResume={toast.resumeErrorTimer}
                    onScrollTo={() =>
                      toast.errorInlineRef()?.scrollIntoView({
                        behavior: "smooth",
                        block: "center",
                      })
                    }
                  />
                </Show>
                <Show when={toast.actionSuccess() && !toast.successVisible()}>
                  <FloatingToast
                    message={toast.actionSuccess()!}
                    type="success"
                    timerDuration={toast.successTimerDuration()}
                    timerKey={toast.successTimerKey()}
                    onDismiss={toast.dismissSuccess}
                    onPause={toast.pauseSuccessTimer}
                    onResume={toast.resumeSuccessTimer}
                    onScrollTo={() =>
                      toast.successInlineRef()?.scrollIntoView({
                        behavior: "smooth",
                        block: "center",
                      })
                    }
                  />
                </Show>

                {/* ── Update check banner ── */}
                <Show when={hasUpdateCheck()}>
                  <UpdateCheckBanner
                    result={updateCheck.updateCheckResult()}
                    checking={updateCheck.updateChecking()}
                    onCheck={(force) => updateCheck.handleCheckForUpdate(force)}
                  />
                </Show>

                {/* ── Inline error toast ── */}
                <Show when={toast.actionError()}>
                  {(err) => (
                    <div
                      ref={toast.setErrorInlineRef}
                      class="error-msg animate-slide-in server-toast-inline"
                      onMouseEnter={toast.pauseErrorTimer}
                      onMouseLeave={toast.resumeErrorTimer}
                    >
                      <span>{err()}</span>
                      <button
                        class="dismiss-btn"
                        onClick={toast.dismissError}
                        aria-label="Dismiss"
                      >
                        ✕
                      </button>
                      <div
                        class="toast-timer-bar toast-timer-bar--error"
                        style={{
                          "animation-duration": `${toast.errorTimerDuration()}ms`,
                        }}
                        data-key={toast.errorTimerKey()}
                      />
                    </div>
                  )}
                </Show>

                {/* ── Save as template dialog ── */}
                <Show when={saveTemplateOpen()}>
                  <div ref={saveTemplateFormRef}>
                    <SaveTemplateDialog
                      submitting={saveTemplateSubmitting()}
                      onSave={handleSaveAsTemplate}
                      onCancel={() => setSaveTemplateOpen(false)}
                    />
                  </div>
                </Show>

                {/* ── Tab bar ── */}
                <div class="tabs">
                  <button
                    class={`tab ${tab() === "console" ? "active" : ""}`}
                    onClick={() => setTab("console")}
                  >
                    Console
                  </button>
                  <button
                    class={`tab ${tab() === "files" ? "active" : ""}`}
                    onClick={() => setTab("files")}
                  >
                    Files
                  </button>
                  <button
                    class={`tab ${tab() === "pipelines" ? "active" : ""}`}
                    onClick={() => setTab("pipelines")}
                  >
                    Pipelines
                  </button>
                  <button
                    class={`tab ${tab() === "resources" ? "active" : ""}`}
                    onClick={() => setTab("resources")}
                  >
                    Resources
                  </button>
                  <button
                    class={`tab ${tab() === "sandbox" ? "active" : ""}`}
                    onClick={() => setTab("sandbox")}
                  >
                    🛡️ Sandbox
                  </button>
                  <Show
                    when={
                      server().permission.level === "admin" ||
                      server().permission.level === "owner" ||
                      server().permission.is_global_admin
                    }
                  >
                    <button
                      class={`tab ${tab() === "access" ? "active" : ""}`}
                      onClick={() => setTab("access")}
                    >
                      🔑 Access
                    </button>
                  </Show>
                  <button
                    class={`tab ${tab() === "config" ? "active" : ""}`}
                    onClick={() => setTab("config")}
                  >
                    Configuration
                  </button>
                </div>

                {/* ── Tab content ── */}
                <div class="tab-content">
                  {/* Console stays mounted (hidden) for scroll position preservation */}
                  <div
                    style={{
                      display: tab() === "console" ? undefined : "none",
                    }}
                  >
                    <Console
                      serverId={params.id}
                      lines={sc.lines()}
                      connectionState={sc.connectionState()}
                      serverStatus={sc.serverStatus()}
                      phaseProgress={sc.phaseProgress()}
                      onClear={sc.clearLines}
                    />
                  </div>

                  <Switch>
                    <Match when={tab() === "sandbox"}>
                      <div class="tab-content-padded">
                        <SandboxConfig serverId={params.id} />
                      </div>
                    </Match>

                    <Match when={tab() === "access"}>
                      <ServerAccessManager
                        serverId={params.id}
                        myLevel={server().permission.level}
                        isGlobalAdmin={server().permission.is_global_admin}
                        ownerId={server().server.owner_id}
                      />
                    </Match>

                    <Match when={tab() === "resources"}>
                      <ResourcesTab stats={resourceStats.resourceStats()} />
                    </Match>

                    <Match when={tab() === "files"}>
                      <FileManager serverId={params.id} />
                    </Match>

                    <Match when={tab() === "pipelines"}>
                      <ServerPipelinesTab
                        serverId={params.id}
                        config={server().server.config}
                        parameterValues={server().server.parameter_values ?? {}}
                        serverDir={(server().server as any).server_dir}
                        onRefetch={refetch}
                        onError={toast.showError}
                        onSuccess={toast.showSuccess}
                        onSaveAsTemplate={() => {
                          setSaveTemplateOpen(true);
                          requestAnimationFrame(() => {
                            saveTemplateFormRef?.scrollIntoView({
                              behavior: "smooth",
                              block: "nearest",
                            });
                          });
                        }}
                      />
                    </Match>

                    <Match when={tab() === "config"}>
                      <ServerConfigTab
                        serverId={server().server.id}
                        config={server().server.config}
                        parameterValues={server().server.parameter_values ?? {}}
                        permission={server().permission}
                        actionBusy={!!activeAction()}
                        activeAction={activeAction()}
                        isGlobalAdmin={auth.isAdmin()}
                        onConfigSave={handleConfigSave}
                        onParamSave={handleParamSave}
                        onKillProcesses={handleKillProcesses}
                        onReset={handleReset}
                        onDelete={handleDelete}
                      />
                    </Match>
                  </Switch>
                </div>
              </>
            );
          }}
        </Match>
      </Switch>

      {/* ── Kill processes dialog (rendered outside main flow) ── */}
      <Show when={killProcessesDialog()}>
        {(dialog) => (
          <KillProcessesDialog
            processes={dialog().processes}
            killing={dialog().killing}
            onConfirm={handleKillProcessesConfirm}
            onCancel={handleKillProcessesCancel}
          />
        )}
      </Show>
    </div>
  );
};

export default ServerDetail;
