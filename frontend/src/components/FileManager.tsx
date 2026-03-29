import {
  type Component,
  createSignal,
  createResource,
  For,
  Show,
  createEffect,
  on,
} from "solid-js";
import Loader from "./Loader";
import { formatDateTime } from "../utils/format";
import {
  listFiles,
  readFile,
  writeFile,
  createDir,
  deletePath,
  getFilePermissions,
  chmodFile,
} from "../api/client";
import type {
  FileEntry,
  FileEntryKind,
  FilePermissionsResponse,
} from "../types/bindings";

interface Props {
  serverId: string;
}

interface EditingFile {
  path: string;
  content: string;
  originalContent: string;
  size: number;
}

interface ChmodDialogState {
  path: string;
  name: string;
  isDirectory: boolean;
  loading: boolean;
  saving: boolean;
  error: string | null;
  mode: string;
  originalMode: string;
  modeDisplay: string;
  owner: string | null;
  group: string | null;
  uid: number;
  gid: number;
}

function octalToBits(octal: string): boolean[] {
  const val = parseInt(octal, 8) || 0;
  return [
    !!(val & 0o400), // owner read
    !!(val & 0o200), // owner write
    !!(val & 0o100), // owner exec
    !!(val & 0o040), // group read
    !!(val & 0o020), // group write
    !!(val & 0o010), // group exec
    !!(val & 0o004), // other read
    !!(val & 0o002), // other write
    !!(val & 0o001), // other exec
  ];
}

function bitsToOctal(bits: boolean[]): string {
  const bitValues = [
    0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
  ];
  let val = 0;
  for (let i = 0; i < 9; i++) {
    if (bits[i]) val |= bitValues[i];
  }
  return val.toString(8);
}

function octalToRwx(octal: string): string {
  const bits = octalToBits(octal);
  const chars = "rwxrwxrwx";
  return bits.map((on, i) => (on ? chars[i] : "-")).join("");
}

const PRESET_MODES: { label: string; mode: string; desc: string }[] = [
  { label: "755", mode: "755", desc: "Owner: full, Others: read+exec" },
  { label: "644", mode: "644", desc: "Owner: read+write, Others: read" },
  { label: "700", mode: "700", desc: "Owner: full, Others: none" },
  { label: "600", mode: "600", desc: "Owner: read+write, Others: none" },
  { label: "775", mode: "775", desc: "Owner+Group: full, Others: read+exec" },
  { label: "664", mode: "664", desc: "Owner+Group: read+write, Others: read" },
  { label: "777", mode: "777", desc: "Everyone: full access" },
];

