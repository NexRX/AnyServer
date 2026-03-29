import { type Component, createSignal } from "solid-js";

export interface SaveTemplateDialogProps {
  /** Whether the save operation is currently in progress. */
  submitting: boolean;
  /** Called when the user submits the form. */
  onSave: (name: string, description: string) => void;
  /** Called when the user cancels. */
  onCancel: () => void;
}

const SaveTemplateDialog: Component<SaveTemplateDialogProps> = (props) => {
  const [name, setName] = createSignal("");
  const [desc, setDesc] = createSignal("");

  const handleSave = () => {
    const trimmedName = name().trim();
    if (!trimmedName) return;
    props.onSave(trimmedName, desc().trim());
  };

  return (
    <div class="template-save-dialog">
      <h3>Save as Template</h3>
      <p class="template-save-dialog-description">
        Save this server's configuration as a reusable template. Parameters,
        install steps, and update steps will all be included.
      </p>
      <div class="form-group">
        <label>Template Name *</label>
        <input
          type="text"
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          placeholder="e.g. Minecraft Paper Server"
        />
      </div>
      <div class="form-group">
        <label>Description</label>
        <input
          type="text"
          value={desc()}
          onInput={(e) => setDesc(e.currentTarget.value)}
          placeholder="Short description of what this template sets up"
        />
      </div>
      <div class="template-save-dialog-actions">
        <button
          class="btn btn-primary"
          onClick={handleSave}
          disabled={props.submitting || !name().trim()}
        >
          {props.submitting ? "Saving..." : "Save Template"}
        </button>
        <button
          class="btn"
          onClick={props.onCancel}
        >
          Cancel
        </button>
      </div>
    </div>
  );
};

export default SaveTemplateDialog;
