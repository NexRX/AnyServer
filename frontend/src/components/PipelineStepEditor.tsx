import { type Component, Show, createSignal, createEffect } from "solid-js";
import type {
  PipelineStep,
  StepAction,
  StepCondition,
} from "../types/bindings";
import StepActionEditor from "./StepActionEditor";
import ParamRefHint from "./ParamRefHint";

interface Props {
  step: PipelineStep;
  index: number;
  onChange: (step: PipelineStep) => void;
  onRemove: () => void;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  defaultCollapsed?: boolean;
  parameterNames?: string[];
}

const PipelineStepEditor: Component<Props> = (props) => {
  const [collapsed, setCollapsed] = createSignal(
    props.defaultCollapsed ?? true,
  );
  const [showConditions, setShowConditions] = createSignal(
    props.step.condition !== null,
  );

  createEffect(() => {
    if (props.step.condition !== null) {
      setShowConditions(true);
    }
  });

  const patch = (updates: Partial<PipelineStep>) => {
    props.onChange({ ...props.step, ...updates });
  };

  const handleActionChange = (action: StepAction) => {
    patch({ action });
  };

  const handleConditionToggle = (enabled: boolean) => {
    setShowConditions(enabled);
    if (!enabled) {
      patch({ condition: null });
    } else {
      patch({
        condition: { path_exists: null, path_not_exists: null },
      });
    }
  };

  const patchCondition = (updates: Partial<StepCondition>) => {
    const current: StepCondition = props.step.condition ?? {
      path_exists: null,
      path_not_exists: null,
    };
    patch({ condition: { ...current, ...updates } });
  };

  const actionTypeLabel = (type: string): string => {
    const labels: Record<string, string> = {
      download: "Download",
      extract: "Extract",
      move: "Move",
      copy: "Copy",
      delete: "Delete",
      create_dir: "Create Dir",
      run_command: "Run Command",
      write_file: "Write File",
      edit_file: "Edit File",
      set_permissions: "Set Permissions",
      glob: "Glob",
    };
    return labels[type] ?? type;
  };

  const stepTitle = () => {
    const name = props.step.name.trim();
    if (name) return name;
    return `Step ${props.index + 1}`;
  };

  const paramNames = () => props.parameterNames ?? [];

  return (
    <div class="pipeline-step-editor" classList={{ collapsed: collapsed() }}>
      <div
        class="pipeline-step-header"
        onClick={() => setCollapsed(!collapsed())}
      >
        <div class="pipeline-step-header-left">
          <span class="pipeline-step-number">{props.index + 1}</span>
          <span class="pipeline-step-chevron">{collapsed() ? "▶" : "▼"}</span>
          <span class="pipeline-step-title">{stepTitle()}</span>
          <span class="pipeline-step-type-badge">
            {actionTypeLabel(props.step.action.type)}
          </span>
        </div>
        <div
          class="pipeline-step-header-actions"
          onClick={(e) => e.stopPropagation()}
        >
          <Show when={props.onMoveUp}>
            <button class="btn btn-sm" onClick={props.onMoveUp} title="Move up">
              ↑
            </button>
          </Show>
          <Show when={props.onMoveDown}>
            <button
              class="btn btn-sm"
              onClick={props.onMoveDown}
              title="Move down"
            >
              ↓
            </button>
          </Show>
          <button
            class="btn btn-sm btn-danger-outline"
            onClick={props.onRemove}
            title="Remove step"
          >
            ✕
          </button>
        </div>
      </div>

      <Show when={!collapsed()}>
        <div class="pipeline-step-body">
          <div class="form-group">
            <label>Step Name *</label>
            <input
              type="text"
              value={props.step.name}
              onInput={(e) => patch({ name: e.currentTarget.value })}
              placeholder={`Step ${props.index + 1}`}
            />
            <small>A short, human-readable label shown in progress UI.</small>
          </div>

          <div class="form-group">
            <label>Description</label>
            <input
              type="text"
              value={props.step.description ?? ""}
              onInput={(e) =>
                patch({ description: e.currentTarget.value || null })
              }
              placeholder="Optional description of what this step does"
            />
          </div>

          <div class="pipeline-step-section">
            <h4>Action</h4>
            <StepActionEditor
              action={props.step.action}
              onChange={handleActionChange}
              parameterNames={paramNames()}
            />
          </div>

          <div class="pipeline-step-section">
            <label class="checkbox-label">
              <input
                type="checkbox"
                checked={showConditions()}
                onChange={(e) => handleConditionToggle(e.currentTarget.checked)}
              />
              <h4 style={{ margin: "0" }}>Conditions</h4>
            </label>

            <Show when={showConditions()}>
              <div class="pipeline-step-conditions">
                <small
                  style={{
                    display: "block",
                    color: "#9ca3af",
                    "margin-bottom": "0.75rem",
                  }}
                >
                  If specified, all conditions must be true for this step to
                  run. Paths are relative to the server data directory.
                </small>

                <div class="form-group">
                  <label>Only run if path exists</label>
                  <input
                    type="text"
                    value={props.step.condition?.path_exists ?? ""}
                    onInput={(e) =>
                      patchCondition({
                        path_exists: e.currentTarget.value || null,
                      })
                    }
                    placeholder="e.g. server.jar"
                  />
                  <ParamRefHint
                    value={props.step.condition?.path_exists}
                    parameterNames={paramNames()}
                  />
                </div>

                <div class="form-group">
                  <label>Only run if path does NOT exist</label>
                  <input
                    type="text"
                    value={props.step.condition?.path_not_exists ?? ""}
                    onInput={(e) =>
                      patchCondition({
                        path_not_exists: e.currentTarget.value || null,
                      })
                    }
                    placeholder="e.g. .installed"
                  />
                  <ParamRefHint
                    value={props.step.condition?.path_not_exists}
                    parameterNames={paramNames()}
                  />
                </div>
              </div>
            </Show>
          </div>

          <label class="checkbox-label" style={{ "margin-top": "0.5rem" }}>
            <input
              type="checkbox"
              checked={props.step.continue_on_error}
              onChange={(e) =>
                patch({ continue_on_error: e.currentTarget.checked })
              }
            />
            Continue pipeline if this step fails
          </label>
        </div>
      </Show>
    </div>
  );
};

export default PipelineStepEditor;