const FileManager: Component<Props> = (props) => {
  const [currentPath, setCurrentPath] = createSignal("");
  const [editingFile, setEditingFile] = createSignal<EditingFile | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [chmodDialog, setChmodDialog] = createSignal<ChmodDialogState | null>(
    null,
  );

  const [files, { refetch }] = createResource(
    () => ({ serverId: props.serverId, path: currentPath() }),
    async (params) => {
      setError(null);
      try {
        return await listFiles(params.serverId, params.path || undefined);
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        setError(`Failed to list files: ${msg}`);
        return null;
      }
    },
  );

  createEffect(
    on(
      () => props.serverId,
      () => {
        setCurrentPath("");
        setEditingFile(null);
        setChmodDialog(null);
        setError(null);
      },
    ),
  );

  const navigateTo = (entry: FileEntry) => {
    setError(null);
    if (entry.kind === "directory") {
      setCurrentPath(entry.path);
      setEditingFile(null);
    } else {
      openFile(entry.path);
    }
  };

  const goUp = () => {
    const parts = currentPath().split("/").filter(Boolean);
    parts.pop();
    setCurrentPath(parts.join("/"));
    setEditingFile(null);
    setError(null);
  };

  const goToRoot = () => {
    setCurrentPath("");
    setEditingFile(null);
    setError(null);
  };

  const breadcrumbs = (): Array<{ label: string; path: string }> => {
    const parts = currentPath().split("/").filter(Boolean);
    const crumbs: Array<{ label: string; path: string }> = [
      { label: "root", path: "" },
    ];
    let accumulated = "";
    for (const part of parts) {
      accumulated = accumulated ? `${accumulated}/${part}` : part;
      crumbs.push({ label: part, path: accumulated });
    }
    return crumbs;
  };

  const openFile = async (path: string) => {
    setError(null);
    setLoading(true);
    try {
      const result = await readFile(props.serverId, path);
      setEditingFile({
        path: result.path,
        content: result.content,
        originalContent: result.content,
        size: result.size,
      });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Cannot open file: ${msg}`);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    const file = editingFile();
    if (!file) return;

    setSaving(true);
    setError(null);
    try {
      await writeFile(props.serverId, {
        path: file.path,
        content: file.content,
      });
      setEditingFile({
        ...file,
        originalContent: file.content,
        size: new Blob([file.content]).size,
      });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Save failed: ${msg}`);
    } finally {
      setSaving(false);
    }
  };

  const handleCloseEditor = () => {
    const file = editingFile();
    if (file && file.content !== file.originalContent) {
      if (!confirm("You have unsaved changes. Discard them?")) {
        return;
      }
    }
    setEditingFile(null);
    setError(null);
  };

  const handleNewDir = async () => {
    const name = prompt("New directory name:");
    if (!name || !name.trim()) return;

    const sanitized = name.trim().replace(/[/\\]/g, "_");
    const path = currentPath() ? `${currentPath()}/${sanitized}` : sanitized;

    setError(null);
    try {
      await createDir(props.serverId, { path });
      refetch();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Failed to create directory: ${msg}`);
    }
  };

  const handleNewFile = async () => {
    const name = prompt("New file name:");
    if (!name || !name.trim()) return;

    const sanitized = name.trim().replace(/[/\\]/g, "_");
    const path = currentPath() ? `${currentPath()}/${sanitized}` : sanitized;

    setError(null);
    try {
      await writeFile(props.serverId, { path, content: "" });
      refetch();
      openFile(path);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Failed to create file: ${msg}`);
    }
  };

  const handleDelete = async (entry: FileEntry, e: MouseEvent) => {
    e.stopPropagation();
    const typeLabel = entry.kind === "directory" ? "directory" : "file";
    if (
      !confirm(
        `Delete ${typeLabel} "${entry.name}"?${entry.kind === "directory" ? " This will delete all contents recursively." : ""}`,
      )
    ) {
      return;
    }

    setError(null);
    try {
      await deletePath(props.serverId, { path: entry.path });
      const editing = editingFile();
      if (editing && editing.path === entry.path) {
        setEditingFile(null);
      }
      refetch();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Delete failed: ${msg}`);
    }
  };

  const openPermissionsDialog = async (entry: FileEntry, e: MouseEvent) => {
    e.stopPropagation();
    const entryMode = entry.mode ?? "0";
    setChmodDialog({
      path: entry.path,
      name: entry.name,
      isDirectory: entry.kind === "directory",
      loading: true,
      saving: false,
      error: null,
      mode: entryMode,
      originalMode: entryMode,
      modeDisplay: octalToRwx(entryMode),
      owner: null,
      group: null,
      uid: 0,
      gid: 0,
    });

    try {
      const perms: FilePermissionsResponse = await getFilePermissions(
        props.serverId,
        entry.path,
      );
      setChmodDialog((prev) =>
        prev
          ? {
              ...prev,
              loading: false,
              mode: perms.mode,
              originalMode: perms.mode,
              modeDisplay: perms.mode_display,
              owner: perms.owner,
              group: perms.group,
              uid: perms.uid,
              gid: perms.gid,
              isDirectory: perms.is_directory,
            }
          : null,
      );
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setChmodDialog((prev) =>
        prev
          ? {
              ...prev,
              loading: false,
              error: `Failed to load permissions: ${msg}`,
            }
          : null,
      );
    }
  };

  const handleChmodSave = async () => {
    const dialog = chmodDialog();
    if (!dialog) return;

    const val = parseInt(dialog.mode, 8);
    if (
      isNaN(val) ||
      val < 0 ||
      val > 0o7777 ||
      !/^[0-7]{1,4}$/.test(dialog.mode)
    ) {
      setChmodDialog({
        ...dialog,
        error: "Invalid octal mode. Use 1-4 octal digits (e.g. 755).",
      });
      return;
    }

    setChmodDialog({ ...dialog, saving: true, error: null });
    try {
      const result = await chmodFile(props.serverId, {
        path: dialog.path,
        mode: dialog.mode,
      });
      setChmodDialog({
        ...dialog,
        saving: false,
        originalMode: result.mode,
        mode: result.mode,
        modeDisplay: result.mode_display,
      });
      refetch();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setChmodDialog((prev) =>
        prev ? { ...prev, saving: false, error: `Chmod failed: ${msg}` } : null,
      );
    }
  };

  const updateChmodBit = (bitIndex: number, value: boolean) => {
    const dialog = chmodDialog();
    if (!dialog) return;
    const bits = octalToBits(dialog.mode);
    bits[bitIndex] = value;
    const newMode = bitsToOctal(bits);
    setChmodDialog({
      ...dialog,
      mode: newMode,
      modeDisplay: octalToRwx(newMode),
      error: null,
    });
  };

  const updateChmodOctal = (value: string) => {
    const dialog = chmodDialog();
    if (!dialog) return;
    setChmodDialog({
      ...dialog,
      mode: value,
      modeDisplay: /^[0-7]{1,4}$/.test(value)
        ? octalToRwx(value)
        : dialog.modeDisplay,
      error: null,
    });
  };

  const setChmodPreset = (mode: string) => {
    const dialog = chmodDialog();
    if (!dialog) return;
    setChmodDialog({
      ...dialog,
      mode,
      modeDisplay: octalToRwx(mode),
      error: null,
    });
  };

  const closeChmodDialog = () => {
    setChmodDialog(null);
  };

  const chmodHasChanges = (): boolean => {
    const dialog = chmodDialog();
    return dialog !== null && dialog.mode !== dialog.originalMode;
  };

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024)
      return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  };

  const formatDate = formatDateTime;

  const fileIcon = (entry: FileEntry): string => {
    if (entry.kind === "directory") return "📁";
    const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
    const iconMap: Record<string, string> = {
      json: "📋",
      yaml: "📋",
      yml: "📋",
      toml: "📋",
      xml: "📋",
      txt: "📝",
      md: "📝",
      log: "📜",
      sh: "⚙️",
      bash: "⚙️",
      bat: "⚙️",
      cmd: "⚙️",
      js: "🟨",
      ts: "🔷",
      py: "🐍",
      rs: "🦀",
      jar: "☕",
      zip: "📦",
      tar: "📦",
      gz: "📦",
      png: "🖼️",
      jpg: "🖼️",
      jpeg: "🖼️",
      gif: "🖼️",
      svg: "🖼️",
    };
    return iconMap[ext] ?? "📄";
  };

  const hasUnsavedChanges = (): boolean => {
    const file = editingFile();
    return file !== null && file.content !== file.originalContent;
  };

  const handleEditorKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      handleSave();
    }
  };

  const renderChmodDialog = () => {
    const dialog = chmodDialog();
    if (!dialog) return null;

    const bits = () => octalToBits(chmodDialog()?.mode ?? "0");
    const labels = ["Read", "Write", "Execute"];
    const groups = ["Owner", "Group", "Other"];

    return (
      <div class="chmod-overlay" onClick={closeChmodDialog}>
        <div class="chmod-dialog" onClick={(e) => e.stopPropagation()}>
          <div class="chmod-header">
            <h3 style={{ margin: 0 }}>File Permissions</h3>
            <button
              class="chmod-close"
              onClick={closeChmodDialog}
              title="Close"
            >
              ✕
            </button>
          </div>

          <div class="chmod-file-info">
            <span class="chmod-file-icon">
              {dialog.isDirectory ? "📁" : "📄"}
            </span>
            <div>
              <div class="chmod-file-name">{dialog.name}</div>
              <div class="chmod-file-path">{dialog.path}</div>
            </div>
          </div>

          <Show when={dialog.error}>
            <div class="error-msg" style={{ "margin-bottom": "0.75rem" }}>
              {dialog.error}
            </div>
          </Show>

          <Show
            when={!dialog.loading}
            fallback={<Loader message="Loading permissions" compact />}
          >
            <div class="chmod-owner-info">
              <div class="chmod-owner-pair">
                <span class="chmod-owner-label">Owner</span>
                <span class="chmod-owner-value">
                  {dialog.owner ?? String(dialog.uid)}
                  <span class="chmod-uid">({dialog.uid})</span>
                </span>
              </div>
              <div class="chmod-owner-pair">
                <span class="chmod-owner-label">Group</span>
                <span class="chmod-owner-value">
                  {dialog.group ?? String(dialog.gid)}
                  <span class="chmod-uid">({dialog.gid})</span>
                </span>
              </div>
            </div>

            <div class="chmod-mode-row">
              <div class="chmod-octal-group">
                <label class="chmod-label">Octal</label>
                <input
                  type="text"
                  class="chmod-octal-input"
                  value={dialog.mode}
                  onInput={(e) => updateChmodOctal(e.currentTarget.value)}
                  maxLength={4}
                  spellcheck={false}
                />
              </div>
              <div class="chmod-display-group">
                <label class="chmod-label">Symbolic</label>
                <span class="chmod-symbolic">
                  {dialog.isDirectory ? "d" : "-"}
                  {dialog.modeDisplay}
                </span>
              </div>
            </div>

            <div class="chmod-matrix">
              <div class="chmod-matrix-header">
                <span class="chmod-matrix-corner"></span>
                <For each={labels}>
                  {(label) => (
                    <span class="chmod-matrix-col-label">{label}</span>
                  )}
                </For>
              </div>
              <For each={groups}>
                {(group, groupIdx) => (
                  <div class="chmod-matrix-row">
                    <span class="chmod-matrix-row-label">{group}</span>
                    <For each={labels}>
                      {(_label, colIdx) => {
                        const bitIndex = () => groupIdx() * 3 + colIdx();
                        return (
                          <label class="chmod-checkbox-cell">
                            <input
                              type="checkbox"
                              checked={bits()[bitIndex()]}
                              onChange={(e) =>
                                updateChmodBit(
                                  bitIndex(),
                                  e.currentTarget.checked,
                                )
                              }
                            />
                          </label>
                        );
                      }}
                    </For>
                  </div>
                )}
              </For>
            </div>

            <div class="chmod-presets">
              <span class="chmod-label">Quick presets</span>
              <div class="chmod-preset-buttons">
                <For each={PRESET_MODES}>
                  {(preset) => (
                    <button
                      class={`chmod-preset-btn ${dialog.mode === preset.mode ? "active" : ""}`}
                      onClick={() => setChmodPreset(preset.mode)}
                      title={preset.desc}
                    >
                      {preset.label}
                    </button>
                  )}
                </For>
              </div>
            </div>

            <div class="chmod-actions">
              <button
                class="btn btn-primary btn-sm"
                onClick={handleChmodSave}
                disabled={dialog.saving || !chmodHasChanges()}
              >
                {dialog.saving ? "Applying..." : "Apply"}
              </button>
              <button class="btn btn-sm" onClick={closeChmodDialog}>
                Cancel
              </button>
              <Show when={chmodHasChanges()}>
                <span style={{ color: "#eab308", "font-size": "0.75rem" }}>
                  (unsaved)
                </span>
              </Show>
            </div>
          </Show>
        </div>
      </div>
    );
  };

  return (
    <div class="file-manager">
      {renderChmodDialog()}

      <Show when={error()}>
        {(err) => (
          <div class="error-msg" style={{ "margin-bottom": "1rem" }}>
            {err()}
            <button
              style={{
                background: "none",
                border: "none",
                color: "inherit",
                cursor: "pointer",
                "margin-left": "0.5rem",
                "font-weight": "bold",
              }}
              onClick={() => setError(null)}
            >
              ✕
            </button>
          </div>
        )}
      </Show>

      <Show
        when={!editingFile()}
        fallback={
          <div class="file-editor">
            <div class="file-editor-header">
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "0.5rem",
                }}
              >
                <span class="file-path">{editingFile()!.path}</span>
                <Show when={hasUnsavedChanges()}>
                  <span
                    style={{
                      color: "#eab308",
                      "font-size": "0.75rem",
                      "font-weight": "600",
                    }}
                  >
                    (unsaved)
                  </span>
                </Show>
                <span
                  style={{
                    color: "#9ca3af",
                    "font-size": "0.75rem",
                  }}
                >
                  {formatSize(new Blob([editingFile()!.content]).size)}
                </span>
              </div>
              <div style={{ display: "flex", gap: "0.5rem" }}>
                <button
                  class="btn btn-primary btn-sm"
                  onClick={handleSave}
                  disabled={saving() || !hasUnsavedChanges()}
                >
                  {saving() ? "Saving..." : "Save"}
                </button>
                <button
                  class="btn btn-sm"
                  onClick={() => {
                    const file = editingFile()!;
                    setEditingFile({
                      ...file,
                      content: file.originalContent,
                    });
                  }}
                  disabled={!hasUnsavedChanges()}
                >
                  Revert
                </button>
                <button class="btn btn-sm" onClick={handleCloseEditor}>
                  Close
                </button>
              </div>
            </div>
            <textarea
              class="file-editor-content"
              value={editingFile()!.content}
              onInput={(e) => {
                const file = editingFile()!;
                setEditingFile({
                  ...file,
                  content: e.currentTarget.value,
                });
              }}
              onKeyDown={handleEditorKeyDown}
              spellcheck={false}
            />
            <div
              style={{
                "font-size": "0.75rem",
                color: "#6b7280",
                padding: "0.4rem 0",
                display: "flex",
                "justify-content": "space-between",
              }}
            >
              <span>
                Lines: {editingFile()!.content.split("\n").length} | Characters:{" "}
                {editingFile()!.content.length}
              </span>
              <span>Ctrl+S to save</span>
            </div>
          </div>
        }
      >
        <div class="file-toolbar">
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "0.5rem",
              "flex-wrap": "wrap",
            }}
          >
            <button
              class="btn btn-sm"
              onClick={goUp}
              disabled={!currentPath()}
              title="Go to parent directory"
            >
              ↑ Up
            </button>
            <button
              class="btn btn-sm"
              onClick={goToRoot}
              disabled={!currentPath()}
              title="Go to server root"
            >
              ⌂ Root
            </button>

            <nav
              class="breadcrumbs"
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.25rem",
              }}
            >
              <For each={breadcrumbs()}>
                {(crumb, index) => (
                  <>
                    <Show when={index() > 0}>
                      <span style={{ color: "#6b7280" }}>/</span>
                    </Show>
                    <button
                      class="breadcrumb-link"
                      onClick={() => {
                        setCurrentPath(crumb.path);
                        setError(null);
                      }}
                      style={{
                        background: "none",
                        border: "none",
                        color:
                          crumb.path === currentPath() ? "#e4e4e7" : "#6366f1",
                        cursor: "pointer",
                        padding: "0.1rem 0.25rem",
                        "font-family": "var(--mono)",
                        "font-size": "0.85rem",
                        "border-radius": "3px",
                        "font-weight":
                          crumb.path === currentPath() ? "600" : "400",
                      }}
                    >
                      {crumb.label}
                    </button>
                  </>
                )}
              </For>
            </nav>
          </div>

          <div style={{ display: "flex", gap: "0.4rem" }}>
            <button class="btn btn-sm" onClick={handleNewFile}>
              + File
            </button>
            <button class="btn btn-sm btn-primary" onClick={handleNewDir}>
              + Folder
            </button>
            <button class="btn btn-sm" onClick={() => refetch()}>
              ↻ Refresh
            </button>
          </div>
        </div>

        <Show when={files.loading && !files()}>
          <Loader message="Loading files" compact />
        </Show>

        <Show when={files()}>
          {(resolved) => (
            <Show
              when={resolved()!.entries.length > 0}
              fallback={
                <div
                  class="empty-state"
                  style={{ padding: "3rem", "text-align": "center" }}
                >
                  <p style={{ "margin-bottom": "1rem" }}>
                    This directory is empty.
                  </p>
                  <div
                    style={{
                      display: "flex",
                      gap: "0.5rem",
                      "justify-content": "center",
                    }}
                  >
                    <button class="btn btn-sm" onClick={handleNewFile}>
                      + Create File
                    </button>
                    <button
                      class="btn btn-sm btn-primary"
                      onClick={handleNewDir}
                    >
                      + Create Folder
                    </button>
                  </div>
                </div>
              }
            >
              <div class="file-list">
                <div class="file-list-header">
                  <span class="file-list-header-cell file-list-header-name">
                    Name
                  </span>
                  <span class="file-list-header-cell file-list-header-perms">
                    Permissions
                  </span>
                  <span class="file-list-header-cell file-list-header-date">
                    Modified
                  </span>
                  <span class="file-list-header-cell file-list-header-size">
                    Size
                  </span>
                  <span class="file-list-header-cell file-list-header-actions"></span>
                </div>
                <For each={resolved()!.entries}>
                  {(entry) => (
                    <div
                      class="file-entry"
                      onClick={() => navigateTo(entry)}
                      role="button"
                      tabIndex={0}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") navigateTo(entry);
                      }}
                    >
                      <span class="file-icon">{fileIcon(entry)}</span>
                      <span class="file-name" title={entry.path}>
                        {entry.name}
                      </span>
                      <Show
                        when={entry.mode !== null && entry.mode !== undefined}
                        fallback={<span class="file-mode file-mode-na">—</span>}
                      >
                        <button
                          class="file-mode"
                          onClick={(e) => openPermissionsDialog(entry, e)}
                          title={`Permissions: ${octalToRwx(entry.mode!)} (${entry.mode}) — click to change`}
                        >
                          <span class="file-mode-octal">{entry.mode}</span>
                          <span class="file-mode-rwx">
                            {octalToRwx(entry.mode!)}
                          </span>
                        </button>
                      </Show>
                      <span
                        class="file-date"
                        style={{
                          color: "#6b7280",
                          "font-size": "0.8rem",
                          "min-width": "100px",
                          "text-align": "right",
                        }}
                      >
                        {formatDate(entry.modified)}
                      </span>
                      <span class="file-size">
                        {entry.kind === "file" ? formatSize(entry.size) : "—"}
                      </span>
                      <button
                        class="btn btn-sm btn-danger-outline"
                        onClick={(e) => handleDelete(entry, e)}
                        title={`Delete ${entry.name}`}
                        style={{ "flex-shrink": "0" }}
                      >
                        ✕
                      </button>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          )}
        </Show>
      </Show>
    </div>
  );
};

export default FileManager;
