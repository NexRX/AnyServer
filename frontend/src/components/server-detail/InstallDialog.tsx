import { type Component, Show } from "solid-js";

export interface InstallDialogProps {
  /** Whether the current user has admin/owner permissions to mark as installed. */
  canMarkInstalled: boolean;
  /** Called when the user chooses to run the install pipeline. */
  onInstall: () => void;
  /** Called when the user chooses to mark the server as already installed. */
  onMarkInstalled: () => void;
  /** Called when the user cancels the dialog. */
  onCancel: () => void;
}

const InstallDialog: Component<InstallDialogProps> = (props) => {
  return (
    <div
      class="modal-overlay"
      data-testid="install-dialog"
      onClick={(e) => {
        if (e.target === e.currentTarget) props.onCancel();
      }}
    >
      <div class="modal-content install-dialog-content">
        <h3 class="install-dialog-title">Server Not Installed</h3>
        <p class="install-dialog-message">
          This server has an install pipeline that hasn't been run yet. How would
          you like to proceed?
        </p>
        <div class="install-dialog-actions">
          <button
            class="btn btn-success install-dialog-btn"
            onClick={props.onInstall}
          >
            📦 Install
          </button>
          <Show when={props.canMarkInstalled}>
            <button
              class="btn btn-warning install-dialog-btn"
              onClick={props.onMarkInstalled}
            >
              ✓ Mark as Installed
            </button>
          </Show>
          <button
            class="btn install-dialog-btn"
            onClick={props.onCancel}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
};

export default InstallDialog;
