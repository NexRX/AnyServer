import { type Component, createSignal, createMemo, Show, For } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { createServer } from "../api/client";
import type { ServerConfig } from "../types/bindings";
import PipelineEditor from "./PipelineEditor";
import ParameterDefinitionEditor from "./ParameterDefinitionEditor";
import { WIZARD_STEPS, defaultConfig } from "./wizard/types";
import WizardBasicsStep from "./wizard/WizardBasicsStep";
import WizardStartStep from "./wizard/WizardStartStep";
import WizardReviewStep from "./wizard/WizardReviewStep";

const WizardCreateServer: Component = () => {
  const navigate = useNavigate();

  const [currentStep, setCurrentStep] = createSignal(0);
  const [config, setConfig] = createSignal<ServerConfig>({ ...defaultConfig });
  const [parameterValues, setParameterValues] = createSignal<
    Record<string, string>
  >({});
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  const [argsText, setArgsText] = createSignal("");
  const [envText, setEnvText] = createSignal("");
  const [useDefaultWorkDir, setUseDefaultWorkDir] = createSignal(true);
  const [highestStepReached, setHighestStepReached] = createSignal(0);

  const currentStepDef = () => WIZARD_STEPS[currentStep()];
  const isFirstStep = () => currentStep() === 0;
  const isLastStep = () => currentStep() === WIZARD_STEPS.length - 1;

  const hasParameters = createMemo(
    () => (config().parameters?.length ?? 0) > 0,
  );

  const parameterNames = createMemo(() =>
    config()
      .parameters.map((p) => p.name)
      .filter((n) => n.trim() !== ""),
  );

  const patchConfig = (updates: Partial<ServerConfig>) => {
    setConfig((prev) => ({ ...prev, ...updates }));
    if (error()) setError(null);
  };

  const parseNumber = (value: string, fallback: number): number => {
    const n = parseInt(value, 10);
    return isNaN(n) || n < 0 ? fallback : n;
  };

  const goNext = () => {
    const err = validateCurrentStep();
    if (err) {
      setError(err);
      return;
    }
    setError(null);
    if (!isLastStep()) {
      const next = currentStep() + 1;
      setCurrentStep(next);
      if (next > highestStepReached()) {
        setHighestStepReached(next);
      }
    }
  };

  const goPrev = () => {
    setError(null);
    if (!isFirstStep()) {
      setCurrentStep((s) => s - 1);
    }
  };

  const goToStep = (index: number) => {
    if (index <= highestStepReached()) {
      setError(null);
      setCurrentStep(index);
    }
  };

  const validateCurrentStep = (): string | null => {
    const c = config();
    const step = currentStepDef().id;

    switch (step) {
      case "parameters": {
        const names = new Set<string>();
        for (const p of c.parameters) {
          if (!p.name.trim()) return `All parameters must have a name.`;
          if (names.has(p.name))
            return `Duplicate parameter name: "${p.name}".`;
          names.add(p.name);
          if (!p.label.trim())
            return `Parameter "${p.name}" must have a label.`;
          if (
            p.param_type === "select" &&
            p.options.length === 0 &&
            !p.options_from
          )
            return `Select parameter "${p.label}" needs at least one option or an options_from definition.`;
        }
        return null;
      }

      case "basics":
        if (!c.name.trim()) return "Server name is required.";
        if (
          !useDefaultWorkDir() &&
          c.working_dir &&
          c.working_dir.trim() === ""
        )
          return "Working directory path cannot be blank when custom directory is enabled.";
        return null;

      case "start":
        if (!c.binary.trim()) return "Binary path is required.";
        if (c.sftp_username && !c.sftp_password)
          return "SFTP password is required when a username is set.";
        return null;

      case "install":
      case "update": {
        const steps = step === "install" ? c.install_steps : c.update_steps;
        for (let i = 0; i < steps.length; i++) {
          if (!steps[i].name.trim()) return `Step ${i + 1} needs a name.`;
        }
        return null;
      }

      case "review": {
        const vals = parameterValues();
        for (const param of c.parameters) {
          if (param.required) {
            const val = vals[param.name];
            if (!val || val.trim() === "")
              return `Parameter "${param.label}" is required.`;
          }
          if (param.param_type === "select" && param.options.length > 0) {
            const val = vals[param.name];
            if (val && val.trim() !== "" && !param.options.includes(val))
              return `Parameter "${param.label}" must be one of: ${param.options.join(", ")}`;
          }
        }
        return null;
      }
    }
    return null;
  };

  const handleSubmit = async () => {
    const err = validateCurrentStep();
    if (err) {
      setError(err);
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const result = await createServer({
        config: config(),
        parameter_values: parameterValues(),
        source_template_id: null,
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

  const handleImportJson = () => {
    const input = prompt("Paste full server configuration JSON:");
    if (!input) return;

    try {
      const parsed = JSON.parse(input);
      const merged: ServerConfig = {
        ...defaultConfig,
        ...parsed,
        args: Array.isArray(parsed.args) ? parsed.args : defaultConfig.args,
        env:
          typeof parsed.env === "object" && parsed.env !== null
            ? parsed.env
            : defaultConfig.env,
        parameters: Array.isArray(parsed.parameters)
          ? parsed.parameters
          : defaultConfig.parameters,
        start_steps: Array.isArray(parsed.start_steps)
          ? parsed.start_steps
          : defaultConfig.start_steps,
        install_steps: Array.isArray(parsed.install_steps)
          ? parsed.install_steps
          : defaultConfig.install_steps,
        update_steps: Array.isArray(parsed.update_steps)
          ? parsed.update_steps
          : defaultConfig.update_steps,
        uninstall_steps: Array.isArray(parsed.uninstall_steps)
          ? parsed.uninstall_steps
          : defaultConfig.uninstall_steps,
      };
      setConfig(merged);
      setArgsText(merged.args.join(" "));
      setEnvText(
        Object.entries(merged.env)
          .map(([k, v]) => `${k}=${v}`)
          .join("\n"),
      );

      if (merged.working_dir) {
        setUseDefaultWorkDir(false);
      } else {
        setUseDefaultWorkDir(true);
      }

      const vals: Record<string, string> = {};
      for (const param of merged.parameters ?? []) {
        if (param.default != null) {
          vals[param.name] = param.default;
        }
      }
      setParameterValues(vals);
      setError(null);
    } catch {
      setError("Invalid JSON. Please check the format and try again.");
    }
  };

  const handleExportJson = () => {
    const json = JSON.stringify(config(), null, 2);
    navigator.clipboard.writeText(json).then(
      () => alert("Configuration JSON copied to clipboard."),
      () => prompt("Copy this JSON:", json),
    );
  };

  const handleParamChange = (name: string, value: string) => {
    setParameterValues((prev) => ({ ...prev, [name]: value }));
    if (error()) setError(null);
  };

  const renderParameters = () => (
    <div class="wizard-step-content">
      <ParameterDefinitionEditor
        parameters={config().parameters}
        defaultCollapsed={false}
        onChange={(parameters) => {
          patchConfig({ parameters });
          const currentVals = { ...parameterValues() };
          let changed = false;
          for (const p of parameters) {
            if (!(p.name in currentVals) && p.default != null) {
              currentVals[p.name] = p.default;
              changed = true;
            }
          }
          if (changed) setParameterValues(currentVals);
        }}
      />
      <Show when={config().parameters.length > 0}>
        <div
          style={{
            "margin-top": "1rem",
            padding: "0.75rem 1rem",
            background: "rgba(99, 102, 241, 0.06)",
            "border-radius": "0.5rem",
            "font-size": "0.85rem",
            color: "#94a3b8",
          }}
        >
          💡 You can reference these parameters in later steps using{" "}
          <code style={{ "font-size": "0.8rem" }}>{"${param_name}"}</code>.
          Unknown references will be highlighted with a warning.
        </div>
      </Show>
    </div>
  );

  const renderInstall = () => (
    <div class="wizard-step-content">
      <PipelineEditor
        label="Install Pipeline"
        description="Steps executed during first-time server setup. Commonly used to download server binaries, extract archives, write config files, etc."
        steps={config().install_steps}
        onChange={(install_steps) => patchConfig({ install_steps })}
        parameterNames={parameterNames()}
        parameters={config().parameters}
        defaultCollapsed={false}
      />
    </div>
  );

  const renderUpdate = () => (
    <div class="wizard-step-content">
      <PipelineEditor
        label="Update Pipeline"
        description="Steps executed when updating the server (e.g. download a new version, re-extract, apply patches)."
        steps={config().update_steps}
        onChange={(update_steps) => patchConfig({ update_steps })}
        parameterNames={parameterNames()}
        parameters={config().parameters}
        defaultCollapsed={false}
      />
    </div>
  );

  const renderCurrentStep = () => {
    switch (currentStepDef().id) {
      case "parameters":
        return renderParameters();
      case "basics":
        return (
          <WizardBasicsStep
            config={config()}
            useDefaultWorkDir={useDefaultWorkDir()}
            parameterNames={parameterNames()}
            onPatchConfig={patchConfig}
            onSetUseDefaultWorkDir={setUseDefaultWorkDir}
          />
        );
      case "start":
        return (
          <WizardStartStep
            config={config()}
            parameterNames={parameterNames()}
            argsText={argsText()}
            envText={envText()}
            onPatchConfig={patchConfig}
            onSetArgsText={setArgsText}
            onSetEnvText={setEnvText}
            parseNumber={parseNumber}
          />
        );
      case "install":
        return renderInstall();
      case "update":
        return renderUpdate();
      case "review":
        return (
          <WizardReviewStep
            config={config()}
            hasParameters={hasParameters()}
            parameterValues={parameterValues()}
            onParamChange={handleParamChange}
          />
        );
      default:
        return null;
    }
  };

  return (
    <div class="wizard-create-server">
      {/* Progress Stepper */}
      <div class="wizard-stepper">
        <For each={WIZARD_STEPS}>
          {(step, index) => (
            <div
              class="wizard-stepper-item"
              classList={{
                active: index() === currentStep(),
                completed: index() < currentStep(),
                clickable: index() <= highestStepReached(),
              }}
              onClick={() => goToStep(index())}
            >
              <div class="wizard-stepper-dot">
                <Show when={index() < currentStep()} fallback={index() + 1}>
                  ✓
                </Show>
              </div>
              <div class="wizard-stepper-label">
                <span class="wizard-stepper-icon">{step.icon}</span>
                <span class="wizard-stepper-text">{step.label}</span>
              </div>
            </div>
          )}
        </For>
      </div>

      {/* Step Header */}
      <div class="wizard-step-header">
        <div class="wizard-step-header-left">
          <h2>
            {currentStepDef().icon} {currentStepDef().label}
          </h2>
          <p class="wizard-step-description">{currentStepDef().description}</p>
        </div>
        <div class="wizard-step-header-actions">
          <button
            class="btn btn-sm"
            onClick={handleImportJson}
            title="Import full config JSON"
          >
            Import JSON
          </button>
          <button
            class="btn btn-sm"
            onClick={handleExportJson}
            title="Export current config as JSON"
          >
            Export JSON
          </button>
        </div>
      </div>

      <Show when={error()}>
        <div class="error-msg">{error()}</div>
      </Show>

      <div class="wizard-step-body-container">{renderCurrentStep()}</div>

      {/* Navigation Footer */}
      <div class="wizard-nav">
        <div class="wizard-nav-left">
          <button
            class="btn"
            onClick={() => navigate("/")}
            disabled={submitting()}
          >
            Cancel
          </button>
        </div>
        <div class="wizard-nav-right">
          <Show when={!isFirstStep()}>
            <button class="btn" onClick={goPrev} disabled={submitting()}>
              ← Previous
            </button>
          </Show>
          <Show
            when={!isLastStep()}
            fallback={
              <button
                class="btn btn-success"
                onClick={handleSubmit}
                disabled={submitting()}
              >
                {submitting() ? "Creating..." : "🚀 Create Server"}
              </button>
            }
          >
            <button class="btn btn-primary" onClick={goNext}>
              Next →
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
};

export default WizardCreateServer;
