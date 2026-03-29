import { type Component, createMemo, Show, For } from "solid-js";
import { validateParamRefs, BUILTIN_PARAMS } from "../utils/paramUtils";

interface Props {
  value: string | null | undefined;
  parameterNames: string[];
  showAvailable?: boolean;
}

const ParamRefHint: Component<Props> = (props) => {
  const unknown = createMemo(() => {
    const text = props.value;
    if (!text) return [];
    return validateParamRefs(text, props.parameterNames);
  });

  const hasUserParams = createMemo(() => props.parameterNames.length > 0);

  const availableList = createMemo(() => {
    const parts: string[] = [];
    for (const name of props.parameterNames) {
      parts.push(name);
    }
    return parts;
  });

  return (
    <>
      <Show when={unknown().length > 0}>
        <div class="param-ref-hint param-ref-hint-error">
          <span class="param-ref-hint-icon">⚠️</span>
          <span>
            Unknown parameter{unknown().length > 1 ? "s" : ""}:{" "}
            <For each={unknown()}>
              {(name, i) => (
                <>
                  <code class="param-ref-hint-name param-ref-hint-name-bad">
                    {"${"}
                    {name}
                    {"}"}
                  </code>
                  {i() < unknown().length - 1 ? ", " : ""}
                </>
              )}
            </For>
            <Show
              when={hasUserParams()}
              fallback={
                <span class="param-ref-hint-tip">
                  {" "}
                  — No parameters defined yet. Go to the Parameters step to
                  create them.
                </span>
              }
            >
              <span class="param-ref-hint-tip">
                {" "}
                — Did you mean one of:{" "}
                <For each={availableList()}>
                  {(name, i) => (
                    <>
                      <code class="param-ref-hint-name param-ref-hint-name-ok">
                        {"${"}
                        {name}
                        {"}"}
                      </code>
                      {i() < availableList().length - 1 ? ", " : ""}
                    </>
                  )}
                </For>
                ?
              </span>
            </Show>
          </span>
        </div>
      </Show>

      <Show
        when={props.showAvailable && unknown().length === 0 && hasUserParams()}
      >
        <div class="param-ref-hint param-ref-hint-info">
          <span class="param-ref-hint-icon">💡</span>
          <span>
            Available parameters:{" "}
            <For each={availableList()}>
              {(name, i) => (
                <>
                  <code class="param-ref-hint-name param-ref-hint-name-ok">
                    {"${"}
                    {name}
                    {"}"}
                  </code>
                  {i() < availableList().length - 1 ? ", " : ""}
                </>
              )}
            </For>
            {[...BUILTIN_PARAMS].length > 0 && (
              <span style={{ opacity: 0.7 }}>
                {" "}
                + built-in:{" "}
                <For each={[...BUILTIN_PARAMS]}>
                  {(name, i) => (
                    <>
                      <code class="param-ref-hint-name">
                        {"${"}
                        {name}
                        {"}"}
                      </code>
                      {i() < [...BUILTIN_PARAMS].length - 1 ? ", " : ""}
                    </>
                  )}
                </For>
              </span>
            )}
          </span>
        </div>
      </Show>
    </>
  );
};

export default ParamRefHint;
