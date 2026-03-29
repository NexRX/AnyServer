import {
  type Component,
  createResource,
  createSignal,
  For,
  Show,
} from "solid-js";
import Loader from "../components/Loader";
import MarkdownRenderer from "../components/MarkdownRenderer";
import { A, useNavigate } from "@solidjs/router";
import { listTemplates, deleteTemplate, createTemplate } from "../api/client";
import { formatDate } from "../utils/format";
import type {
  ServerTemplate,
  ServerConfig,
  PipelineStep,
} from "../types/bindings";

const Templates: Component = () => {
  const navigate = useNavigate();
  const [data, { refetch }] = createResource(listTemplates);
  const [showCreate, setShowCreate] = createSignal(false);
  const [importJson, setImportJson] = createSignal("");
  const [templateName, setTemplateName] = createSignal("");
  const [templateDesc, setTemplateDesc] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  const [descriptionTab, setDescriptionTab] = createSignal<"write" | "preview">(
    "write",
  );
  const [pendingConfig, setPendingConfig] = createSignal<ServerConfig | null>(
    null,
  );
  const [showRunCommandWarning, setShowRunCommandWarning] = createSignal(false);
  const [pendingTemplate, setPendingTemplate] =
    createSignal<ServerTemplate | null>(null);
  const [showUseTemplateWarning, setShowUseTemplateWarning] =
    createSignal(false);

  const handleDelete = async (t: ServerTemplate) => {
    if (!confirm(`Delete template "${t.name}"? This cannot be undone.`)) return;

    try {
      await deleteTemplate(t.id);
      refetch();
    } catch (e: any) {
      setError(`Delete failed: ${e.message || e}`);
    }
  };

  const handleUseTemplate = (t: ServerTemplate) => {
    // Phase 4 — Check for RunCommand steps and show warning
    const runCommandSteps = extractRunCommandSteps(t.config);
    if (runCommandSteps.length > 0) {
      setPendingTemplate(t);
      setShowUseTemplateWarning(true);
      return;
    }

    // No RunCommand steps, proceed directly
    sessionStorage.setItem("anyserver_template", JSON.stringify(t));
    navigate("/create");
  };

  const handleConfirmUseTemplate = () => {
    const template = pendingTemplate();
    if (template) {
      sessionStorage.setItem("anyserver_template", JSON.stringify(template));
      navigate("/create");
    }
  };

  const handleCancelUseTemplate = () => {
    setPendingTemplate(null);
    setShowUseTemplateWarning(false);
  };

  const extractRunCommandSteps = (
    config: ServerConfig,
  ): Array<{ phase: string; step: PipelineStep }> => {
    const runCommands: Array<{ phase: string; step: PipelineStep }> = [];

    const checkSteps = (steps: PipelineStep[], phase: string) => {
      steps.forEach((step) => {
        if (step.action.type === "run_command") {
          runCommands.push({ phase, step });
        }
      });
    };

    checkSteps(config.install_steps || [], "install");
    checkSteps(config.update_steps || [], "update");
    checkSteps(config.uninstall_steps || [], "uninstall");

    return runCommands;
  };

  const handleCreateFromJson = async () => {
    const name = templateName().trim();
    if (!name) {
      setError("Template name is required.");
      return;
    }

    const json = importJson().trim();
    if (!json) {
      setError("Please paste a server configuration JSON.");
      return;
    }

    let config: ServerConfig;
    try {
      config = JSON.parse(json);
    } catch {
      setError("Invalid JSON. Please check the format and try again.");
      return;
    }

    if (!config.name && !config.binary) {
      setError(
        "The JSON doesn't look like a valid server configuration. It should have at least 'name' and 'binary' fields.",
      );
      return;
    }

    // Phase 4 — Check for RunCommand steps and show warning
    const runCommandSteps = extractRunCommandSteps(config);
    if (runCommandSteps.length > 0) {
      setPendingConfig(config);
      setShowRunCommandWarning(true);
      return;
    }

    // No RunCommand steps, proceed directly
    await saveTemplate(config);
  };

  const saveTemplate = async (config: ServerConfig) => {
    setSubmitting(true);
    setError(null);

    try {
      await createTemplate({
        name: templateName(),
        description: templateDesc().trim() || null,
        config,
      });
      setShowCreate(false);
      setImportJson("");
      setTemplateName("");
      setTemplateDesc("");
      setPendingConfig(null);
      setShowRunCommandWarning(false);
      refetch();
    } catch (e: any) {
      setError(e.message || "Failed to create template.");
    } finally {
      setSubmitting(false);
    }
  };

  const handleConfirmRunCommands = async () => {
    const config = pendingConfig();
    if (config) {
      await saveTemplate(config);
    }
  };

  const handleCancelRunCommands = () => {
    setPendingConfig(null);
    setShowRunCommandWarning(false);
  };

  const handleCancelCreate = () => {
    setShowCreate(false);
    setImportJson("");
    setTemplateName("");
    setTemplateDesc("");
    setDescriptionTab("write");
    setError(null);
  };

  return (
    <div class="templates-page">
      <div class="page-header">
        <h1>Templates</h1>
        <div class="actions" style={{ display: "flex", gap: "0.5rem" }}>
          <Show when={!showCreate()}>
            <button class="btn btn-primary" onClick={() => setShowCreate(true)}>
              📥 Import Template
            </button>
          </Show>
          <button
            class="btn"
            onClick={() => navigate("/create")}
            title="Create a server to use as the basis for a new template"
          >
            + Create New
          </button>
        </div>
      </div>

      <Show when={error()}>
        {(err) => <div class="error-msg">{err()}</div>}
      </Show>

      {/* ─── RunCommand Warning Dialog (Template Import) ─── */}
      <Show when={showRunCommandWarning()}>
        <div class="modal-overlay" onClick={handleCancelRunCommands}>
          <div class="modal-content" onClick={(e) => e.stopPropagation()}>
            <h2>⚠️ Security Warning: Shell Commands</h2>
            <p style={{ "margin-bottom": "1rem" }}>
              This template contains{" "}
              <strong>{extractRunCommandSteps(pendingConfig()!).length}</strong>{" "}
              shell command
              {extractRunCommandSteps(pendingConfig()!).length !== 1
                ? "s"
                : ""}{" "}
              that will run during installation/updates:
            </p>

            <div
              style={{
                "max-height": "400px",
                "overflow-y": "auto",
                border: "1px solid var(--border)",
                "border-radius": "0.25rem",
                padding: "1rem",
                background: "var(--bg-secondary)",
                "margin-bottom": "1rem",
              }}
            >
              <For each={extractRunCommandSteps(pendingConfig()!)}>
                {(item, index) => (
                  <div
                    style={{
                      "margin-bottom": "1rem",
                      "padding-bottom": "1rem",
                      "border-bottom":
                        index() <
                        extractRunCommandSteps(pendingConfig()!).length - 1
                          ? "1px solid var(--border-dim)"
                          : "none",
                    }}
                  >
                    <div style={{ "margin-bottom": "0.5rem" }}>
                      <strong style={{ color: "var(--warning)" }}>
                        {item.phase.charAt(0).toUpperCase() +
                          item.phase.slice(1)}{" "}
                        Phase
                      </strong>
                      {" — "}
                      <span style={{ color: "var(--text-muted)" }}>
                        {item.step.name}
                      </span>
                    </div>
                    <code
                      style={{
                        display: "block",
                        padding: "0.5rem",
                        background: "var(--bg)",
                        "border-radius": "0.25rem",
                        "font-size": "0.85rem",
                        "word-break": "break-all",
                      }}
                    >
                      {
                        (
                          item.step.action as {
                            type: "run_command";
                            command: string;
                            args: string[];
                          }
                        ).command
                      }{" "}
                      {(
                        item.step.action as {
                          type: "run_command";
                          command: string;
                          args: string[];
                        }
                      ).args?.join(" ") || ""}
                    </code>
                  </div>
                )}
              </For>
            </div>

            <div
              style={{
                background: "rgba(255, 193, 7, 0.1)",
                border: "1px solid var(--warning)",
                "border-radius": "0.25rem",
                padding: "1rem",
                "margin-bottom": "1rem",
              }}
            >
              <strong>⚠️ Important:</strong>
              <ul style={{ margin: "0.5rem 0 0 1.5rem", padding: "0" }}>
                <li>
                  These commands run with the privileges of the AnyServer
                  process
                </li>
                <li>
                  They can read/write files, install software, and make network
                  requests
                </li>
                <li>Only import templates from sources you trust</li>
                <li>RunCommand execution must be enabled in Admin Settings</li>
              </ul>
            </div>

            <div style={{ display: "flex", gap: "0.75rem" }}>
              <button
                class="btn btn-primary"
                onClick={handleConfirmRunCommands}
                disabled={submitting()}
              >
                {submitting() ? "Saving..." : "I Understand, Import Template"}
              </button>
              <button
                class="btn"
                onClick={handleCancelRunCommands}
                disabled={submitting()}
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      </Show>

      {/* ─── RunCommand Warning Dialog (Use Template) ─── */}
      <Show when={showUseTemplateWarning()}>
        <div class="modal-overlay" onClick={handleCancelUseTemplate}>
          <div class="modal-content" onClick={(e) => e.stopPropagation()}>
            <h2>⚠️ Security Warning: Shell Commands</h2>
            <p style={{ "margin-bottom": "1rem" }}>
              This template contains{" "}
              <strong>
                {extractRunCommandSteps(pendingTemplate()!.config).length}
              </strong>{" "}
              shell command
              {extractRunCommandSteps(pendingTemplate()!.config).length !== 1
                ? "s"
                : ""}{" "}
              that will run when you install/update servers from this template:
            </p>

            <div
              style={{
                "max-height": "400px",
                "overflow-y": "auto",
                border: "1px solid var(--border)",
                "border-radius": "0.25rem",
                padding: "1rem",
                background: "var(--bg-secondary)",
                "margin-bottom": "1rem",
              }}
            >
              <For each={extractRunCommandSteps(pendingTemplate()!.config)}>
                {(item, index) => (
                  <div
                    style={{
                      "margin-bottom": "1rem",
                      "padding-bottom": "1rem",
                      "border-bottom":
                        index() <
                        extractRunCommandSteps(pendingTemplate()!.config)
                          .length -
                          1
                          ? "1px solid var(--border-dim)"
                          : "none",
                    }}
                  >
                    <div style={{ "margin-bottom": "0.5rem" }}>
                      <strong style={{ color: "var(--warning)" }}>
                        {item.phase.charAt(0).toUpperCase() +
                          item.phase.slice(1)}{" "}
                        Phase
                      </strong>
                      {" — "}
                      <span style={{ color: "var(--text-muted)" }}>
                        {item.step.name}
                      </span>
                    </div>
                    <code
                      style={{
                        display: "block",
                        padding: "0.5rem",
                        background: "var(--bg)",
                        "border-radius": "0.25rem",
                        "font-size": "0.85rem",
                        "word-break": "break-all",
                      }}
                    >
                      {
                        (
                          item.step.action as {
                            type: "run_command";
                            command: string;
                            args: string[];
                          }
                        ).command
                      }{" "}
                      {(
                        item.step.action as {
                          type: "run_command";
                          command: string;
                          args: string[];
                        }
                      ).args?.join(" ") || ""}
                    </code>
                  </div>
                )}
              </For>
            </div>

            <div
              style={{
                background: "rgba(255, 193, 7, 0.1)",
                border: "1px solid var(--warning)",
                "border-radius": "0.25rem",
                padding: "1rem",
                "margin-bottom": "1rem",
              }}
            >
              <strong>⚠️ Important:</strong>
              <ul style={{ margin: "0.5rem 0 0 1.5rem", padding: "0" }}>
                <li>
                  These commands run with the privileges of the AnyServer
                  process
                </li>
                <li>
                  They can read/write files, install software, and make network
                  requests
                </li>
                <li>Only use templates from sources you trust</li>
                <li>RunCommand execution must be enabled in Admin Settings</li>
              </ul>
            </div>

            <div style={{ display: "flex", gap: "0.75rem" }}>
              <button
                class="btn btn-primary"
                onClick={handleConfirmUseTemplate}
              >
                I Understand, Use This Template
              </button>
              <button class="btn" onClick={handleCancelUseTemplate}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      </Show>

      {/* ─── Import Template Dialog ─── */}
      <Show when={showCreate()}>
        <div class="template-create-card">
          <h3>Import Template</h3>
          <p class="template-create-hint">
            Paste a server configuration JSON to save it as a reusable template.
            When someone creates a server from this template, they'll be
            prompted to fill in any defined parameters.
          </p>

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
            <label>Server Configuration JSON *</label>
            <textarea
              value={importJson()}
              onInput={(e) => setImportJson(e.currentTarget.value)}
              placeholder='{"name": "My Server", "binary": "...", ...}'
              rows={12}
              style={{ "font-family": "monospace", "font-size": "0.85rem" }}
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
                class={descriptionTab() === "write" ? "active" : ""}
                onClick={() => setDescriptionTab("write")}
              >
                Write
              </button>
              <button
                class={descriptionTab() === "preview" ? "active" : ""}
                onClick={() => setDescriptionTab("preview")}
              >
                Preview
              </button>
            </div>
            <Show
              when={descriptionTab() === "write"}
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
            style={{ display: "flex", gap: "0.75rem", "margin-top": "1rem" }}
          >
            <button
              class="btn btn-primary"
              onClick={handleCreateFromJson}
              disabled={submitting()}
            >
              {submitting() ? "Saving..." : "Save Template"}
            </button>
            <button class="btn" onClick={handleCancelCreate}>
              Cancel
            </button>
          </div>
        </div>
      </Show>

      {/* ─── Loading ─── */}
      <Show when={data.loading && !data()}>
        <Loader message="Loading templates" />
      </Show>

      <Show when={data.error}>
        <div class="error-msg">
          Failed to load templates: {String(data.error?.message ?? data.error)}
        </div>
      </Show>

      {/* ─── Template Grid ─── */}
      <Show when={data()}>
        {(resolved) => (
          <Show
            when={resolved().templates.length > 0}
            fallback={
              <Show when={!showCreate()}>
                <div class="empty-state">
                  <h2>No templates yet</h2>
                  <p>
                    Import a template from JSON, or create a server and save its
                    configuration as a reusable template from the Pipeline tab.
                  </p>
                  <div
                    style={{
                      display: "flex",
                      gap: "0.75rem",
                      "justify-content": "center",
                      "margin-top": "1rem",
                    }}
                  >
                    <button
                      class="btn btn-primary"
                      onClick={() => setShowCreate(true)}
                    >
                      📥 Import Template
                    </button>
                    <button class="btn" onClick={() => navigate("/create")}>
                      + Create New Server
                    </button>
                  </div>
                </div>
              </Show>
            }
          >
            <div class="template-grid">
              <For each={resolved().templates}>
                {(template) => {
                  const isSteamDisabled = () =>
                    template.requires_steamcmd &&
                    !resolved().steamcmd_available;
                  return (
                    <div
                      class={`template-card${template.is_builtin ? " template-card-builtin" : ""}${isSteamDisabled() ? " template-card--disabled" : ""}`}
                      aria-disabled={isSteamDisabled() ? "true" : undefined}
                      aria-label={
                        isSteamDisabled()
                          ? `${template.name} — disabled because SteamCMD is not installed on this host`
                          : undefined
                      }
                    >
                      <div class="template-card-header">
                        <div class="template-card-icon">
                          {template.is_builtin ? "📦" : "📄"}
                        </div>
                        <div class="template-card-title">
                          <div class="template-card-title-row">
                            <h3>{template.name}</h3>
                            <Show when={template.is_builtin}>
                              <span class="template-builtin-badge">
                                Built-in
                              </span>
                            </Show>
                            <Show when={template.requires_steamcmd}>
                              <span
                                class="template-builtin-badge"
                                style={{
                                  background: resolved().steamcmd_available
                                    ? "rgba(34, 197, 94, 0.15)"
                                    : "rgba(239, 68, 68, 0.15)",
                                  color: resolved().steamcmd_available
                                    ? "#22c55e"
                                    : "#f87171",
                                }}
                              >
                                {resolved().steamcmd_available
                                  ? "🎮 SteamCMD"
                                  : "⚠️ SteamCMD Required"}
                              </span>
                            </Show>
                          </div>
                          <Show when={template.description}>
                            <MarkdownRenderer
                              content={template.description!}
                              class="markdown-body-compact template-card-desc"
                            />
                          </Show>
                        </div>
                      </div>

                      <div class="template-card-meta">
                        <Show when={template.config.steam_app_id != null}>
                          <span class="template-meta-tag">
                            🎮 Steam App {template.config.steam_app_id}
                          </span>
                        </Show>
                        <Show when={template.config.parameters.length > 0}>
                          <span class="template-meta-tag">
                            🔧 {template.config.parameters.length} parameter
                            {template.config.parameters.length !== 1 ? "s" : ""}
                          </span>
                        </Show>
                        <Show when={template.config.install_steps.length > 0}>
                          <span class="template-meta-tag">
                            📦 {template.config.install_steps.length} install
                            step
                            {template.config.install_steps.length !== 1
                              ? "s"
                              : ""}
                          </span>
                        </Show>
                        <Show when={template.config.update_steps.length > 0}>
                          <span class="template-meta-tag">
                            🔄 {template.config.update_steps.length} update step
                            {template.config.update_steps.length !== 1
                              ? "s"
                              : ""}
                          </span>
                        </Show>
                        <Show when={!template.is_builtin}>
                          <span class="template-meta-tag template-meta-date">
                            {formatDate(template.created_at)}
                          </span>
                        </Show>
                      </div>

                      <div class="template-card-details">
                        <div class="template-detail-row">
                          <span class="template-detail-label">Binary</span>
                          <code class="template-detail-value">
                            {template.config.binary || "—"}
                          </code>
                        </div>
                        <Show when={template.config.args.length > 0}>
                          <div class="template-detail-row">
                            <span class="template-detail-label">Args</span>
                            <code class="template-detail-value">
                              {template.config.args.join(" ")}
                            </code>
                          </div>
                        </Show>
                      </div>

                      <Show when={isSteamDisabled()}>
                        <div class="template-card-unavailable" role="alert">
                          <p class="template-card-unavailable-text">
                            ⚠️ SteamCMD is required but not available on this
                            host.
                          </p>
                          <a
                            href="https://developer.valvesoftware.com/wiki/SteamCMD#Downloading_SteamCMD"
                            target="_blank"
                            rel="noopener noreferrer"
                            class="template-card-learn-more"
                          >
                            How to install SteamCMD →
                          </a>
                        </div>
                      </Show>

                      <div class="template-card-actions">
                        <button
                          class="btn btn-primary btn-sm"
                          onClick={() => handleUseTemplate(template)}
                          disabled={isSteamDisabled()}
                          aria-disabled={isSteamDisabled() ? "true" : undefined}
                          aria-label={
                            isSteamDisabled()
                              ? "Cannot use this template because SteamCMD is not installed"
                              : undefined
                          }
                          title={
                            isSteamDisabled()
                              ? "SteamCMD is not installed on this host. Install SteamCMD and restart AnyServer to enable this template."
                              : undefined
                          }
                        >
                          {isSteamDisabled()
                            ? "SteamCMD Required"
                            : "Use Template"}
                        </button>
                        <button
                          class="btn btn-sm"
                          onClick={() => {
                            const json = JSON.stringify(
                              template.config,
                              null,
                              2,
                            );
                            navigator.clipboard.writeText(json).then(
                              () =>
                                alert(
                                  "Configuration JSON copied to clipboard.",
                                ),
                              () => prompt("Copy this JSON:", json),
                            );
                          }}
                        >
                          Export JSON
                        </button>
                        <Show when={!template.is_builtin}>
                          <button
                            class="btn btn-danger-outline btn-sm"
                            onClick={() => handleDelete(template)}
                          >
                            Delete
                          </button>
                        </Show>
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>
        )}
      </Show>
    </div>
  );
};

export default Templates;
