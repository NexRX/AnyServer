import {
  type Component,
  createSignal,
  For,
  Show,
  createMemo,
  onMount,
} from "solid-js";
import Loader from "../components/Loader";
import MarkdownRenderer from "../components/MarkdownRenderer";
import SearchableSelect from "../components/SearchableSelect";
import { A, useNavigate } from "@solidjs/router";
import { createServer, listTemplates } from "../api/client";
import { fetchOptions } from "../api/templates";
import { fetchGithubReleases } from "../api/github";
import { fetchCurseForgeFiles } from "../api/curseforge";
import JavaRuntimeSelector, {
  isJavaBinary,
} from "../components/JavaRuntimeSelector";
import DotnetRuntimeSelector, {
  isDotnetBinary,
} from "../components/DotnetRuntimeSelector";
import WizardCreateServer from "../components/WizardCreateServer";
import { useAuth } from "../context/auth";
import { useIntegrationStatus } from "../context/integrations";
import type {
  ServerConfig,
  ConfigParameter,
  FetchedOption,
  ServerTemplate,
} from "../types/bindings";

type CreationMode = "wizard" | "template";

const defaultConfig: ServerConfig = {
  name: "",
  binary: "",
  args: [],
  env: {},
  working_dir: null,
  auto_start: false,
  auto_restart: false,
  max_restart_attempts: 0,
  restart_delay_secs: 5,
  stop_command: null,
  stop_signal: "sigterm",
  stop_timeout_secs: 10,
  stop_steps: [],
  sftp_username: null,
  sftp_password: null,
  parameters: [],
  start_steps: [],
  install_steps: [],
  update_steps: [],
  uninstall_steps: [],
  isolation: {
    enabled: true,
    extra_read_paths: [],
    extra_rw_paths: [],
    pids_max: null,
  },
  update_check: null,
  log_to_disk: true,
  max_log_size_mb: 50,
  enable_java_helper: false,
  enable_dotnet_helper: false,
  steam_app_id: null,
};

