import { type Component, createSignal, Show, For, type JSX } from "solid-js";
import type { ConfigParameter, FetchedOption } from "../types/bindings";
import { fetchOptions } from "../api/templates";
import SearchableSelect from "./SearchableSelect";

interface ParameterEditorProps {
  parameters: ConfigParameter[];
  values: Record<string, string>;
  onSave: (values: Record<string, string>) => void;
  title?: string;
  /** When true, renders without the outer `<div class="config-editor">` wrapper
   *  so the content can be embedded inside another config-editor (e.g. ConfigEditor's children slot). */
  bare?: boolean;
}

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

const ParameterEditor: Component<ParameterEditorProps> = (props) => {
  const [localValues, setLocalValues] = createSignal<Record<string, string>>({
    ...props.values,
  });
  const [saved, setSaved] = createSignal(false);

  // Track loading / error / loaded-options state per parameter name.
  const [loadingParams, setLoadingParams] = createSignal<
    Record<string, boolean>
  >({});
  const [paramErrors, setParamErrors] = createSignal<Record<string, string>>(
    {},
  );
  const [dynamicOptions, setDynamicOptions] = createSignal<
    Record<string, FetchedOption[]>
  >({});

  const seeded = () => {
    const vals = { ...localValues() };
    for (const p of props.parameters) {
      if (!(p.name in vals) && p.default != null) {
        vals[p.name] = p.default;
      }
    }
    return vals;
  };

  const handleChange = (name: string, value: string) => {
    setLocalValues((prev) => ({ ...prev, [name]: value }));
    setSaved(false);
  };

  const handleSave = () => {
    props.onSave(seeded());
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const buildSubstitutionParams = (
    excludeName: string,
  ): Record<string, string> => {
    const vals = seeded();
    const result: Record<string, string> = {};
    for (const p of props.parameters) {
      if (p.name !== excludeName && vals[p.name]) {
        result[p.name] = vals[p.name];
      }
    }
    return result;
  };

  const handleLoadOptions = async (param: ConfigParameter) => {
    if (!param.options_from) return;
    const of = param.options_from;

    const substitutionParams = buildSubstitutionParams(param.name);
    const key = cacheKey(of.url, of.path, substitutionParams);
    const cached = optionsCache.get(key);
    if (cached) {
      const age = (Date.now() - cached.fetchedAt) / 1000;
      if (age < cached.cacheSecs) {
        setDynamicOptions((prev) => ({
          ...prev,
          [param.name]: cached.options,
        }));
        setParamErrors((prev) => {
          const next = { ...prev };
          delete next[param.name];
          return next;
        });
        return;
      }
      optionsCache.delete(key);
    }

    setLoadingParams((prev) => ({ ...prev, [param.name]: true }));
    setParamErrors((prev) => {
      const next = { ...prev };
      delete next[param.name];
      return next;
    });
    try {
      const resp = await fetchOptions({
        url: of.url,
        path: of.path,
        value_key: of.value_key,
        label_key: of.label_key,
        sort: of.sort,
        limit: of.limit,
        params: substitutionParams,
      });

      setDynamicOptions((prev) => ({
        ...prev,
        [param.name]: resp.options,
      }));

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
      setParamErrors((prev) => ({ ...prev, [param.name]: msg }));
    } finally {
      setLoadingParams((prev) => ({ ...prev, [param.name]: false }));
    }
  };

  const mergedOptions = (param: ConfigParameter): FetchedOption[] => {
    const staticOpts: FetchedOption[] = param.options.map((o) => ({
      value: o,
      label: o,
    }));
    const dynamic = dynamicOptions()[param.name];
    if (!dynamic || dynamic.length === 0) return staticOpts;

    if (staticOpts.length > 0) {
      const seen = new Set(staticOpts.map((o) => o.value));
      const deduped = dynamic.filter((d) => !seen.has(d.value));
      return [...staticOpts, ...deduped];
    }
    return dynamic;
  };

  const content = () => (
    <>
      <h3>{props.title ?? "Template Parameters"}</h3>
      <p
        style={{
          "font-size": "0.85rem",
          color: "#9ca3af",
          "margin-bottom": "1rem",
        }}
      >
        These values are substituted as{" "}
        <code style={{ "font-size": "0.8rem" }}>{"${name}"}</code> in the
        server's binary path, arguments, environment variables, pipeline steps,
        and file contents.
      </p>

      <For each={props.parameters}>
        {(param) => {
          const value = () => seeded()[param.name] ?? "";
          const isLoading = () => loadingParams()[param.name] ?? false;
          const errorMsg = () => paramErrors()[param.name];
          const hasDynamic = () =>
            (dynamicOptions()[param.name]?.length ?? 0) > 0;
          const hasOptionsFrom = () => param.options_from != null;

          const labelBlock = (
            <>
              {param.label}
              {param.required && " *"}
              <span
                style={{
                  "font-size": "0.75rem",
                  color: "#64748b",
                  "margin-left": "0.5rem",
                }}
              >
                ${"{" + param.name + "}"}
              </span>
              <Show when={param.is_version}>
                <span
                  style={{
                    "font-size": "0.65rem",
                    color: "#38bdf8",
                    background: "rgba(56, 189, 248, 0.1)",
                    border: "1px solid rgba(56, 189, 248, 0.3)",
                    "border-radius": "4px",
                    padding: "1px 6px",
                    "margin-left": "0.5rem",
                    "vertical-align": "middle",
                  }}
                  title="This parameter drives update detection"
                >
                  version
                </span>
              </Show>
            </>
          );

          if (param.param_type === "select" || hasOptionsFrom()) {
            const opts = () => mergedOptions(param);
            const showAsSelect = () =>
              param.param_type === "select" ||
              hasDynamic() ||
              (param.options && param.options.length > 0);

            return (
              <div class="form-group">
                <label>{labelBlock}</label>

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
                          handleChange(param.name, e.currentTarget.value)
                        }
                        placeholder={param.default ?? ""}
                        style={{ flex: "1" }}
                      />
                    }
                  >
                    <SearchableSelect
                      options={opts().map((opt) => ({
                        value: opt.value,
                        label:
                          opt.label !== opt.value
                            ? `${opt.label} (${opt.value})`
                            : opt.value,
                      }))}
                      value={value()}
                      onChange={(v) => handleChange(param.name, v)}
                      allowEmpty={!param.required}
                      emptyLabel="— none —"
                      placeholder="Select or search…"
                    />
                  </Show>

                  <Show when={hasOptionsFrom()}>
                    <button
                      class="btn btn-sm"
                      onClick={() => handleLoadOptions(param)}
                      disabled={isLoading()}
                      title="Fetch dropdown options from the external API defined in the template"
                      style={{
                        "white-space": "nowrap",
                        "min-width": "fit-content",
                      }}
                    >
                      {isLoading()
                        ? "Loading…"
                        : hasDynamic()
                          ? "↻ Refresh"
                          : "Load options"}
                    </button>
                  </Show>
                </div>

                <Show when={errorMsg()}>
                  <small style={{ color: "#ef4444" }}>{errorMsg()}</small>
                </Show>
                <Show when={param.description}>
                  <small>{param.description}</small>
                </Show>
              </div>
            );
          }

          if (param.param_type === "boolean") {
            return (
              <div class="form-group">
                <label class="checkbox-label">
                  <input
                    type="checkbox"
                    checked={value() === "true"}
                    onChange={(e) =>
                      handleChange(
                        param.name,
                        e.currentTarget.checked ? "true" : "false",
                      )
                    }
                  />
                  {labelBlock}
                </label>
                <Show when={param.description}>
                  <small>{param.description}</small>
                </Show>
              </div>
            );
          }

          return (
            <div class="form-group">
              <label>{labelBlock}</label>
              <input
                type={param.param_type === "number" ? "number" : "text"}
                value={value()}
                onInput={(e) => handleChange(param.name, e.currentTarget.value)}
                placeholder={param.default ?? ""}
              />
              <Show when={param.description}>
                <small>{param.description}</small>
              </Show>
            </div>
          );
        }}
      </For>

      <div
        style={{
          "margin-top": "1.5rem",
          display: "flex",
          "align-items": "center",
          gap: "0.75rem",
        }}
      >
        <button class="btn btn-primary" onClick={handleSave}>
          Save Parameters
        </button>
        <Show when={saved()}>
          <span style={{ color: "#22c55e", "font-size": "0.85rem" }}>
            ✓ Saved
          </span>
        </Show>
      </div>
    </>
  );

  return (
    <Show when={!props.bare} fallback={content()}>
      <div class="config-editor">{content()}</div>
    </Show>
  );
};

export default ParameterEditor;
