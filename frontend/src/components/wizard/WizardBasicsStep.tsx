import { type Component, Show } from "solid-js";
import type { ServerConfig } from "../../types/bindings";
import ParamRefHint from "../ParamRefHint";

interface Props {
  config: ServerConfig;
  useDefaultWorkDir: boolean;
  parameterNames: string[];
  onPatchConfig: (updates: Partial<ServerConfig>) => void;
  onSetUseDefaultWorkDir: (value: boolean) => void;
}

const WizardBasicsStep: Component<Props> = (props) => {
  return (
    <div class="wizard-step-content">
      <div class="form-group">
        <label for="wiz-name">Server Name *</label>
        <input
          id="wiz-name"
          type="text"
          value={props.config.name}
          onInput={(e) => props.onPatchConfig({ name: e.currentTarget.value })}
          placeholder="My Game Server"
        />
      </div>

      <div class="form-group">
        <label>Working Directory</label>
        <label class="checkbox-label" style={{ "margin-bottom": "0.5rem" }}>
          <input
            type="checkbox"
            checked={props.useDefaultWorkDir}
            onChange={(e) => {
              const useDefault = e.currentTarget.checked;
              props.onSetUseDefaultWorkDir(useDefault);
              if (useDefault) {
                props.onPatchConfig({ working_dir: null });
              }
            }}
          />
          Use server instance directory (recommended)
        </label>
        <Show when={props.useDefaultWorkDir}>
          <small>
            Each server gets its own isolated data directory automatically. All
            relative paths in your config will resolve within it.
          </small>
        </Show>
        <Show when={!props.useDefaultWorkDir}>
          <input
            type="text"
            value={props.config.working_dir ?? ""}
            onInput={(e) =>
              props.onPatchConfig({
                working_dir: e.currentTarget.value || null,
              })
            }
            placeholder="/absolute/path or relative/to/server/dir"
          />
          <small>
            Override the working directory for the server process. Can be
            absolute or relative to the server's data directory.
          </small>
          <ParamRefHint
            value={props.config.working_dir}
            parameterNames={props.parameterNames}
          />
        </Show>
      </div>
    </div>
  );
};

export default WizardBasicsStep;
