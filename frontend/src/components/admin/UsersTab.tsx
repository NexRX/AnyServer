import {
  type Component,
  createSignal,
  createResource,
  For,
  Show,
} from "solid-js";
import Loader from "../Loader";
import { formatDate } from "../../utils/format";
import { listUsers, updateUserRole, deleteUser } from "../../api/client";
import { useAuth } from "../../context/auth";
import type { UserPublic, Role } from "../../types/bindings";

const UsersTab: Component = () => {
  const auth = useAuth();
  const [users, { refetch }] = createResource(listUsers);
  const [actionError, setActionError] = createSignal<string | null>(null);
  const [actionLoading, setActionLoading] = createSignal<string | null>(null);

  const handleRoleChange = async (user: UserPublic, newRole: Role) => {
    if (user.id === auth.user()?.id) {
      setActionError("You cannot change your own role.");
      return;
    }

    const label =
      newRole === "admin" ? "promote to admin" : "demote to regular user";
    if (!confirm(`Are you sure you want to ${label} "${user.username}"?`)) {
      return;
    }

    setActionLoading(user.id);
    setActionError(null);
    try {
      await updateUserRole(user.id, { role: newRole });
      await refetch();
    } catch (e: unknown) {
      setActionError(
        e instanceof Error ? e.message : "Failed to update user role.",
      );
    } finally {
      setActionLoading(null);
    }
  };

  const handleDelete = async (user: UserPublic) => {
    if (user.id === auth.user()?.id) {
      setActionError("You cannot delete your own account.");
      return;
    }

    if (
      !confirm(
        `Are you sure you want to delete user "${user.username}"? This will remove all their server permissions. This action cannot be undone.`,
      )
    ) {
      return;
    }

    setActionLoading(user.id);
    setActionError(null);
    try {
      await deleteUser(user.id);
      await refetch();
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : "Failed to delete user.");
    } finally {
      setActionLoading(null);
    }
  };

  return (
    <div class="admin-users">
      <div class="admin-section-header">
        <h2>User Management</h2>
        <button class="btn btn-sm" onClick={() => refetch()}>
          ↻ Refresh
        </button>
      </div>

      <Show when={actionError()}>
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
              onClick={() => setActionError(null)}
            >
              ✕
            </button>
          </div>
        )}
      </Show>

      <Show when={users()} fallback={<Loader message="Loading users" />}>
        {(resolved) => (
          <div class="admin-user-list">
            <div class="admin-user-header">
              <span class="admin-user-col-name">Username</span>
              <span class="admin-user-col-role">Role</span>
              <span class="admin-user-col-date">Created</span>
              <span class="admin-user-col-actions">Actions</span>
            </div>
            <For each={resolved().users}>
              {(user) => {
                const isSelf = () => user.id === auth.user()?.id;
                const isLoading = () => actionLoading() === user.id;

                return (
                  <div
                    class={`admin-user-row ${isSelf() ? "admin-user-self" : ""}`}
                  >
                    <span class="admin-user-col-name">
                      <span class="admin-username">{user.username}</span>
                      <Show when={isSelf()}>
                        <span class="tag" style={{ "margin-left": "0.4rem" }}>
                          you
                        </span>
                      </Show>
                    </span>
                    <span class="admin-user-col-role">
                      <span
                        class={`status-badge ${user.role === "admin" ? "status-running" : "status-stopped"}`}
                      >
                        {user.role}
                      </span>
                    </span>
                    <span class="admin-user-col-date">
                      {formatDate(user.created_at)}
                    </span>
                    <span class="admin-user-col-actions">
                      <Show when={!isSelf()}>
                        <Show
                          when={user.role !== "admin"}
                          fallback={
                            <button
                              class="btn btn-sm"
                              onClick={() => handleRoleChange(user, "user")}
                              disabled={isLoading()}
                              title="Demote to regular user"
                            >
                              Demote
                            </button>
                          }
                        >
                          <button
                            class="btn btn-sm btn-primary"
                            onClick={() => handleRoleChange(user, "admin")}
                            disabled={isLoading()}
                            title="Promote to admin"
                          >
                            Promote
                          </button>
                        </Show>
                        <button
                          class="btn btn-sm btn-danger-outline"
                          onClick={() => handleDelete(user)}
                          disabled={isLoading()}
                          title="Delete user"
                        >
                          Delete
                        </button>
                      </Show>
                      <Show when={isSelf()}>
                        <span
                          style={{
                            color: "var(--text-dim)",
                            "font-size": "0.8rem",
                          }}
                        >
                          —
                        </span>
                      </Show>
                    </span>
                  </div>
                );
              }}
            </For>
          </div>
        )}
      </Show>

      <Show when={users()}>
        <div
          style={{
            "margin-top": "1rem",
            color: "var(--text-dim)",
            "font-size": "0.85rem",
          }}
        >
          {users()!.users.length} user{users()!.users.length !== 1 ? "s" : ""}{" "}
          total · {users()!.users.filter((u) => u.role === "admin").length}{" "}
          admin
          {users()!.users.filter((u) => u.role === "admin").length !== 1
            ? "s"
            : ""}
        </div>
      </Show>
    </div>
  );
};

export default UsersTab;