const CreateServer: Component = () => {
  const navigate = useNavigate();
  const auth = useAuth();
  const integrations = useIntegrationStatus();
  const [mode, setMode] = createSignal<CreationMode>("template");
  const [config, setConfig] = createSignal<ServerConfig>({ ...defaultConfig });
  const [parameterValues, setParameterValues] = createSignal<
    Record<string, string>
  >({});
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  const [steamcmdAvailable, setSteamcmdAvailable] = createSignal(true);

  // Dynamic options state (for options_from)
  const [dynamicOptions, setDynamicOptions] = createSignal<
    Record<string, FetchedOption[]>
  >({});
  const [loadingOptions, setLoadingOptions] = createSignal<
    Record<string, boolean>
  >({});
  const [optionErrors, setOptionErrors] = createSignal<Record<string, string>>(
    {},
  );
  // Simple client-side cache: key → { options, fetchedAt, cacheSecs }
  const optionsCacheMap = new Map<
    string,
    { options: FetchedOption[]; fetchedAt: number; cacheSecs: number }
  >();

  // Template state
  const [templates, setTemplates] = createSignal<ServerTemplate[]>([]);
  const [templatesLoading, setTemplatesLoading] = createSignal(false);
  const [selectedTemplate, setSelectedTemplate] =
    createSignal<ServerTemplate | null>(null);
  // Steam app ID validation state
  const [steamAppName, setSteamAppName] = createSignal<string | null>(null);
  const [steamAppValidating, setSteamAppValidating] = createSignal(false);
  const [steamAppError, setSteamAppError] = createSignal<string | null>(null);

  // Check if a template was passed via sessionStorage (from Templates page "Use Template")
  onMount(() => {
    const stored = sessionStorage.getItem("anyserver_template");
    if (stored) {
      sessionStorage.removeItem("anyserver_template");
      try {
        const template: ServerTemplate = JSON.parse(stored);
        setMode("template");
        applyTemplate(template);
      } catch {
        // ignore invalid
      }
    }

    if (mode() === "template") {
      loadTemplates();
    }
  });

  const loadTemplates = async () => {
    if (templates().length > 0) return; // already loaded
    setTemplatesLoading(true);
    try {
      const res = await listTemplates();
      setTemplates(res.templates);
      setSteamcmdAvailable(res.steamcmd_available);
    } catch (e: any) {
      setError(`Failed to load templates: ${e.message || e}`);
    } finally {
      setTemplatesLoading(false);
    }
  };

  const validateSteamAppId = async (appId: number | null) => {
    setSteamAppName(null);
    setSteamAppError(null);
    if (appId == null || appId === 0) return;
    setSteamAppValidating(true);
    try {
      const { validateSteamApp } = await import("../api/client");
      const resp = await validateSteamApp(appId);
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

  const applyTemplate = (template: ServerTemplate) => {
    setSelectedTemplate(template);
    const merged: ServerConfig = {
      ...defaultConfig,
      ...template.config,
      args: Array.isArray(template.config.args)
        ? template.config.args
        : defaultConfig.args,
      env:
        typeof template.config.env === "object" && template.config.env !== null
          ? template.config.env
          : defaultConfig.env,
      parameters: Array.isArray(template.config.parameters)
        ? template.config.parameters
        : defaultConfig.parameters,
      start_steps: Array.isArray(template.config.start_steps)
        ? template.config.start_steps
        : defaultConfig.start_steps,
      install_steps: Array.isArray(template.config.install_steps)
        ? template.config.install_steps
        : defaultConfig.install_steps,
      update_steps: Array.isArray(template.config.update_steps)
        ? template.config.update_steps
        : defaultConfig.update_steps,
      uninstall_steps: Array.isArray(template.config.uninstall_steps)
        ? template.config.uninstall_steps
        : defaultConfig.uninstall_steps,
    };
    setConfig(merged);

    // Seed parameter values from defaults
    const vals: Record<string, string> = {};
    for (const param of merged.parameters ?? []) {
      if (param.default != null) {
        vals[param.name] = param.default;
      }
    }
    setParameterValues(vals);
    setError(null);
  };

  const hasParameters = createMemo(
    () => (config().parameters?.length ?? 0) > 0,
  );

  const validate = (c: ServerConfig): string | null => {
    if (!c.name.trim()) {
      return "Server name is required.";
    }
    if (!c.binary.trim()) {
      return "Binary path is required.";
    }
    if (c.restart_delay_secs < 0) {
      return "Restart delay must be non-negative.";
    }
    if (c.stop_timeout_secs < 0) {
      return "Stop timeout must be non-negative.";
    }
    if (c.max_restart_attempts < 0) {
      return "Max restart attempts must be non-negative.";
    }
    if (c.sftp_username && !c.sftp_password) {
      return "SFTP password is required when a username is set.";
    }

    // Validate required parameters
    const vals = parameterValues();
    for (const param of c.parameters ?? []) {
      if (param.required) {
        const val = vals[param.name];
        if (!val || val.trim() === "") {
          return `Parameter "${param.label}" is required.`;
        }
      }
      // Validate select options (accept both static and dynamically loaded values)
      if (param.param_type === "select" && param.options.length > 0) {
        const val = vals[param.name];
        const dynOpts = dynamicOptions()[param.name] ?? [];
        const allValid = new Set([
          ...param.options,
          ...dynOpts.map((o) => o.value),
        ]);
        if (val && val.trim() !== "" && !allValid.has(val)) {
          return `Parameter "${param.label}" must be one of the available options.`;
        }
      }
    }

    return null;
  };

  const handleParamChange = (name: string, value: string) => {
    setParameterValues((prev) => ({ ...prev, [name]: value }));
    if (error()) {
      setError(null);
    }
  };

  const handleSubmit = async () => {
    const c = config();
    const validationError = validate(c);
    if (validationError) {
      setError(validationError);
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const tmpl = selectedTemplate();
      const result = await createServer({
        config: c,
        parameter_values: parameterValues(),
        source_template_id: tmpl ? tmpl.id : null,
      });
      navigate(`/server/${result.server.id}`);
    } catch (e: unknown) {
      if (e instanceof Error) {
        setError(e.message);
      } else {
        setError("An unexpected error occurred while creating the server.");
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleModeSwitch = (newMode: CreationMode) => {
    if (newMode === mode()) return;
    setMode(newMode);
    setError(null);
    if (newMode === "template") {
      loadTemplates();
    }
    if (newMode !== "template") {
      setSelectedTemplate(null);
    }
  };

  const buildSubstitutionParams = (
    excludeName: string,
  ): Record<string, string> => {
    const vals = parameterValues();
    const result: Record<string, string> = {};
    for (const p of config().parameters ?? []) {
      if (p.name !== excludeName) {
        const v = vals[p.name] ?? p.default ?? "";
        if (v) result[p.name] = v;
      }
    }
    return result;
  };

  const handleLoadOptions = async (param: ConfigParameter) => {
    if (!param.options_from) return;
    const of = param.options_from;
    const subs = buildSubstitutionParams(param.name);
    const cacheKey = JSON.stringify({
      url: of.url,
      path: of.path ?? null,
      params: subs,
    });

    // Check client-side cache.
    const cached = optionsCacheMap.get(cacheKey);
    if (cached) {
      const ageSecs = (Date.now() - cached.fetchedAt) / 1000;
      if (ageSecs < cached.cacheSecs) {
        setDynamicOptions((prev) => ({
          ...prev,
          [param.name]: cached.options,
        }));
        setOptionErrors((prev) => {
          const n = { ...prev };
          delete n[param.name];
          return n;
        });
        return;
      }
      optionsCacheMap.delete(cacheKey);
    }

    setLoadingOptions((prev) => ({ ...prev, [param.name]: true }));
    setOptionErrors((prev) => {
      const n = { ...prev };
      delete n[param.name];
      return n;
    });

    try {
      const resp = await fetchOptions({
        url: of.url,
        path: of.path,
        value_key: of.value_key,
        label_key: of.label_key,
        sort: of.sort,
        limit: of.limit,
        params: subs,
      });
      setDynamicOptions((prev) => ({ ...prev, [param.name]: resp.options }));
      const cacheSecs = of.cache_secs ?? 0;
      if (cacheSecs > 0) {
        optionsCacheMap.set(cacheKey, {
          options: resp.options,
          fetchedAt: Date.now(),
          cacheSecs,
        });
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : "Failed to load options";
      setOptionErrors((prev) => ({ ...prev, [param.name]: msg }));
    } finally {
      setLoadingOptions((prev) => ({ ...prev, [param.name]: false }));
    }
  };

  const getMergedOptions = (param: ConfigParameter): FetchedOption[] => {
    const staticOpts: FetchedOption[] = param.options.map((o) => ({
      value: o,
      label: o,
    }));
    const dynamic = dynamicOptions()[param.name];
    if (!dynamic || dynamic.length === 0) return staticOpts;
    if (staticOpts.length > 0) {
      const seen = new Set(staticOpts.map((o) => o.value));
      return [...staticOpts, ...dynamic.filter((d) => !seen.has(d.value))];
    }
    return dynamic;
  };

  const renderParameterInput = (param: ConfigParameter) => {
    const value = () => parameterValues()[param.name] ?? param.default ?? "";
    const hasOptionsFrom = () => param.options_from != null;
    const isLoadingOpt = () => loadingOptions()[param.name] ?? false;
    const optError = () => optionErrors()[param.name];
    const hasDynamic = () => (dynamicOptions()[param.name]?.length ?? 0) > 0;

    switch (param.param_type) {
      case "github_release_tag":
        return (
          <div class="form-group">
            <label>
              {param.label}
              {param.required && " *"}
            </label>
            {/* ─── GitHub token not configured — soft info, not a blocker ─── */}
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
                  "margin-bottom": "0.5rem",
                  "font-size": "0.82rem",
                  color: "#fde68a",
                }}
              >
                <span style={{ "flex-shrink": "0" }}>ℹ️</span>
                <div>
                  <strong>No GitHub token configured.</strong> Public repos work
                  fine — private repos and higher rate limits require a token.
                  <Show when={auth.isAdmin()}>
                    {" "}
                    <A
                      href="/admin"
                      style={{
                        color: "#facc15",
                        "text-decoration": "underline",
                      }}
                      onClick={() =>
                        sessionStorage.setItem("admin_tab", "github")
                      }
                    >
                      Configure →
                    </A>
                  </Show>
                </div>
              </div>
            </Show>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.5rem",
              }}
            >
              <div style={{ flex: "1", position: "relative" }}>
                <input
                  type="text"
                  value={value()}
                  onInput={(e) =>
                    handleParamChange(param.name, e.currentTarget.value)
                  }
                  list={`${param.name}-releases`}
                  placeholder={
                    hasDynamic()
                      ? "Type to search releases..."
                      : "Click 'Load releases' to see options"
                  }
                  style={{
                    width: "100%",
                    "padding-right": hasDynamic() ? "2rem" : undefined,
                  }}
                />
                <Show when={hasDynamic()}>
                  <span
                    style={{
                      position: "absolute",
                      right: "0.75rem",
                      top: "50%",
                      transform: "translateY(-50%)",
                      "pointer-events": "none",
                      color: "#6b7280",
                      "font-size": "0.875rem",
                    }}
                  >
                    {getMergedOptions(param).length} releases
                  </span>
                </Show>
                <datalist id={`${param.name}-releases`}>
                  <For each={getMergedOptions(param)}>
                    {(opt) => (
                      <option value={opt.value}>
                        {opt.label !== opt.value ? opt.label : undefined}
                      </option>
                    )}
                  </For>
                </datalist>
              </div>
              <button
                class="btn btn-sm"
                onClick={async () => {
                  if (!param.github_repo) {
                    setOptionErrors({
                      ...optionErrors(),
                      [param.name]: "No GitHub repository configured",
                    });
                    return;
                  }
                  setLoadingOptions({
                    ...loadingOptions(),
                    [param.name]: true,
                  });
                  setOptionErrors((prev) => {
                    const n = { ...prev };
                    delete n[param.name];
                    return n;
                  });
                  try {
                    const resp = await fetchGithubReleases(param.github_repo);
                    const options = resp.releases.map(
                      (r: { name: string; title: string }) => ({
                        value: r.name,
                        label:
                          r.title !== r.name
                            ? `${r.title} (${r.name})`
                            : r.name,
                      }),
                    );
                    setDynamicOptions({
                      ...dynamicOptions(),
                      [param.name]: options,
                    });
                    // Auto-select first option if nothing selected
                    if (!value() && options.length > 0) {
                      handleParamChange(param.name, options[0].value);
                    }
                  } catch (err) {
                    const msg =
                      err instanceof Error ? err.message : String(err);
                    setOptionErrors({ ...optionErrors(), [param.name]: msg });
                  } finally {
                    setLoadingOptions({
                      ...loadingOptions(),
                      [param.name]: false,
                    });
                  }
                }}
                disabled={isLoadingOpt()}
                title="Fetch release tags from GitHub"
                style={{
                  "white-space": "nowrap",
                  "min-width": "fit-content",
                  background: hasDynamic()
                    ? "linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%)"
                    : undefined,
                  border: hasDynamic() ? "none" : undefined,
                  color: hasDynamic() ? "#fff" : undefined,
                  "box-shadow": hasDynamic()
                    ? "0 2px 8px rgba(99, 102, 241, 0.3)"
                    : undefined,
                }}
              >
                {isLoadingOpt()
                  ? "Loading…"
                  : hasDynamic()
                    ? "↻ Refresh"
                    : "🔍 Load releases"}
              </button>
            </div>
            <Show when={optError()}>
              <small style={{ color: "#ef4444" }}>{optError()}</small>
            </Show>
            <Show when={param.description}>
              <small>{param.description}</small>
            </Show>
            <Show when={hasDynamic() && value()}>
              <small
                style={{
                  color: "#10b981",
                  display: "flex",
                  "align-items": "center",
                  gap: "0.25rem",
                  "margin-top": "0.25rem",
                }}
              >
                ✓ Selected: {value()}
              </small>
            </Show>
          </div>
        );

      case "curse_forge_file_version":
        return (
          <div class="form-group">
            <label>
              {param.label}
              {param.required && " *"}
            </label>
            {/* ─── CurseForge integration not configured warning ─── */}
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
                  "margin-bottom": "0.5rem",
                  "font-size": "0.85rem",
                  color: "#fdba74",
                }}
              >
                <span style={{ "flex-shrink": "0" }}>🔶</span>
                <div>
                  <strong>CurseForge API key not configured.</strong> Loading
                  file versions and downloading server packs will fail until an
                  admin configures the API key.
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
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.5rem",
              }}
            >
              <div style={{ flex: "1" }}>
                <Show
                  when={hasDynamic()}
                  fallback={
                    <input
                      type="text"
                      value={value()}
                      onInput={(e) =>
                        handleParamChange(param.name, e.currentTarget.value)
                      }
                      placeholder={
                        !integrations.status().curseforge_configured
                          ? "CurseForge API key required — see warning above"
                          : "Click 'Load versions' to see options"
                      }
                      style={{ width: "100%" }}
                      disabled={!integrations.status().curseforge_configured}
                    />
                  }
                >
                  <SearchableSelect
                    options={getMergedOptions(param).map((opt) => ({
                      value: opt.value,
                      label: opt.label,
                    }))}
                    value={value()}
                    onChange={(v) => handleParamChange(param.name, v)}
                    allowEmpty={!param.required}
                    emptyLabel="— select a version —"
                    placeholder="Search file versions…"
                  />
                </Show>
              </div>
              <button
                class="btn btn-sm"
                onClick={async () => {
                  if (!integrations.status().curseforge_configured) {
                    setOptionErrors({
                      ...optionErrors(),
                      [param.name]:
                        "CurseForge API key is not configured. Ask an admin to set it up in Admin Panel → CurseForge.",
                    });
                    return;
                  }
                  if (!param.curseforge_project_id) {
                    setOptionErrors({
                      ...optionErrors(),
                      [param.name]: "No CurseForge project ID configured",
                    });
                    return;
                  }
                  setLoadingOptions({
                    ...loadingOptions(),
                    [param.name]: true,
                  });
                  setOptionErrors((prev) => {
                    const n = { ...prev };
                    delete n[param.name];
                    return n;
                  });
                  try {
                    const resp = await fetchCurseForgeFiles(
                      param.curseforge_project_id,
                    );
                    const options = resp.options.map(
                      (o: { value: string; label: string }) => ({
                        value: o.value,
                        label: o.label,
                      }),
                    );
                    setDynamicOptions({
                      ...dynamicOptions(),
                      [param.name]: options,
                    });
                    // Auto-select first option if nothing selected
                    if (!value() && options.length > 0) {
                      handleParamChange(param.name, options[0].value);
                    }
                  } catch (err) {
                    const msg =
                      err instanceof Error ? err.message : String(err);
                    setOptionErrors({ ...optionErrors(), [param.name]: msg });
                  } finally {
                    setLoadingOptions({
                      ...loadingOptions(),
                      [param.name]: false,
                    });
                  }
                }}
                disabled={
                  isLoadingOpt() || !integrations.status().curseforge_configured
                }
                title={
                  !integrations.status().curseforge_configured
                    ? "CurseForge API key must be configured by an admin before loading versions"
                    : "Fetch file versions from CurseForge"
                }
                style={{
                  "white-space": "nowrap",
                  "min-width": "fit-content",
                  ...(!integrations.status().curseforge_configured
                    ? { opacity: "0.5", cursor: "not-allowed" }
                    : {}),
                  background:
                    hasDynamic() && integrations.status().curseforge_configured
                      ? "linear-gradient(135deg, #f97316 0%, #ea580c 100%)"
                      : undefined,
                  border:
                    hasDynamic() && integrations.status().curseforge_configured
                      ? "none"
                      : undefined,
                  color:
                    hasDynamic() && integrations.status().curseforge_configured
                      ? "#fff"
                      : undefined,
                  "box-shadow":
                    hasDynamic() && integrations.status().curseforge_configured
                      ? "0 2px 8px rgba(249, 115, 22, 0.3)"
                      : undefined,
                }}
              >
                {!integrations.status().curseforge_configured
                  ? "🔒 Not configured"
                  : isLoadingOpt()
                    ? "Loading…"
                    : hasDynamic()
                      ? "↻ Refresh"
                      : "🔍 Load versions"}
              </button>
            </div>
            <Show when={optError()}>
              <small style={{ color: "#ef4444" }}>{optError()}</small>
            </Show>
            <Show when={param.description}>
              <small>{param.description}</small>
            </Show>
            <Show when={hasDynamic() && value()}>
              <small
                style={{
                  color: "#10b981",
                  display: "flex",
                  "align-items": "center",
                  gap: "0.25rem",
                  "margin-top": "0.25rem",
                }}
              >
                ✓ Selected: file ID {value()}
              </small>
            </Show>
          </div>
        );

      case "boolean":
        return (
          <label class="checkbox-label">
            <input
              type="checkbox"
              checked={value() === "true"}
              onChange={(e) =>
                handleParamChange(
                  param.name,
                  e.currentTarget.checked ? "true" : "false",
                )
              }
            />
            {param.label}
            {param.required && " *"}
          </label>
        );

      case "number":
        return (
          <div class="form-group">
            <label>
              {param.label}
              {param.required && " *"}
            </label>
            <input
              type="number"
              value={value()}
              onInput={(e) =>
                handleParamChange(param.name, e.currentTarget.value)
              }
              placeholder={param.default ?? ""}
            />
            <Show when={param.description}>
              <small>{param.description}</small>
            </Show>
          </div>
        );

      case "select":
        return (
          <div class="form-group">
            <label>
              {param.label}
              {param.required && " *"}
            </label>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.5rem",
              }}
            >
              <SearchableSelect
                options={getMergedOptions(param).map((opt) => ({
                  value: opt.value,
                  label:
                    opt.label !== opt.value
                      ? `${opt.label} (${opt.value})`
                      : opt.value,
                }))}
                value={value()}
                onChange={(v) => handleParamChange(param.name, v)}
                allowEmpty={!param.required}
                emptyLabel="— none —"
                placeholder="Select or search…"
              />
              <Show when={hasOptionsFrom()}>
                <button
                  class="btn btn-sm"
                  onClick={() => handleLoadOptions(param)}
                  disabled={isLoadingOpt()}
                  title="Fetch dropdown options from the external API defined in the template"
                  style={{
                    "white-space": "nowrap",
                    "min-width": "fit-content",
                  }}
                >
                  {isLoadingOpt()
                    ? "Loading…"
                    : hasDynamic()
                      ? "↻ Refresh"
                      : "Load options"}
                </button>
              </Show>
            </div>
            <Show when={optError()}>
              <small style={{ color: "#ef4444" }}>{optError()}</small>
            </Show>
            <Show when={param.description}>
              <small>{param.description}</small>
            </Show>
          </div>
        );

      default: {
        // "string" or unrecognized — but may still have options_from
        const showAsSelect = () => hasOptionsFrom() && hasDynamic();

        return (
          <div class="form-group">
            <label>
              {param.label}
              {param.required && " *"}
            </label>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.5rem",
              }}
            >
              <Show
                when={showAsSelect()}
                fallback={
                  <input
                    type="text"
                    value={value()}
                    onInput={(e) =>
                      handleParamChange(param.name, e.currentTarget.value)
                    }
                    placeholder={param.default ?? ""}
                    style={{ flex: "1" }}
                  />
                }
              >
                <SearchableSelect
                  options={getMergedOptions(param).map((opt) => ({
                    value: opt.value,
                    label:
                      opt.label !== opt.value
                        ? `${opt.label} (${opt.value})`
                        : opt.value,
                  }))}
                  value={value()}
                  onChange={(v) => handleParamChange(param.name, v)}
                  allowEmpty={!param.required}
                  emptyLabel="— none —"
                  placeholder="Select or search…"
                />
              </Show>
              <Show when={hasOptionsFrom()}>
                <button
                  class="btn btn-sm"
                  onClick={() => handleLoadOptions(param)}
                  disabled={isLoadingOpt()}
                  title="Fetch dropdown options from the external API defined in the template"
                  style={{
                    "white-space": "nowrap",
                    "min-width": "fit-content",
                  }}
                >
                  {isLoadingOpt()
                    ? "Loading…"
                    : hasDynamic()
                      ? "↻ Refresh"
                      : "Load options"}
                </button>
              </Show>
            </div>
            <Show when={optError()}>
              <small style={{ color: "#ef4444" }}>{optError()}</small>
            </Show>
            <Show when={param.description}>
              <small>{param.description}</small>
            </Show>
          </div>
        );
      }
    }
  };

  // ─── Template Mode Render ───
  const renderTemplateMode = () => (
    <>
      <Show when={error()}>
        <div class="error-msg">{error()}</div>
      </Show>

      <Show
        when={selectedTemplate()?.requires_steamcmd && !steamcmdAvailable()}
      >
        <div class="wizard-warning" role="alert">
          <span class="wizard-warning__icon" aria-hidden="true">
            ⚠️
          </span>
          <div class="wizard-warning__content">
            <strong>SteamCMD is not available</strong>
            <p>
              This template requires SteamCMD for installation and updates. You
              can still create the server, but the install pipeline will fail
              until SteamCMD is installed on the host.
            </p>
            <a
              href="https://developer.valvesoftware.com/wiki/SteamCMD#Downloading_SteamCMD"
              target="_blank"
              rel="noopener noreferrer"
              class="wizard-warning__link"
            >
              SteamCMD installation guide →
            </a>
          </div>
        </div>
      </Show>

      <Show when={!selectedTemplate()}>
        {/* Template Selection */}
        <Show when={templatesLoading()}>
          <Loader message="Loading templates" />
        </Show>

        <Show when={!templatesLoading() && templates().length === 0}>
          <div class="empty-state">
            <h2>No templates available</h2>
            <p>
              Create templates from the{" "}
              <a
                href="/templates"
                style={{
                  color: "var(--accent)",
                  "text-decoration": "underline",
                }}
              >
                Templates page
              </a>{" "}
              or by saving an existing server's configuration as a template.
            </p>
            <p
              style={{
                "margin-top": "0.75rem",
                color: "#9ca3af",
                "font-size": "0.9rem",
              }}
            >
              Or use the <strong>Wizard</strong> to build a server configuration
              from scratch.
            </p>
          </div>
        </Show>

        <Show when={!templatesLoading() && templates().length > 0}>
          <div class="template-select-grid">
            <For each={templates()}>
              {(template) => (
                <button
                  class={`template-select-card${template.is_builtin ? " template-select-card-builtin" : ""}${template.requires_steamcmd && !steamcmdAvailable() ? " template-select-card-steamcmd-warn" : ""}`}
                  onClick={() => {
                    applyTemplate(template);
                  }}
                >
                  <div class="template-select-card-icon">
                    {template.is_builtin ? "📦" : "📄"}
                  </div>
                  <div class="template-select-card-body">
                    <div class="template-select-card-title-row">
                      <h4>{template.name}</h4>
                      <Show when={template.is_builtin}>
                        <span class="template-builtin-badge">Built-in</span>
                      </Show>
                    </div>
                    <Show
                      when={template.requires_steamcmd && !steamcmdAvailable()}
                    >
                      <div
                        class="template-select-card-steamcmd-notice"
                        role="status"
                      >
                        ⚠️ Requires SteamCMD (not installed)
                      </div>
                    </Show>
                    <Show
                      when={template.requires_steamcmd && steamcmdAvailable()}
                    >
                      <div class="template-select-card-steamcmd-ok">
                        🎮 Uses SteamCMD
                      </div>
                    </Show>
                    <Show when={template.description}>
                      <MarkdownRenderer
                        content={template.description!}
                        class="markdown-body-compact template-select-card-desc"
                      />
                    </Show>
                    <div class="template-select-card-meta">
                      <Show when={template.config.parameters.length > 0}>
                        <span>
                          🔧 {template.config.parameters.length} param
                          {template.config.parameters.length !== 1 ? "s" : ""}
                        </span>
                      </Show>
                      <Show when={template.config.install_steps.length > 0}>
                        <span>
                          📦 {template.config.install_steps.length} install step
                          {template.config.install_steps.length !== 1
                            ? "s"
                            : ""}
                        </span>
                      </Show>
                      <Show when={template.config.update_steps.length > 0}>
                        <span>
                          🔄 {template.config.update_steps.length} update step
                          {template.config.update_steps.length !== 1 ? "s" : ""}
                        </span>
                      </Show>
                    </div>
                  </div>
                  <div class="template-select-card-arrow">→</div>
                </button>
              )}
            </For>
          </div>
        </Show>
      </Show>

      {/* Template selected — show config with parameter fill-in */}
      <Show when={selectedTemplate()}>
        {(tmpl) => (
          <>
            <div class="template-selected-banner">
              <div class="template-selected-info">
                <span class="template-selected-icon">
                  {tmpl().is_builtin ? "📦" : "📄"}
                </span>
                <div>
                  <strong>
                    Template: {tmpl().name}
                    <Show when={tmpl().is_builtin}>
                      {" "}
                      <span class="template-builtin-badge">Built-in</span>
                    </Show>
                  </strong>
                  <Show when={tmpl().description}>
                    <MarkdownRenderer
                      content={tmpl().description!}
                      class="markdown-body-compact"
                      inline
                    />
                  </Show>
                </div>
              </div>
              <button
                class="btn btn-sm"
                style={{
                  "white-space": "nowrap",
                  "padding-inline": "1rem",
                  "flex-shrink": "0",
                }}
                onClick={() => {
                  setSelectedTemplate(null);
                  setConfig({ ...defaultConfig });
                  setParameterValues({});
                }}
              >
                Change Template
              </button>
            </div>

            {/* Editable server name */}
            <div class="config-editor" style={{ "margin-bottom": "1.5rem" }}>
              <h3>Server Name</h3>
              <div class="form-group">
                <label for="tmpl-name">Name *</label>
                <input
                  id="tmpl-name"
                  type="text"
                  value={config().name}
                  onInput={(e) => {
                    setConfig((prev) => ({
                      ...prev,
                      name: e.currentTarget.value,
                    }));
                  }}
                  placeholder="Give your server a name"
                />
                <small>You can change this from the template default.</small>
              </div>
            </div>

            {/* Parameter fill-in */}
            <Show when={hasParameters()}>
              <div class="config-editor" style={{ "margin-bottom": "1.5rem" }}>
                <h3>Template Parameters</h3>
                <p
                  style={{
                    "font-size": "0.85rem",
                    color: "#9ca3af",
                    "margin-bottom": "1rem",
                  }}
                >
                  This template requires the following values. They'll be
                  substituted as{" "}
                  <code style={{ "font-size": "0.8rem" }}>{"${name}"}</code>{" "}
                  throughout the configuration and pipeline steps.
                </p>
                <For each={config().parameters ?? []}>
                  {(param) => renderParameterInput(param)}
                </For>
              </div>
            </Show>

            {/* Java runtime selector — for Java-based servers or when helper is enabled */}
            <Show
              when={
                isJavaBinary(config().binary) || config().enable_java_helper
              }
            >
              <div class="config-editor" style={{ "margin-bottom": "1.5rem" }}>
                <h3>☕ Java Runtime</h3>
                <p
                  style={{
                    "font-size": "0.85rem",
                    color: "#9ca3af",
                    "margin-bottom": "0.5rem",
                  }}
                >
                  This template uses Java. You can optionally select a specific
                  Java runtime to configure via environment variables. The
                  backend automatically prepends <code>$JAVA_HOME/bin</code> to{" "}
                  <code>PATH</code>, so shell scripts will use the selected
                  runtime.
                </p>
                <JavaRuntimeSelector
                  currentBinary={config().binary}
                  currentEnv={config().env}
                  onEnvChange={(envVars) => {
                    setConfig((prev) => {
                      const merged = { ...prev.env };
                      for (const [key, value] of Object.entries(envVars)) {
                        if (value === "") {
                          delete merged[key];
                        } else {
                          merged[key] = value;
                        }
                      }
                      return { ...prev, env: merged };
                    });
                  }}
                />
              </div>
            </Show>

            {/* .NET runtime selector — for .NET-based servers or when helper is enabled */}
            <Show
              when={
                isDotnetBinary(config().binary) || config().enable_dotnet_helper
              }
            >
              <div class="config-editor" style={{ "margin-bottom": "1.5rem" }}>
                <h3>⚡ .NET Runtime</h3>
                <p
                  style={{
                    "font-size": "0.85rem",
                    color: "#9ca3af",
                    "margin-bottom": "0.5rem",
                  }}
                >
                  This template uses .NET. Select a .NET runtime to configure
                  the appropriate environment variables.
                </p>
                <DotnetRuntimeSelector
                  currentBinary={config().binary}
                  currentEnv={config().env}
                  onSelect={(envVars) => {
                    setConfig((prev) => {
                      const merged = { ...prev.env };
                      for (const [key, value] of Object.entries(envVars)) {
                        if (value === "") {
                          delete merged[key];
                        } else {
                          merged[key] = value;
                        }
                      }
                      return { ...prev, env: merged };
                    });
                  }}
                />
              </div>
            </Show>

            {/* Pipeline summary */}
            <Show
              when={
                (config().install_steps?.length ?? 0) > 0 ||
                (config().update_steps?.length ?? 0) > 0
              }
            >
              <div
                style={{
                  "margin-bottom": "1rem",
                  padding: "0.75rem 1rem",
                  background: "#1e293b",
                  "border-radius": "0.5rem",
                  "font-size": "0.85rem",
                  color: "#94a3b8",
                }}
              >
                <Show when={(config().install_steps?.length ?? 0) > 0}>
                  <p>
                    📦{" "}
                    <strong style={{ color: "#e2e8f0" }}>
                      Install pipeline
                    </strong>
                    : {config().install_steps.length} step
                    {config().install_steps.length !== 1 ? "s" : ""} defined.
                  </p>
                </Show>
                <Show when={(config().update_steps?.length ?? 0) > 0}>
                  <p style={{ "margin-top": "0.25rem" }}>
                    🔄{" "}
                    <strong style={{ color: "#e2e8f0" }}>
                      Update pipeline
                    </strong>
                    : {config().update_steps.length} step
                    {config().update_steps.length !== 1 ? "s" : ""} defined.
                  </p>
                </Show>
              </div>
            </Show>

            <div
              style={{
                "margin-top": "1.5rem",
                display: "flex",
                gap: "0.75rem",
              }}
            >
              <button
                class="btn btn-primary"
                onClick={handleSubmit}
                disabled={submitting()}
              >
                {submitting() ? "Creating..." : "Create Server"}
              </button>
              <button class="btn" onClick={() => navigate("/")}>
                Cancel
              </button>
            </div>
          </>
        )}
      </Show>
    </>
  );

  return (
    <div class="create-server">
      <div class="page-header">
        <h1>Create Server</h1>
      </div>

      {/* ─── Mode Selector ─── */}
      <div class="creation-mode-selector">
        <button
          class="creation-mode-btn"
          classList={{ active: mode() === "template" }}
          onClick={() => handleModeSwitch("template")}
        >
          <span class="creation-mode-icon">📄</span>
          <span class="creation-mode-info">
            <span class="creation-mode-title">From Template</span>
            <span class="creation-mode-desc">
              Pick a saved template, fill in parameters, and go.
            </span>
          </span>
        </button>
        <button
          class="creation-mode-btn"
          classList={{ active: mode() === "wizard" }}
          onClick={() => handleModeSwitch("wizard")}
        >
          <span class="creation-mode-icon">🧙</span>
          <span class="creation-mode-info">
            <span class="creation-mode-title">Wizard</span>
            <span class="creation-mode-desc">
              Step-by-step — define parameters, install, update, and start
              pipelines.
            </span>
          </span>
        </button>
      </div>

      {/* ─── Mode Content ─── */}
      <Show when={mode() === "template"}>{renderTemplateMode()}</Show>
      <Show when={mode() === "wizard"}>
        <WizardCreateServer />
      </Show>
    </div>
  );
};

export default CreateServer;
