import {
  type Component,
  createSignal,
  createResource,
  For,
  Show,
} from "solid-js";
import Loader from "../Loader";
import {
  createInviteCode,
  listInviteCodes,
  deleteInviteCode,
  updateInvitePermissions,
  listServers,
} from "../../api/client";
import type {
  Role,
  InviteCodePublic,
  InviteExpiry,
  InvitePermissionGrant,
  PermissionLevel,
} from "../../types/bindings";

const EXPIRY_OPTIONS: { value: InviteExpiry; label: string }[] = [
  { value: "thirty_minutes", label: "30 minutes" },
  { value: "one_hour", label: "1 hour" },
  { value: "one_day", label: "1 day" },
  { value: "three_days", label: "3 days" },
  { value: "seven_days", label: "7 days" },
];

const PERMISSION_LEVELS: { value: PermissionLevel; label: string }[] = [
  { value: "viewer", label: "Viewer" },
  { value: "operator", label: "Operator" },
  { value: "manager", label: "Manager" },
  { value: "admin", label: "Admin" },
];

const InviteCodesTab: Component = () => {
  const [invites, { refetch }] = createResource(listInviteCodes);
  const [servers] = createResource(() =>
    listServers({ page: 1, per_page: 200, sort: "name", order: "asc" }),
  );
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);
  const [creating, setCreating] = createSignal(false);
  const [showCreate, setShowCreate] = createSignal(false);

  // Create form state
  const [expiry, setExpiry] = createSignal<InviteExpiry>("one_day");
  const [assignedRole, setAssignedRole] = createSignal<Role>("user");
  const [label, setLabel] = createSignal("");
  const [grants, setGrants] = createSignal<InvitePermissionGrant[]>([]);

  // Edit form state
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [editRole, setEditRole] = createSignal<Role>("user");
  const [editGrants, setEditGrants] = createSignal<InvitePermissionGrant[]>([]);

  const clearMessages = () => {
    setError(null);
    setSuccess(null);
  };

  const handleCreate = async () => {
    clearMessages();
    setCreating(true);
    try {
      const resp = await createInviteCode({
        expiry: expiry(),
        assigned_role: assignedRole(),
        assigned_permissions: grants(),
        assigned_capabilities: [],
        label: label() || null,
      });
      const displayCode =
        resp.invite.code.length === 8
          ? `${resp.invite.code.slice(0, 4)}-${resp.invite.code.slice(4)}`
          : resp.invite.code;
      setSuccess(
        `Invite code created: ${displayCode} — share this with the user!`,
      );
      setShowCreate(false);
      setLabel("");
      setGrants([]);
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to create invite code");
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("Delete this invite code?")) return;
    clearMessages();
    try {
      await deleteInviteCode(id);
      setSuccess("Invite code deleted");
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to delete");
    }
  };

  const startEdit = (invite: InviteCodePublic) => {
    setEditingId(invite.id);
    setEditRole(invite.assigned_role);
    setEditGrants([...invite.assigned_permissions]);
  };

  const handleUpdatePermissions = async () => {
    const id = editingId();
    if (!id) return;
    clearMessages();
    try {
      await updateInvitePermissions(id, {
        assigned_role: editRole(),
        assigned_permissions: editGrants(),
      });
      setSuccess("Invite permissions updated");
      setEditingId(null);
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to update permissions");
    }
  };

  const addGrant = (
    list: () => InvitePermissionGrant[],
    setList: (v: InvitePermissionGrant[]) => void,
  ) => {
    const serverList = servers()?.servers;
    if (!serverList || serverList.length === 0) return;
    const existing = new Set(list().map((g) => g.server_id));
    const first = serverList.find((s) => !existing.has(s.server.id));
    if (!first) return;
    setList([...list(), { server_id: first.server.id, level: "viewer" }]);
  };

  const removeGrant = (
    list: () => InvitePermissionGrant[],
    setList: (v: InvitePermissionGrant[]) => void,
    idx: number,
  ) => {
    setList(list().filter((_, i) => i !== idx));
  };

  const formatExpiry = (expiresAt: string) => {
    const d = new Date(expiresAt);
    const now = new Date();
    const diff = d.getTime() - now.getTime();
    if (diff <= 0) return "Expired";
    const mins = Math.floor(diff / 60000);
    if (mins < 60) return `${mins}m remaining`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h remaining`;
    const days = Math.floor(hrs / 24);
    return `${days}d remaining`;
  };

  const GrantEditor = (props: {
    grants: () => InvitePermissionGrant[];
    setGrants: (v: InvitePermissionGrant[]) => void;
  }) => (
    <div style={{ "margin-top": "0.5rem" }}>
      <div
        style={{
          display: "flex",
          "justify-content": "space-between",
          "align-items": "center",
          "margin-bottom": "0.5rem",
        }}
      >
        <label
          style={{
            "font-size": "0.85rem",
            color: "var(--text-muted)",
            "font-weight": "600",
          }}
        >
          Server Permissions
        </label>
        <button
          type="button"
          class="btn btn-sm btn-secondary"
          onClick={() => addGrant(props.grants, props.setGrants)}
          style={{ "font-size": "0.75rem", padding: "0.2rem 0.5rem" }}
        >
          + Add Server
        </button>
      </div>
      <For each={props.grants()}>
        {(grant, idx) => (
          <div
            style={{
              display: "flex",
              gap: "0.5rem",
              "align-items": "center",
              "margin-bottom": "0.4rem",
            }}
          >
            <select
              value={grant.server_id}
              onChange={(e) => {
                const updated = [...props.grants()];
                updated[idx()] = { ...grant, server_id: e.currentTarget.value };
                props.setGrants(updated);
              }}
              style={{
                flex: "1",
                padding: "0.3rem 0.5rem",
                background: "var(--bg-input)",
                color: "var(--text)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                "font-size": "0.8rem",
              }}
            >
              <For each={servers()?.servers ?? []}>
                {(server) => (
                  <option value={server.server.id}>
                    {server.server.config.name}
                  </option>
                )}
              </For>
            </select>
            <select
              value={grant.level}
              onChange={(e) => {
                const updated = [...props.grants()];
                updated[idx()] = {
                  ...grant,
                  level: e.currentTarget.value as PermissionLevel,
                };
                props.setGrants(updated);
              }}
              style={{
                width: "110px",
                padding: "0.3rem 0.5rem",
                background: "var(--bg-input)",
                color: "var(--text)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                "font-size": "0.8rem",
              }}
            >
              <For each={PERMISSION_LEVELS}>
                {(level) => <option value={level.value}>{level.label}</option>}
              </For>
            </select>
            <button
              type="button"
              class="btn btn-sm"
              style={{
                background: "var(--danger-bg)",
                color: "var(--danger)",
                border: "1px solid var(--danger)",
                padding: "0.2rem 0.5rem",
                "font-size": "0.75rem",
                cursor: "pointer",
              }}
              onClick={() => removeGrant(props.grants, props.setGrants, idx())}
            >
              ✕
            </button>
          </div>
        )}
      </For>
      <Show when={props.grants().length === 0}>
        <p
          style={{
            "font-size": "0.8rem",
            color: "var(--text-dim)",
            "font-style": "italic",
          }}
        >
          No server permissions — user will only get the assigned role.
        </p>
      </Show>
    </div>
  );

  return (
    <div>
      <div
        style={{
          display: "flex",
          "justify-content": "space-between",
          "align-items": "center",
          "margin-bottom": "1rem",
        }}
      >
        <h2>Invite Codes</h2>
        <button
          class="btn btn-primary"
          onClick={() => setShowCreate(!showCreate())}
          style={{ "font-size": "0.85rem" }}
        >
          {showCreate() ? "Cancel" : "+ New Invite Code"}
        </button>
      </div>

      <Show when={error()}>
        {(err) => <div class="error-msg">{err()}</div>}
      </Show>
      <Show when={success()}>
        {(msg) => (
          <div
            style={{
              background: "var(--success-bg)",
              border: "1px solid var(--success)",
              "border-radius": "var(--radius-sm)",
              padding: "0.75rem",
              color: "var(--success)",
              "margin-bottom": "1rem",
              "font-size": "0.9rem",
              "font-weight": "600",
            }}
          >
            {msg()}
          </div>
        )}
      </Show>

      {/* Create form */}
      <Show when={showCreate()}>
        <div
          style={{
            background: "var(--bg-elevated)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius)",
            padding: "1.25rem",
            "margin-bottom": "1.5rem",
          }}
        >
          <h3
            style={{
              "margin-bottom": "1rem",
              color: "var(--text)",
              "text-transform": "none",
              "font-size": "1rem",
            }}
          >
            Create New Invite Code
          </h3>

          <div style={{ display: "flex", gap: "1rem", "flex-wrap": "wrap" }}>
            <div class="form-group" style={{ flex: "1", "min-width": "150px" }}>
              <label>Expiry</label>
              <select
                value={expiry()}
                onChange={(e) =>
                  setExpiry(e.currentTarget.value as InviteExpiry)
                }
              >
                <For each={EXPIRY_OPTIONS}>
                  {(opt) => <option value={opt.value}>{opt.label}</option>}
                </For>
              </select>
            </div>
            <div class="form-group" style={{ flex: "1", "min-width": "150px" }}>
              <label>Assigned Role</label>
              <select
                value={assignedRole()}
                onChange={(e) => setAssignedRole(e.currentTarget.value as Role)}
              >
                <option value="user">User</option>
                <option value="admin">Admin</option>
              </select>
            </div>
            <div class="form-group" style={{ flex: "2", "min-width": "200px" }}>
              <label>Label (optional)</label>
              <input
                type="text"
                value={label()}
                onInput={(e) => setLabel(e.currentTarget.value)}
                placeholder="e.g. For John's account"
              />
            </div>
          </div>

          <GrantEditor grants={grants} setGrants={setGrants} />

          <button
            class="btn btn-primary"
            onClick={handleCreate}
            disabled={creating()}
            style={{ "margin-top": "1rem" }}
          >
            {creating() ? "Creating..." : "Generate Invite Code"}
          </button>
        </div>
      </Show>

      {/* Invite code list */}
      <Show when={invites.loading}>
        <Loader message="Loading invite codes" />
      </Show>

      <Show when={invites()}>
        {(resolved) => (
          <Show
            when={resolved().invites.length > 0}
            fallback={
              <div
                style={{
                  "text-align": "center",
                  padding: "2rem",
                  color: "var(--text-dim)",
                }}
              >
                <p>No invite codes yet. Create one to get started.</p>
              </div>
            }
          >
            <div
              style={{
                display: "grid",
                gap: "0.75rem",
              }}
            >
              <For each={resolved().invites}>
                {(invite) => (
                  <div
                    style={{
                      background: "var(--bg-card)",
                      border: `1px solid ${invite.is_active ? "var(--primary)" : invite.redeemed_by ? "var(--success)" : "var(--danger)"}`,
                      "border-radius": "var(--radius)",
                      padding: "1rem 1.25rem",
                      display: "flex",
                      "justify-content": "space-between",
                      "align-items": "flex-start",
                      gap: "1rem",
                      "flex-wrap": "wrap",
                      opacity: invite.is_active ? "1" : "0.7",
                    }}
                  >
                    <div style={{ flex: "1", "min-width": "200px" }}>
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          gap: "0.75rem",
                          "margin-bottom": "0.5rem",
                        }}
                      >
                        <span
                          style={{
                            "font-family": "var(--mono)",
                            "font-size": "1.5rem",
                            "font-weight": "700",
                            "letter-spacing": "0.3em",
                            color: invite.is_active
                              ? "var(--primary)"
                              : "var(--text-dim)",
                          }}
                        >
                          {invite.code.length === 8
                            ? `${invite.code.slice(0, 4)}-${invite.code.slice(4)}`
                            : invite.code}
                        </span>
                        <span
                          class="status-badge"
                          style={{
                            background: invite.is_active
                              ? "var(--primary-bg)"
                              : invite.redeemed_by
                                ? "var(--success-bg)"
                                : "var(--danger-bg)",
                            color: invite.is_active
                              ? "var(--primary)"
                              : invite.redeemed_by
                                ? "var(--success)"
                                : "var(--danger)",
                            padding: "0.15rem 0.5rem",
                            "border-radius": "999px",
                            "font-size": "0.7rem",
                            "font-weight": "600",
                          }}
                        >
                          {invite.is_active
                            ? "Active"
                            : invite.redeemed_by
                              ? "Redeemed"
                              : "Expired"}
                        </span>
                        <span
                          class="status-badge"
                          style={{
                            background:
                              invite.assigned_role === "admin"
                                ? "var(--orange-bg)"
                                : "var(--bg-elevated)",
                            color:
                              invite.assigned_role === "admin"
                                ? "var(--orange)"
                                : "var(--text-muted)",
                            padding: "0.15rem 0.5rem",
                            "border-radius": "999px",
                            "font-size": "0.7rem",
                          }}
                        >
                          Role: {invite.assigned_role}
                        </span>
                      </div>

                      <div
                        style={{
                          "font-size": "0.8rem",
                          color: "var(--text-muted)",
                          display: "flex",
                          gap: "1rem",
                          "flex-wrap": "wrap",
                        }}
                      >
                        <Show when={invite.label}>
                          <span>📝 {invite.label}</span>
                        </Show>
                        <span>
                          ⏱️{" "}
                          {invite.is_active
                            ? formatExpiry(invite.expires_at)
                            : invite.redeemed_by
                              ? `Redeemed by ${invite.redeemed_by_username ?? "unknown"}`
                              : "Expired"}
                        </span>
                        <span>
                          👤 Created by{" "}
                          {invite.created_by_username ?? "unknown"}
                        </span>
                        <Show when={invite.assigned_permissions.length > 0}>
                          <span>
                            🔑 {invite.assigned_permissions.length} server
                            permission
                            {invite.assigned_permissions.length > 1 ? "s" : ""}
                          </span>
                        </Show>
                      </div>
                    </div>

                    <div
                      style={{
                        display: "flex",
                        gap: "0.5rem",
                        "align-items": "flex-start",
                      }}
                    >
                      <Show when={invite.is_active}>
                        <Show when={editingId() === invite.id}>
                          <button
                            class="btn btn-sm btn-primary"
                            onClick={handleUpdatePermissions}
                            style={{
                              "font-size": "0.75rem",
                              padding: "0.3rem 0.6rem",
                            }}
                          >
                            Save
                          </button>
                          <button
                            class="btn btn-sm btn-secondary"
                            onClick={() => setEditingId(null)}
                            style={{
                              "font-size": "0.75rem",
                              padding: "0.3rem 0.6rem",
                            }}
                          >
                            Cancel
                          </button>
                        </Show>
                        <Show when={editingId() !== invite.id}>
                          <button
                            class="btn btn-sm btn-secondary"
                            onClick={() => startEdit(invite)}
                            style={{
                              "font-size": "0.75rem",
                              padding: "0.3rem 0.6rem",
                            }}
                          >
                            Edit Perms
                          </button>
                        </Show>
                      </Show>
                      <button
                        class="btn btn-sm"
                        style={{
                          background: "var(--danger-bg)",
                          color: "var(--danger)",
                          border: "1px solid var(--danger)",
                          "font-size": "0.75rem",
                          padding: "0.3rem 0.6rem",
                          cursor: "pointer",
                        }}
                        onClick={() => handleDelete(invite.id)}
                      >
                        Delete
                      </button>
                    </div>

                    {/* Inline edit panel */}
                    <Show when={editingId() === invite.id}>
                      <div
                        style={{
                          width: "100%",
                          background: "var(--bg-elevated)",
                          "border-radius": "var(--radius-sm)",
                          padding: "0.75rem",
                          "margin-top": "0.25rem",
                          "border-top": "1px solid var(--border)",
                        }}
                      >
                        <div
                          class="form-group"
                          style={{ "margin-bottom": "0.5rem" }}
                        >
                          <label
                            style={{
                              "font-size": "0.8rem",
                              color: "var(--text-muted)",
                            }}
                          >
                            Assigned Role
                          </label>
                          <select
                            value={editRole()}
                            onChange={(e) =>
                              setEditRole(e.currentTarget.value as Role)
                            }
                            style={{
                              padding: "0.3rem 0.5rem",
                              "font-size": "0.85rem",
                            }}
                          >
                            <option value="user">User</option>
                            <option value="admin">Admin</option>
                          </select>
                        </div>
                        <GrantEditor
                          grants={editGrants}
                          setGrants={setEditGrants}
                        />
                      </div>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
        )}
      </Show>
    </div>
  );
};

export default InviteCodesTab;
