import { type Component, Index, Show, createSignal } from "solid-js";
import type { PipelineStep, StepAction } from "../types/bindings";
import PipelineStepEditor from "./PipelineStepEditor";

interface Props {
  label: string;
  description?: string;
  steps: PipelineStep[];
  onChange: (steps: PipelineStep[]) => void;
  parameterNames?: string[];
  defaultCollapsed?: boolean;
}

function defaultAction(): StepAction {
  return {
    type: "download",
    url: "",
    destination: ".",
    filename: null,
    executable: false,
  };
}

function blankStep(index: number): PipelineStep {
  return {
    name: `Step ${index + 1}`,
    description: null,
    action: defaultAction(),
    condition: null,
    continue_on_error: false,
  };
}

const PipelineEditor: Component<Props> = (props) => {
  const [collapsed, setCollapsed] = createSignal(
    props.defaultCollapsed ?? props.steps.length === 0,
  );

  const handleStepChange = (index: number, updated: PipelineStep) => {
    const next = [...props.steps];
    next[index] = updated;
    props.onChange(next);
  };

  const handleRemove = (index: number) => {
    const next = [...props.steps];
    next.splice(index, 1);
    props.onChange(next);
  };

  const handleAdd = () => {
    props.onChange([...props.steps, blankStep(props.steps.length)]);
  };

  const handleMoveUp = (index: number) => {
    if (index <= 0) return;
    const next = [...props.steps];
    [next[index - 1], next[index]] = [next[index], next[index - 1]];
    props.onChange(next);
  };

  const handleMoveDown = (index: number) => {
    if (index >= props.steps.length - 1) return;
    const next = [...props.steps];
    [next[index], next[index + 1]] = [next[index + 1], next[index]];
    props.onChange(next);
  };

  const handleDuplicateStep = (index: number) => {
    const original = props.steps[index];
    const clone: PipelineStep = JSON.parse(JSON.stringify(original));
    clone.name = `${clone.name} (copy)`;
    const next = [...props.steps];
    next.splice(index + 1, 0, clone);
    props.onChange(next);
  };

  const handleClearAll = () => {
    if (props.steps.length === 0) return;
    if (
      !confirm(`Remove all ${props.steps.length} step(s) from ${props.label}?`)
    )
      return;
    props.onChange([]);
  };

  const handleImportSteps = () => {
    const input = prompt(
      "Paste pipeline steps JSON (array of PipelineStep objects):",
    );
    if (!input) return;
    try {
      const parsed = JSON.parse(input);
      if (!Array.isArray(parsed)) {
        alert("Expected a JSON array of pipeline steps.");
        return;
      }
      // Basic validation: each item should have at least a name and action
      for (let i = 0; i < parsed.length; i++) {
        if (!parsed[i].action || !parsed[i].action.type) {
          alert(
            `Step at index ${i} is missing a valid 'action' with a 'type'.`,
          );
          return;
        }
        // Default missing fields
        if (!parsed[i].name) {
          parsed[i].name = `Imported Step ${i + 1}`;
        }
        if (parsed[i].description === undefined) {
          parsed[i].description = null;
        }
        if (parsed[i].condition === undefined) {
          parsed[i].condition = null;
        }
        if (parsed[i].continue_on_error === undefined) {
          parsed[i].continue_on_error = false;
        }
      }
      props.onChange([...props.steps, ...parsed]);
      // Auto-expand so user sees the imported steps
      setCollapsed(false);
    } catch {
      alert("Invalid JSON. Please check the format and try again.");
    }
  };

  const handleExportSteps = () => {
    if (props.steps.length === 0) {
      alert("No steps to export.");
      return;
    }
    const json = JSON.stringify(props.steps, null, 2);
    navigator.clipboard.writeText(json).then(
      () => alert(`${props.steps.length} step(s) copied to clipboard as JSON.`),
      () => prompt("Copy this JSON:", json),
    );
  };

  return (
    <div
      class="pipeline-editor"
      classList={{ "pipeline-editor--collapsed": collapsed() }}
    >
      <div
        class="pipeline-editor-header pipeline-editor-header--clickable"
        onClick={() => setCollapsed(!collapsed())}
      >
        <div class="pipeline-editor-header-left">
          <span class="pipeline-editor-chevron">{collapsed() ? "▶" : "▼"}</span>
          <h3 style={{ margin: "0" }}>{props.label}</h3>
          <span class="pipeline-step-count">
            {props.steps.length} step
            {props.steps.length !== 1 ? "s" : ""}
          </span>
        </div>
        <div
          class="pipeline-editor-header-actions"
          onClick={(e) => e.stopPropagation()}
        >
          <Show when={!collapsed()}>
            <button
              class="btn btn-sm"
              onClick={handleImportSteps}
              title="Import steps from JSON"
            >
              Import
            </button>
            <Show when={props.steps.length > 0}>
              <button
                class="btn btn-sm"
                onClick={handleExportSteps}
                title="Export steps as JSON"
              >
                Export
              </button>
              <button
                class="btn btn-sm btn-danger-outline"
                onClick={handleClearAll}
                title="Remove all steps"
              >
                Clear All
              </button>
            </Show>
          </Show>
        </div>
      </div>

      <Show when={!collapsed()}>
        <div class="pipeline-editor-body">
          <Show when={props.description}>
            <p class="pipeline-editor-description">{props.description}</p>
          </Show>

          {/* <Index> (not <For>) so editing a step doesn't destroy/recreate the component and kill focus. */}
          <Show
            when={props.steps.length > 0}
            fallback={
              <div class="pipeline-editor-empty">
                <p>No steps defined yet.</p>
                <p style={{ "font-size": "0.8rem", color: "#6b7280" }}>
                  Click "Add Step" to start building your pipeline.
                </p>
              </div>
            }
          >
            <div class="pipeline-step-list">
              <Index each={props.steps}>
                {(step, index) => (
                  <PipelineStepEditor
                    step={step()}
                    index={index}
                    onChange={(updated) => handleStepChange(index, updated)}
                    onRemove={() => handleRemove(index)}
                    onMoveUp={index > 0 ? () => handleMoveUp(index) : undefined}
                    onMoveDown={
                      index < props.steps.length - 1
                        ? () => handleMoveDown(index)
                        : undefined
                    }
                    defaultCollapsed={true}
                    parameterNames={props.parameterNames}
                  />
                )}
              </Index>
            </div>
          </Show>

          <div class="pipeline-editor-add">
            <button class="btn btn-primary" onClick={handleAdd}>
              + Add Step
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default PipelineEditor;
