import { type Component, createSignal, Show, For } from "solid-js";
import type {
  ServerConfig,
  ConfigParameter,
  FetchedOption,
} from "../../types/bindings";
import { fetchOptions } from "../../api/templates";
import SearchableSelect from "../SearchableSelect";

interface Props {
  config: ServerConfig;
  hasParameters: boolean;
  parameterValues: Record<string, string>;
  onParamChange: (name: string, value: string) => void;
}

// Client-side cache shared within the review step.
const optionsCache = new Map<
  string,
  { options: FetchedOption[]; fetchedAt: number; cacheSecs: number }
>();

function cacheKey(
  url: string,
  path: string | null | undefined,
  params: Record<string, string> | undefined,
): string {
  return JSON.stringify({ url, path: path ?? null, params: params ?? {} });
}

const WizardReviewStep: Component<Props> = (props) => {
  const [dynamicOptions, setDynamicOptions] = createSignal<
    Record<string, FetchedOption[]>
  >({});
  const [loadingOptions, setLoadingOptions] = createSignal<
    Record<string, boolean>
  >({});
  const [optionErrors, setOptionErrors] = createSignal<Record<string, string>>(
    {},
  );

  const buildSubstitutionParams = (
    excludeName: string,
  ): Record<string, string> => {
    const vals = props.parameterValues;
    const result: Record<string, string> = {};
    for (const p of props.config.parameters ?? []) {
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
    const key = cacheKey(of.url, of.path, subs);

    // Check client-side cache.
    const cached = optionsCache.get(key);
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
      optionsCache.delete(key);
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
        optionsCache.set(key, {
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
    const value = () =>
      props.parameterValues[param.name] ?? param.default ?? "";
    const hasOptionsFrom = () => param.options_from != null;
    const isLoadingOpt = () => loadingOptions()[param.name] ?? false;
    const optError = () => optionErrors()[param.name];
    const hasDynamic = () => (dynamicOptions()[param.name]?.length ?? 0) > 0;

    switch (param.param_type) {
      case "boolean":
        return (
          <label class="checkbox-label">
            <input
              type="checkbox"
              checked={value() === "true"}
              onChange={(e) =>
                props.onParamChange(
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
                props.onParamChange(param.name, e.currentTarget.value)
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
                onChange={(v) => props.onParamChange(param.name, v)}
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
        // "string" or unrecognized — may still have options_from
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
                      props.onParamChange(param.name, e.currentTarget.value)
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
                  onChange={(v) => props.onParamChange(param.name, v)}
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

  return (
    <div class="wizard-step-content">
      {/* Configuration Summary */}
      <div class="wizard-review-section">
        <h4>📋 Server Info</h4>
        <div class="wizard-review-grid">
          <div class="wizard-review-item">
            <span class="wizard-review-label">Name</span>
            <span class="wizard-review-value">{props.config.name || "—"}</span>
          </div>
          <div class="wizard-review-item">
            <span class="wizard-review-label">Binary</span>
            <span class="wizard-review-value">
              <code>{props.config.binary || "—"}</code>
            </span>
          </div>
          <Show when={props.config.args.length > 0}>
            <div class="wizard-review-item">
              <span class="wizard-review-label">Arguments</span>
              <span class="wizard-review-value">
                <code>{props.config.args.join(" ")}</code>
              </span>
            </div>
          </Show>
          <div class="wizard-review-item">
            <span class="wizard-review-label">Working Dir</span>
            <span class="wizard-review-value">
              {props.config.working_dir ? (
                <code>{props.config.working_dir}</code>
              ) : (
                <span style={{ color: "#22c55e" }}>
                  Server instance directory (default)
                </span>
              )}
            </span>
          </div>
          <Show when={props.config.stop_command}>
            <div class="wizard-review-item">
              <span class="wizard-review-label">Stop Command</span>
              <span class="wizard-review-value">
                <code>{props.config.stop_command}</code>
              </span>
            </div>
          </Show>
          <div class="wizard-review-item">
            <span class="wizard-review-label">Auto-start</span>
            <span class="wizard-review-value">
              {props.config.auto_start ? "Yes" : "No"}
            </span>
          </div>
          <div class="wizard-review-item">
            <span class="wizard-review-label">Auto-restart</span>
            <span class="wizard-review-value">
              {props.config.auto_restart ? "Yes" : "No"}
            </span>
          </div>
        </div>
      </div>

      {/* Pipeline Summary */}
      <div class="wizard-review-section">
        <h4>🔧 Pipelines & Parameters</h4>
        <div class="wizard-review-grid">
          <div class="wizard-review-item">
            <span class="wizard-review-label">Parameters</span>
            <span class="wizard-review-value">
              {props.config.parameters.length} defined
            </span>
          </div>
          <div class="wizard-review-item">
            <span class="wizard-review-label">Install Steps</span>
            <span class="wizard-review-value">
              {props.config.install_steps.length} step
              {props.config.install_steps.length !== 1 ? "s" : ""}
            </span>
          </div>
          <div class="wizard-review-item">
            <span class="wizard-review-label">Update Steps</span>
            <span class="wizard-review-value">
              {props.config.update_steps.length} step
              {props.config.update_steps.length !== 1 ? "s" : ""}
            </span>
          </div>
        </div>

        <Show when={props.config.install_steps.length > 0}>
          <div class="wizard-review-step-list">
            <strong style={{ "font-size": "0.8rem", color: "#94a3b8" }}>
              Install:
            </strong>
            <For each={props.config.install_steps}>
              {(step, i) => (
                <span class="wizard-review-step-tag">
                  {i() + 1}. {step.name}{" "}
                  <span style={{ opacity: 0.6 }}>({step.action.type})</span>
                </span>
              )}
            </For>
          </div>
        </Show>

        <Show when={props.config.update_steps.length > 0}>
          <div class="wizard-review-step-list">
            <strong style={{ "font-size": "0.8rem", color: "#94a3b8" }}>
              Update:
            </strong>
            <For each={props.config.update_steps}>
              {(step, i) => (
                <span class="wizard-review-step-tag">
                  {i() + 1}. {step.name}{" "}
                  <span style={{ opacity: 0.6 }}>({step.action.type})</span>
                </span>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* SFTP Summary */}
      <Show when={props.config.sftp_username}>
        <div class="wizard-review-section">
          <h4>🔒 SFTP</h4>
          <div class="wizard-review-grid">
            <div class="wizard-review-item">
              <span class="wizard-review-label">Username</span>
              <span class="wizard-review-value">
                {props.config.sftp_username}
              </span>
            </div>
            <div class="wizard-review-item">
              <span class="wizard-review-label">Password</span>
              <span class="wizard-review-value">••••••</span>
            </div>
          </div>
        </div>
      </Show>

      {/* Parameter Values */}
      <Show when={props.hasParameters}>
        <div class="wizard-review-section">
          <h4>📝 Parameter Values</h4>
          <p
            style={{
              "font-size": "0.85rem",
              color: "#9ca3af",
              "margin-bottom": "1rem",
            }}
          >
            Fill in the values for the template parameters you defined. These
            will be substituted as{" "}
            <code style={{ "font-size": "0.8rem" }}>{"${name}"}</code>{" "}
            throughout the configuration.
          </p>
          <For each={props.config.parameters}>
            {(param) => renderParameterInput(param)}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default WizardReviewStep;
