import { type Component, Index, Show, createSignal } from "solid-js";
import { A } from "@solidjs/router";
import type {
  ConfigParameter,
  ConfigParameterType,
  OptionsFrom,
  OptionsSortOrder,
} from "../types/bindings";
import { useIntegrationStatus } from "../context/integrations";
import { useAuth } from "../context/auth";

interface Props {
  parameters: ConfigParameter[];
  onChange: (parameters: ConfigParameter[]) => void;
  defaultCollapsed?: boolean;
}

const PARAM_TYPES: {
  value: ConfigParameterType;
  label: string;
  description: string;
}[] = [
  { value: "string", label: "String", description: "Free-form text input" },
  { value: "number", label: "Number", description: "Numeric input" },
  { value: "boolean", label: "Boolean", description: "True/false toggle" },
  {
    value: "select",
    label: "Select",
    description: "Dropdown with predefined options",
  },
  {
    value: "github_release_tag",
    label: "GitHub Release Tag",
    description: "Dropdown of release tags from a GitHub repository",
  },
  {
    value: "curse_forge_file_version",
    label: "CurseForge File Version",
    description: "Dropdown of file versions from a CurseForge project",
  },
];

function blankParameter(index: number): ConfigParameter {
  return {
    name: `param_${index + 1}`,
    label: `Parameter ${index + 1}`,
    description: null,
    param_type: "string",
    default: null,
    required: false,
    options: [],
    regex: null,
    is_version: false,
    options_from: null,
    github_repo: null,
    curseforge_project_id: null,
  };
}

