import { type Component, For, Show } from "solid-js";

export interface KillProcessEntry {
  pid: number;
  command: string;
  args: string[];
}

export interface KillProcessesDialogProps {
  /** The list of processes to display. */
  processes: KillProcessEntry[];
  /** Whether the kill operation is currently in progress. */
  killing: boolean;
  /** Called when the user confirms the kill. */
  onConfirm: () => void;
  /** Called when the user cancels the dialog. */
  onCancel: () => void;
}

const KillProcessesDialog: Component<KillProcessesDialogProps> = (props) => {
  const processCount = () => props.processes.length;
  const pluralSuffix = () => (processCount() !== 1 ? "es" : "");

  return (
    <div class="modal-overlay" onClick={props.onCancel}>
      <div
        class="modal-content kill-processes-dialog"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div class="kill-processes-header">
          <h3 class="kill-processes-title">
            ⚠️ Kill {processCount()} Process{pluralSuffix()}?
          </h3>
          <button
            class="chmod-close"
            onClick={props.onCancel}
            title="Close"
            disabled={props.killing}
          >
            ✕
          </button>
        </div>

        <p class="kill-processes-description">
          The following OS processes are running inside this server's data
          directory. Killing them is{" "}
          <strong class="kill-processes-warn">irreversible</strong> and may cause
          data loss.
        </p>

        {/* Process list */}
        <div class="kill-processes-table-wrapper">
          <table class="kill-processes-table">
            <thead>
              <tr class="kill-processes-table-header">
                <th class="kill-processes-th">PID</th>
                <th class="kill-processes-th">Command</th>
                <th class="kill-processes-th">Arguments</th>
              </tr>
            </thead>
            <tbody>
              <For each={props.processes}>
                {(proc) => (
                  <tr class="kill-processes-row">
                    <td class="kill-processes-pid">{proc.pid}</td>
                    <td class="kill-processes-command">{proc.command}</td>
                    <td class="kill-processes-args">{proc.args.join(" ")}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>

        {/* Actions */}
        <div class="kill-processes-actions">
          <button
            class="btn"
            onClick={props.onCancel}
            disabled={props.killing}
          >
            Cancel
          </button>
          <button
            class="btn btn-danger"
            onClick={props.onConfirm}
            disabled={props.killing}
          >
            <Show when={props.killing} fallback={"💀"}>
              <span class="btn-spinner" />
            </Show>{" "}
            Kill {processCount()} Process{pluralSuffix()}
          </button>
        </div>
      </div>
    </div>
  );
};

export default KillProcessesDialog;