const ParameterDefinitionEditor: Component<Props> = (props) => {
  const integrations = useIntegrationStatus();
  const auth = useAuth();

  const handleChange = (index: number, updated: ConfigParameter) => {
    const next = [...props.parameters];
    next[index] = updated;
    props.onChange(next);
  };

  const handleRemove = (index: number) => {
    const next = [...props.parameters];
    next.splice(index, 1);
    props.onChange(next);
  };

  const handleAdd = () => {
    props.onChange([
      ...props.parameters,
      blankParameter(props.parameters.length),
    ]);
  };

  const handleMoveUp = (index: number) => {
    if (index <= 0) return;
    const next = [...props.parameters];
    [next[index - 1], next[index]] = [next[index], next[index - 1]];
    props.onChange(next);
  };

  const handleMoveDown = (index: number) => {
    if (index >= props.parameters.length - 1) return;
    const next = [...props.parameters];
    [next[index], next[index + 1]] = [next[index + 1], next[index]];
    props.onChange(next);
  };

  const handleDuplicate = (index: number) => {
    const original = props.parameters[index];
    const clone: ConfigParameter = JSON.parse(JSON.stringify(original));
    clone.name = `${clone.name}_copy`;
    clone.label = `${clone.label} (copy)`;
    const next = [...props.parameters];
    next.splice(index + 1, 0, clone);
    props.onChange(next);
  };

  const handleClearAll = () => {
    if (props.parameters.length === 0) return;
    if (!confirm(`Remove all ${props.parameters.length} parameter(s)?`)) return;
    props.onChange([]);
  };

  const patch = (index: number, updates: Partial<ConfigParameter>) => {
    handleChange(index, { ...props.parameters[index], ...updates });
  };

  const autoName = (label: string): string => {
    return (
      label
        .toLowerCase()
        .replace(/[^a-z0-9_]+/g, "_")
        .replace(/^_+|_+$/g, "")
        .replace(/_+/g, "_") || "param"
    );
  };

  const handleOptionsChange = (index: number, text: string) => {
    const options = text
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0);
    patch(index, { options });
  };

  const [collapsed, setCollapsed] = createSignal(
    props.defaultCollapsed ?? props.parameters.length === 0,
  );

  return (
    <div
      class="parameter-def-editor"
      classList={{ "pipeline-editor--collapsed": collapsed() }}
    >
      {/* ─── Header (clickable to expand/collapse) ─── */}
      <div
        class="pipeline-editor-header pipeline-editor-header--clickable"
        onClick={() => setCollapsed(!collapsed())}
      >
        <div class="pipeline-editor-header-left">
          <span class="pipeline-editor-chevron">{collapsed() ? "▶" : "▼"}</span>
          <h3 style={{ margin: "0" }}>Template Parameters</h3>
          <span class="pipeline-step-count">
            {props.parameters.length} parameter
            {props.parameters.length !== 1 ? "s" : ""}
          </span>
        </div>
        <div
          class="pipeline-editor-header-actions"
          onClick={(e) => e.stopPropagation()}
        >
          <Show when={!collapsed()}>
            <Show when={props.parameters.length > 0}>
              <button
                class="btn btn-sm btn-danger-outline"
                onClick={handleClearAll}
                title="Remove all parameters"
              >
                Clear All
              </button>
            </Show>
          </Show>
        </div>
      </div>

      {/* ─── Body (collapsible) ─── */}
      <Show when={!collapsed()}>
        <div class="pipeline-editor-body">
          <p class="pipeline-editor-description">
            Define template parameters that users fill in when creating a
            server. Reference them anywhere in your config or pipeline steps as{" "}
            <code style={{ "font-size": "0.8rem" }}>{"${param_name}"}</code>.
          </p>

          {/* ─── Parameter List ─── */}
          <Show
            when={props.parameters.length > 0}
            fallback={
              <div class="pipeline-editor-empty">
                <p>No parameters defined yet.</p>
                <p style={{ "font-size": "0.8rem", color: "#6b7280" }}>
                  Click "Add Parameter" to start defining template variables.
                </p>
              </div>
            }
          >
            <div class="parameter-def-list">
              {/*
               * Using <Index> instead of <For> is critical here.
               *
               * <For> tracks items by reference identity. When patch() creates
               * a new object for the edited parameter, <For> sees the old
               * reference disappear and a new one appear, so it destroys and
               * recreates the DOM for that item — which kills input focus.
               *
               * <Index> tracks by *position*. Each index gets a stable DOM node,
               * and when the item at that index changes, only the reactive
               * bindings update. The DOM element (and its focus state) is preserved.
               */}
              <Index each={props.parameters}>
                {(param, index) => {
                  const [cardCollapsed, setCardCollapsed] = createSignal(true);
                  return (
                    <div
                      class="parameter-def-card"
                      classList={{
                        "parameter-def-card--collapsed": cardCollapsed(),
                      }}
                    >
                      {/* ─── Card Header (clickable to expand/collapse) ─── */}
                      <div
                        class="parameter-def-card-header parameter-def-card-header--clickable"
                        onClick={() => setCardCollapsed(!cardCollapsed())}
                      >
                        <div class="parameter-def-card-header-left">
                          <span class="pipeline-step-number">{index + 1}</span>
                          <span class="pipeline-step-chevron">
                            {cardCollapsed() ? "▶" : "▼"}
                          </span>
                          <span class="parameter-def-name-preview">
                            {"${"}
                            {param().name || "..."}
                            {"}"}
                          </span>
                          <Show when={param().required}>
                            <span class="parameter-def-required-badge">
                              required
                            </span>
                          </Show>
                          <Show when={cardCollapsed()}>
                            <span class="pipeline-step-type-badge">
                              {param().param_type}
                            </span>
                            <Show when={param().default != null}>
                              <span class="pipeline-step-type-badge">
                                = {param().default}
                              </span>
                            </Show>
                          </Show>
                        </div>
                        <div
                          class="pipeline-step-header-actions"
                          onClick={(e) => e.stopPropagation()}
                        >
                          <Show when={index > 0}>
                            <button
                              class="btn btn-sm"
                              onClick={() => handleMoveUp(index)}
                              title="Move up"
                            >
                              ↑
                            </button>
                          </Show>
                          <Show when={index < props.parameters.length - 1}>
                            <button
                              class="btn btn-sm"
                              onClick={() => handleMoveDown(index)}
                              title="Move down"
                            >
                              ↓
                            </button>
                          </Show>
                          <button
                            class="btn btn-sm"
                            onClick={() => handleDuplicate(index)}
                            title="Duplicate parameter"
                          >
                            ⧉
                          </button>
                          <button
                            class="btn btn-sm btn-danger-outline"
                            onClick={() => handleRemove(index)}
                            title="Remove parameter"
                          >
                            ✕
                          </button>
                        </div>
                      </div>

                      {/* ─── Card Body (collapsible) ─── */}
                      <Show when={!cardCollapsed()}>
                        <div class="parameter-def-card-body">
                          {/* Row: Label + Name */}
                          <div class="form-row">
                            <div class="form-group">
                              <label>Label *</label>
                              <input
                                type="text"
                                value={param().label}
                                onInput={(e) => {
                                  const label = e.currentTarget.value;
                                  // Auto-update name if it matches the auto-generated pattern
                                  const currentAutoName = autoName(
                                    param().label,
                                  );
                                  const shouldAutoName =
                                    param().name === currentAutoName ||
                                    param().name === "" ||
                                    param().name === `param_${index + 1}`;
                                  const updates: Partial<ConfigParameter> = {
                                    label,
                                  };
                                  if (shouldAutoName) {
                                    updates.name = autoName(label);
                                  }
                                  patch(index, updates);
                                }}
                                placeholder="Server Version"
                              />
                              <small>
                                Human-readable label shown in forms.
                              </small>
                            </div>
                            <div class="form-group">
                              <label>Name (key) *</label>
                              <input
                                type="text"
                                value={param().name}
                                onInput={(e) =>
                                  patch(index, { name: e.currentTarget.value })
                                }
                                placeholder="server_version"
                              />
                              <small>
                                Machine key. Use as{" "}
                                <code style={{ "font-size": "0.75rem" }}>
                                  {"${"}
                                  {param().name || "name"}
                                  {"}"}
                                </code>
                              </small>
                            </div>
                          </div>

                          {/* Description */}
                          <div class="form-group">
                            <label>Description</label>
                            <input
                              type="text"
                              value={param().description ?? ""}
                              onInput={(e) =>
                                patch(index, {
                                  description: e.currentTarget.value || null,
                                })
                              }
                              placeholder="Optional help text for the user"
                            />
                          </div>

                          {/* Row: Type + Default */}
                          <div class="form-row">
                            <div class="form-group">
                              <label>Type</label>
                              <select
                                value={param().param_type}
                                onChange={(e) => {
                                  const newType = e.currentTarget
                                    .value as ConfigParameterType;
                                  const updates: Partial<ConfigParameter> = {
                                    param_type: newType,
                                  };
                                  // Reset options when switching away from select
                                  if (
                                    newType !== "select" &&
                                    param().options.length > 0
                                  ) {
                                    updates.options = [];
                                  }
                                  // Reset regex when switching away from string
                                  if (newType !== "string" && param().regex) {
                                    updates.regex = null;
                                  }
                                  // Reset github_repo when switching away
                                  if (
                                    newType !== "github_release_tag" &&
                                    param().github_repo != null
                                  ) {
                                    updates.github_repo = null;
                                  }
                                  // Reset curseforge_project_id when switching away
                                  if (
                                    newType !== "curse_forge_file_version" &&
                                    param().curseforge_project_id != null
                                  ) {
                                    updates.curseforge_project_id = null;
                                  }
                                  // Auto-set required for CurseForge and GitHub params
                                  if (
                                    newType === "curse_forge_file_version" ||
                                    newType === "github_release_tag"
                                  ) {
                                    updates.required = true;
                                  }
                                  patch(index, updates);
                                }}
                              >
                                {PARAM_TYPES.map((t) => (
                                  <option value={t.value}>{t.label}</option>
                                ))}
                              </select>
                              <small>
                                {
                                  PARAM_TYPES.find(
                                    (t) => t.value === param().param_type,
                                  )?.description
                                }
                              </small>
                            </div>
                            <div class="form-group">
                              <label>Default Value</label>
                              <Show
                                when={param().param_type !== "boolean"}
                                fallback={
                                  <select
                                    value={param().default ?? "false"}
                                    onChange={(e) =>
                                      patch(index, {
                                        default: e.currentTarget.value,
                                      })
                                    }
                                  >
                                    <option value="true">true</option>
                                    <option value="false">false</option>
                                  </select>
                                }
                              >
                                <Show
                                  when={
                                    param().param_type !== "select" ||
                                    param().options.length === 0
                                  }
                                  fallback={
                                    <select
                                      value={param().default ?? ""}
                                      onChange={(e) =>
                                        patch(index, {
                                          default:
                                            e.currentTarget.value || null,
                                        })
                                      }
                                    >
                                      <option value="">— none —</option>
                                      {param().options.map((opt) => (
                                        <option value={opt}>{opt}</option>
                                      ))}
                                    </select>
                                  }
                                >
                                  <input
                                    type={
                                      param().param_type === "number"
                                        ? "number"
                                        : "text"
                                    }
                                    value={param().default ?? ""}
                                    onInput={(e) =>
                                      patch(index, {
                                        default: e.currentTarget.value || null,
                                      })
                                    }
                                    placeholder="(no default)"
                                  />
                                </Show>
                              </Show>
                            </div>
                          </div>

                          {/* Required checkbox */}
                          <label class="checkbox-label">
                            <input
                              type="checkbox"
                              checked={param().required}
                              onChange={(e) =>
                                patch(index, {
                                  required: e.currentTarget.checked,
                                })
                              }
                            />
                            Required — user must provide a value
                          </label>

                          {/* Is-version checkbox */}
                          <label
                            class="checkbox-label"
                            style={{ "margin-top": "0.25rem" }}
                          >
                            <input
                              type="checkbox"
                              checked={param().is_version}
                              onChange={(e) =>
                                patch(index, {
                                  is_version: e.currentTarget.checked,
                                })
                              }
                            />
                            Version parameter — drives update detection
                          </label>

                          {/* ─── Select options (only when type is select) ─── */}
                          <Show when={param().param_type === "select"}>
                            <div
                              class="form-group"
                              style={{ "margin-top": "0.75rem" }}
                            >
                              <label>Options (one per line)</label>
                              <textarea
                                value={param().options.join("\n")}
                                onInput={(e) =>
                                  handleOptionsChange(
                                    index,
                                    e.currentTarget.value,
                                  )
                                }
                                placeholder={"option_a\noption_b\noption_c"}
                                rows={4}
                              />
                              <small>
                                Each line becomes a static option in the
                                dropdown.
                                {param().options.length > 0 &&
                                  ` Currently: ${param().options.length} option(s).`}{" "}
                                You can also configure dynamic options below.
                              </small>
                            </div>
                          </Show>

                          {/* ─── Dynamic Options (options_from) ─── */}
                          <div
                            class="form-group"
                            style={{ "margin-top": "0.75rem" }}
                          >
                            <label class="checkbox-label">
                              <input
                                type="checkbox"
                                checked={param().options_from != null}
                                onChange={(e) => {
                                  if (e.currentTarget.checked) {
                                    patch(index, {
                                      options_from: {
                                        url: "",
                                        path: null,
                                        value_key: null,
                                        label_key: null,
                                        sort: null,
                                        limit: null,
                                        cache_secs: null,
                                      } as OptionsFrom,
                                    });
                                  } else {
                                    patch(index, { options_from: null });
                                  }
                                }}
                              />
                              Enable dynamic options from API
                            </label>
                            <small>
                              Fetch dropdown options from an external JSON API
                              at runtime. The user clicks "Load options" to
                              trigger the fetch.
                            </small>
                          </div>

                          <Show when={param().options_from != null}>
                            {(() => {
                              const of = () => param().options_from!;
                              const patchOf = (
                                updates: Partial<OptionsFrom>,
                              ) => {
                                patch(index, {
                                  options_from: { ...of(), ...updates },
                                });
                              };

                              return (
                                <div
                                  style={{
                                    "margin-top": "0.5rem",
                                    padding: "0.75rem",
                                    background: "rgba(99, 102, 241, 0.06)",
                                    "border-radius": "0.5rem",
                                    border:
                                      "1px solid rgba(99, 102, 241, 0.15)",
                                  }}
                                >
                                  <div
                                    style={{
                                      "font-size": "0.8rem",
                                      "font-weight": "600",
                                      color: "#94a3b8",
                                      "margin-bottom": "0.5rem",
                                    }}
                                  >
                                    🌐 API Fetch Configuration
                                  </div>

                                  {/* URL */}
                                  <div class="form-group">
                                    <label>API URL *</label>
                                    <input
                                      type="text"
                                      value={of().url}
                                      onInput={(e) =>
                                        patchOf({ url: e.currentTarget.value })
                                      }
                                      placeholder="https://api.papermc.io/v2/projects/paper"
                                    />
                                    <small>
                                      The URL to GET. Use {"{{param_name}}"} for
                                      variable substitution from other parameter
                                      values.
                                    </small>
                                  </div>

                                  {/* Path */}
                                  <div class="form-group">
                                    <label>JSON Path</label>
                                    <input
                                      type="text"
                                      value={of().path ?? ""}
                                      onInput={(e) =>
                                        patchOf({
                                          path: e.currentTarget.value || null,
                                        })
                                      }
                                      placeholder="versions (dot-separated, e.g. data.items)"
                                    />
                                    <small>
                                      Dot-separated path to the array in the
                                      JSON response. Leave empty if the root is
                                      an array.
                                    </small>
                                  </div>

                                  {/* Value Key + Label Key */}
                                  <div
                                    style={{
                                      display: "grid",
                                      "grid-template-columns": "1fr 1fr",
                                      gap: "0.75rem",
                                    }}
                                  >
                                    <div class="form-group">
                                      <label>Value Key</label>
                                      <input
                                        type="text"
                                        value={of().value_key ?? ""}
                                        onInput={(e) =>
                                          patchOf({
                                            value_key:
                                              e.currentTarget.value || null,
                                          })
                                        }
                                        placeholder="id"
                                      />
                                      <small>
                                        Object key for the option value. Leave
                                        empty for string arrays.
                                      </small>
                                    </div>
                                    <div class="form-group">
                                      <label>Label Key</label>
                                      <input
                                        type="text"
                                        value={of().label_key ?? ""}
                                        onInput={(e) =>
                                          patchOf({
                                            label_key:
                                              e.currentTarget.value || null,
                                          })
                                        }
                                        placeholder="display_name"
                                      />
                                      <small>
                                        Object key for the display label.
                                        Defaults to value key.
                                      </small>
                                    </div>
                                  </div>

                                  {/* Sort + Limit + Cache */}
                                  <div
                                    style={{
                                      display: "grid",
                                      "grid-template-columns": "1fr 1fr 1fr",
                                      gap: "0.75rem",
                                    }}
                                  >
                                    <div class="form-group">
                                      <label>Sort</label>
                                      <select
                                        value={of().sort ?? ""}
                                        onChange={(e) =>
                                          patchOf({
                                            sort: (e.currentTarget.value ||
                                              null) as OptionsSortOrder | null,
                                          })
                                        }
                                      >
                                        <option value="">
                                          None (API order)
                                        </option>
                                        <option value="asc">Ascending</option>
                                        <option value="desc">Descending</option>
                                      </select>
                                    </div>
                                    <div class="form-group">
                                      <label>Limit</label>
                                      <input
                                        type="number"
                                        value={of().limit ?? ""}
                                        onInput={(e) => {
                                          const v = parseInt(
                                            e.currentTarget.value,
                                            10,
                                          );
                                          patchOf({
                                            limit: isNaN(v) ? null : v,
                                          });
                                        }}
                                        placeholder="25"
                                        min="1"
                                      />
                                    </div>
                                    <div class="form-group">
                                      <label>Cache (secs)</label>
                                      <input
                                        type="number"
                                        value={of().cache_secs ?? ""}
                                        onInput={(e) => {
                                          const v = parseInt(
                                            e.currentTarget.value,
                                            10,
                                          );
                                          patchOf({
                                            cache_secs: isNaN(v) ? null : v,
                                          });
                                        }}
                                        placeholder="300"
                                        min="0"
                                      />
                                    </div>
                                  </div>
                                </div>
                              );
                            })()}
                          </Show>

                          {/* ─── Regex (only when type is string) ─── */}

                          {/* ─── GitHub Repo (only when type is github_release_tag) ─── */}
                          <Show
                            when={param().param_type === "github_release_tag"}
                          >
                            <div
                              class="form-group"
                              style={{ "margin-top": "0.75rem" }}
                            >
                              <label>GitHub Repository</label>
                              {/* Integration status warning */}
                              <Show
                                when={!integrations.status().github_configured}
                              >
                                <div
                                  style={{
                                    display: "flex",
                                    "align-items": "flex-start",
                                    gap: "0.4rem",
                                    padding: "0.5rem 0.65rem",
                                    background: "rgba(250, 204, 21, 0.06)",
                                    border: "1px solid rgba(250, 204, 21, 0.2)",
                                    "border-radius": "0.375rem",
                                    "margin-bottom": "0.5rem",
                                    "font-size": "0.8rem",
                                    color: "#fde68a",
                                  }}
                                >
                                  <span style={{ "flex-shrink": "0" }}>ℹ️</span>
                                  <div>
                                    <strong>No GitHub token configured.</strong>{" "}
                                    Public repos work fine — private repos and
                                    higher rate limits require a token.
                                    <Show when={auth.isAdmin()}>
                                      {" "}
                                      <A
                                        href="/admin"
                                        style={{
                                          color: "#facc15",
                                          "text-decoration": "underline",
                                        }}
                                        onClick={() =>
                                          sessionStorage.setItem(
                                            "admin_tab",
                                            "github",
                                          )
                                        }
                                      >
                                        Configure →
                                      </A>
                                    </Show>
                                  </div>
                                </div>
                              </Show>
                              <input
                                type="text"
                                value={param().github_repo ?? ""}
                                onInput={(e) => {
                                  const v =
                                    e.currentTarget.value.trim() || null;
                                  patch(index, { github_repo: v });
                                }}
                                placeholder="owner/repo (e.g. PaperMC/Paper)"
                              />
                              <small>
                                GitHub repository in <code>owner/repo</code>{" "}
                                format. Users will see a dropdown of release
                                tags from this repository when creating a
                                server.
                              </small>
                            </div>
                          </Show>

                          {/* ─── CurseForge Project ID (only when type is curseforge_file_version) ─── */}
                          <Show
                            when={
                              param().param_type === "curse_forge_file_version"
                            }
                          >
                            <div
                              class="form-group"
                              style={{ "margin-top": "0.75rem" }}
                            >
                              <label>CurseForge Project ID</label>
                              {/* Integration status warning */}
                              <Show
                                when={
                                  !integrations.status().curseforge_configured
                                }
                              >
                                <div
                                  style={{
                                    display: "flex",
                                    "align-items": "flex-start",
                                    gap: "0.4rem",
                                    padding: "0.5rem 0.65rem",
                                    background: "rgba(249, 115, 22, 0.08)",
                                    border: "1px solid rgba(249, 115, 22, 0.3)",
                                    "border-radius": "0.375rem",
                                    "margin-bottom": "0.5rem",
                                    "font-size": "0.8rem",
                                    color: "#fdba74",
                                  }}
                                >
                                  <span style={{ "flex-shrink": "0" }}>🔶</span>
                                  <div>
                                    <strong>
                                      CurseForge API key not configured.
                                    </strong>{" "}
                                    Users will not be able to load file versions
                                    or download server packs until an admin
                                    configures the API key.
                                    <Show
                                      when={auth.isAdmin()}
                                      fallback={
                                        <span>
                                          {" "}
                                          Ask an admin to configure it in Admin
                                          Panel → CurseForge.
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
                                          sessionStorage.setItem(
                                            "admin_tab",
                                            "curseforge",
                                          )
                                        }
                                      >
                                        Configure CurseForge API key →
                                      </A>
                                    </Show>
                                  </div>
                                </div>
                              </Show>
                              <input
                                type="number"
                                value={
                                  param().curseforge_project_id?.toString() ??
                                  ""
                                }
                                onInput={(e) => {
                                  const v = e.currentTarget.value;
                                  const n = v ? parseInt(v, 10) : null;
                                  patch(index, {
                                    curseforge_project_id:
                                      n != null && !isNaN(n) ? n : null,
                                  });
                                }}
                                placeholder="e.g. 857790"
                              />
                              <small>
                                The numeric project ID from CurseForge (visible
                                in the project page URL). Users will see a
                                dropdown of available file versions from this
                                project when creating a server.
                              </small>
                            </div>
                          </Show>

                          <Show when={param().param_type === "string"}>
                            <div
                              class="form-group"
                              style={{ "margin-top": "0.75rem" }}
                            >
                              <label>Regex Validation</label>
                              <input
                                type="text"
                                value={param().regex ?? ""}
                                onInput={(e) =>
                                  patch(index, {
                                    regex: e.currentTarget.value || null,
                                  })
                                }
                                placeholder="^[a-zA-Z0-9._-]+$ (optional)"
                              />
                              <small>
                                If set, the user's value must match this
                                pattern.
                              </small>
                            </div>
                          </Show>
                        </div>
                      </Show>
                    </div>
                  );
                }}
              </Index>
            </div>
          </Show>

          {/* ─── Add Button ─── */}
          <div class="pipeline-editor-add">
            <button class="btn btn-primary" onClick={handleAdd}>
              + Add Parameter
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default ParameterDefinitionEditor;
