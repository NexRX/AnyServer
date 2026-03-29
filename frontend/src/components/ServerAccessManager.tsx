import {
  type Component,
  createSignal,
  createResource,
  Show,
  For,
  onCleanup,
} from "solid-js";
import {
  listPermissions,
  setPermission,
  removePermission,
  searchUsers,
} from "../api/client";
import { useAuth } from "../context/auth";
import type {
  ServerPermissionEntry,
  PermissionLevel,
  UserPublic,
} from "../types/bindings";
import Loader from "./Loader";

interface ServerAccessManagerProps {
  serverId: string;
  /** The current user's effective permission level on this server */
  myLevel: PermissionLevel;
  /** Whether the current user is a global admin */
  isGlobalAdmin: boolean;
  /** The owner_id of the server */
  ownerId: string;
}

const PERMISSION_LEVELS: { value: PermissionLevel; label: string }[] = [
  { value: "viewer", label: "Viewer" },
  { value: "operator", label: "Operator" },
  { value: "manager", label: "Manager" },
  { value: "admin", label: "Admin" },
  { value: "owner", label: "Owner" },
];

const LEVEL_DESCRIPTIONS: Record<PermissionLevel, string> = {
  viewer:
    "View server status, logs, files, and configuration. Read-only access — cannot start, stop, or modify anything.",
  operator:
    "Everything a Viewer can do, plus start, stop, restart the server and send console commands.",
  manager:
    "Everything an Operator can do, plus edit configuration, manage files (create, edit, delete), and run install/update pipelines.",
  admin:
    "Everything a Manager can do, plus delete the server, manage per-server permissions, and edit pipeline definitions.",
  owner:
    "Full control over the server. Only one owner per server. Can transfer ownership and do everything an Admin can.",
};

const LEVEL_RANK: Record<PermissionLevel, number> = {
  viewer: 0,
  operator: 1,
  manager: 2,
  admin: 3,
  owner: 4,
};

const LEVEL_COLORS: Record<PermissionLevel, string> = {
  viewer: "var(--text-dim)",
  operator: "var(--primary)",
  manager: "var(--warning)",
  admin: "var(--orange)",
  owner: "var(--success)",
};

const ServerAccessManager: Component<ServerAccessManagerProps> = (props) => {
  const auth = useAuth();

  // ── State ──
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);
  const [actionLoading, setActionLoading] = createSignal<string | null>(null);

  // User search
  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<UserPublic[]>([]);
  const [searching, setSearching] = createSignal(false);
  const [showSearch, setShowSearch] = createSignal(false);
  const [selectedUser, setSelectedUser] = createSignal<UserPublic | null>(null);
  const [grantLevel, setGrantLevel] = createSignal<PermissionLevel>("viewer");

  // Permissions list
  const [permissions, { refetch }] = createResource(
    () => props.serverId,
    (id) => listPermissions(id),
  );

  // Debounced search
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  onCleanup(() => {
    if (searchTimer) clearTimeout(searchTimer);
  });

  const handleSearchInput = (value: string) => {
    setSearchQuery(value);
    setSelectedUser(null);
    if (searchTimer) clearTimeout(searchTimer);

    if (value.trim().length === 0) {
      setSearchResults([]);
      return;
    }

    searchTimer = setTimeout(async () => {
      setSearching(true);
      try {
        const res = await searchUsers(value.trim());
        // Filter out users who already have permissions on this server
        const existingIds = new Set(
          (permissions()?.permissions ?? []).map((p) => p.user.id),
        );
        setSearchResults(
          res.users.filter((u) => !existingIds.has(u.id)),
        );
      } catch (e) {
        // Silently handle search errors — not critical
        setSearchResults([]);
      } finally {
        setSearching(false);
      }
    }, 300);
  };

  const clearMessages = () => {
    setError(null);
    setSuccess(null);
  };

  // ── What levels can I grant? ──
  const grantableLevels = (): typeof PERMISSION_LEVELS => {
    if (props.isGlobalAdmin) return PERMISSION_LEVELS;
    const myRank = LEVEL_RANK[props.myLevel];
    // Server-level admins can grant up to their own level minus one,
    // except global admins who can grant Owner
    return PERMISSION_LEVELS.filter((l) => {
      if (l.value === "owner") return false; // only global admins
      return LEVEL_RANK[l.value] < myRank;
    });
  };

  const canModifyUser = (entry: ServerPermissionEntry): boolean => {
    // Can't modify yourself
    if (entry.user.id === auth.user()?.id) return false;
    // Can't modify the server owner's inherent permission
    if (entry.user.id === props.ownerId && entry.level === "owner")
      return false;
    if (props.isGlobalAdmin) return true;
    // Can only modify users with lower permission than yours
    return LEVEL_RANK[entry.level] < LEVEL_RANK[props.myLevel];
  };

  // ── Actions ──
  const handleGrant = async () => {
    const user = selectedUser();
    if (!user) return;
    clearMessages();
    setActionLoading("grant");
    try {
      await setPermission(props.serverId, {
        user_id: user.id,
        level: grantLevel(),
      });
      setSuccess(
        `Granted ${grantLevel()} access to ${user.username}.`,
      );
      setSelectedUser(null);
      setSearchQuery("");
      setSearchResults([]);
      setShowSearch(false);
      setGrantLevel("viewer");
      refetch();
    } catch (e: unknown) {
      setError(
        e instanceof Error ? e.message : "Failed to grant permission.",
      );
    } finally {
      setActionLoading(null);
    }
  };

  const handleChangeLevel = async (
    userId: string,
    username: string,
    newLevel: PermissionLevel,
  ) => {
    clearMessages();
    setActionLoading(userId);
    try {
      await setPermission(props.serverId, {
        user_id: userId,
        level: newLevel,
      });
      setSuccess(`Updated ${username} to ${newLevel}.`);
      refetch();
    } catch (e: unknown) {
      setError(
        e instanceof Error
          ? e.message
          : "Failed to update permission.",
      );
    } finally {
      setActionLoading(null);
    }
  };

  const handleRevoke = async (userId: string, username: string) => {
    if (
      !confirm(
        `Remove ${username}'s access to this server? They will no longer be able to see or interact with it.`,
      )
    )
      return;
    clearMessages();
    setActionLoading(userId);
    try {
      await removePermission(props.serverId, { user_id: userId });
      setSuccess(`Removed ${username}'s access.`);
      refetch();
    } catch (e: unknown) {
      setError(
        e instanceof Error
          ? e.message
          : "Failed to remove permission.",
      );
    } finally {
      setActionLoading(null);
    }
  };

  const isOwnerEntry = (entry: ServerPermissionEntry): boolean =>
    entry.user.id === props.ownerId && entry.level === "owner";

  const isSelf = (entry: ServerPermissionEntry): boolean =>
    entry.user.id === auth.user()?.id;

  return (
    <div class="access-manager">
      {/* ── Header ── */}
      <div class="access-header">
        <div class="access-header-text">
          <h3 style={{ margin: 0, "font-size": "1.1rem" }}>
            Server Access
          </h3>
          <p
            style={{
              margin: "0.25rem 0 0",
              color: "var(--text-muted)",
              "font-size": "0.85rem",
            }}
          >
            Control who can access this server and what they can do. Users
            need an account on this AnyServer instance before you can grant
            them access.
          </p>
        </div>
        <Show when={!showSearch()}>
          <button
            class="btn btn-primary"
            onClick={() => setShowSearch(true)}
            style={{ "white-space": "nowrap" }}
          >
            + Add User
          </button>
        </Show>
      </div>

      {/* ── Messages ── */}
      <Show when={error()}>
        {(msg) => (
          <div class="access-msg access-msg-error">
            <span>{msg()}</span>
            <button
              class="access-msg-close"
              onClick={() => setError(null)}
            >
              ✕
            </button>
          </div>
        )}
      </Show>
      <Show when={success()}>
        {(msg) => (
          <div class="access-msg access-msg-success">
            <span>{msg()}</span>
            <button
              class="access-msg-close"
              onClick={() => setSuccess(null)}
            >
              ✕
            </button>
          </div>
        )}
      </Show>

      {/* ── Add User Panel ── */}
      <Show when={showSearch()}>
        <div class="access-add-panel">
          <div class="access-add-panel-header">
            <h4 style={{ margin: 0, "font-size": "0.95rem" }}>
              Grant Access to a User
            </h4>
            <button
              class="btn btn-sm"
              onClick={() => {
                setShowSearch(false);
                setSearchQuery("");
                setSearchResults([]);
                setSelectedUser(null);
              }}
            >
              Cancel
            </button>
          </div>

          <Show
            when={!selectedUser()}
            fallback={
              <div class="access-selected-user">
                <div class="access-selected-user-info">
                  <div class="access-avatar">
                    {(selectedUser()?.username ?? "?")[0].toUpperCase()}
                  </div>
                  <div>
                    <div style={{ "font-weight": 500 }}>
                      {selectedUser()?.username}
                    </div>
                    <div
                      style={{
                        "font-size": "0.8rem",
                        color: "var(--text-dim)",
                      }}
                    >
                      {selectedUser()?.role === "admin"
                        ? "Platform Admin"
                        : "User"}
                    </div>
                  </div>
                  <button
                    class="btn btn-sm"
                    style={{ "margin-left": "auto" }}
                    onClick={() => {
                      setSelectedUser(null);
                      setSearchQuery("");
                    }}
                  >
                    Change
                  </button>
                </div>

                <div class="access-grant-form">
                  <label class="access-label">Permission Level</label>
                  <div class="access-level-picker">
                    <For each={grantableLevels()}>
                      {(level) => (
                        <button
                          class={`access-level-option ${grantLevel() === level.value ? "active" : ""}`}
                          onClick={() => setGrantLevel(level.value)}
                        >
                          <span
                            class="access-level-dot"
                            style={{
                              background: LEVEL_COLORS[level.value],
                            }}
                          />
                          <span class="access-level-option-label">
                            {level.label}
                          </span>
                        </button>
                      )}
                    </For>
                  </div>

                  <div class="access-level-desc">
                    {LEVEL_DESCRIPTIONS[grantLevel()]}
                  </div>

                  <button
                    class="btn btn-primary"
                    style={{ "margin-top": "0.75rem" }}
                    disabled={actionLoading() === "grant"}
                    onClick={handleGrant}
                  >
                    {actionLoading() === "grant"
                      ? "Granting..."
                      : `Grant ${grantLevel()} access`}
                  </button>
                </div>
              </div>
            }
          >
            <div class="access-search-box">
              <input
                type="text"
                class="form-input"
                placeholder="Search by username…"
                value={searchQuery()}
                onInput={(e) =>
                  handleSearchInput(e.currentTarget.value)
                }
                autofocus
              />
              <Show when={searching()}>
                <div class="access-search-spinner">
                  <Loader message="" />
                </div>
              </Show>
            </div>

            <Show
              when={
                searchQuery().trim().length > 0 &&
                !searching()
              }
            >
              <Show
                when={searchResults().length > 0}
                fallback={
                  <div class="access-search-empty">
                    <span style={{ "font-size": "1.5rem" }}>🔍</span>
                    <span>
                      No users found matching "
                      {searchQuery().trim()}".
                    </span>
                    <span
                      style={{
                        "font-size": "0.8rem",
                        color: "var(--text-dim)",
                      }}
                    >
                      Users must have an account on this platform first.
                      Already-added users are hidden from results.
                    </span>
                  </div>
                }
              >
                <div class="access-search-results">
                  <For each={searchResults()}>
                    {(user) => (
                      <button
                        class="access-search-result"
                        onClick={() => {
                          setSelectedUser(user);
                          setSearchResults([]);
                        }}
                      >
                        <div class="access-avatar">
                          {user.username[0].toUpperCase()}
                        </div>
                        <div class="access-search-result-info">
                          <span class="access-search-result-name">
                            {user.username}
                          </span>
                          <span class="access-search-result-role">
                            {user.role === "admin"
                              ? "Platform Admin"
                              : "User"}
                          </span>
                        </div>
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Show>
        </div>
      </Show>

      {/* ── Permission Tiers Legend ── */}
      <details class="access-legend">
        <summary class="access-legend-summary">
          <span>Permission levels explained</span>
          <span class="access-legend-arrow">▸</span>
        </summary>
        <div class="access-legend-content">
          <For each={PERMISSION_LEVELS}>
            {(level) => (
              <div class="access-legend-row">
                <div class="access-legend-label">
                  <span
                    class="access-level-dot"
                    style={{
                      background: LEVEL_COLORS[level.value],
                    }}
                  />
                  <span style={{ "font-weight": 500 }}>
                    {level.label}
                  </span>
                </div>
                <div class="access-legend-desc">
                  {LEVEL_DESCRIPTIONS[level.value]}
                </div>
              </div>
            )}
          </For>
        </div>
      </details>

      {/* ── Current Permissions ── */}
      <Show
        when={!permissions.loading}
        fallback={<Loader message="Loading permissions" />}
      >
        <Show when={permissions()}>
          {(resolved) => (
            <div class="access-list">
              <div class="access-list-header">
                <span class="access-col-user">User</span>
                <span class="access-col-level">Level</span>
                <span class="access-col-actions">Actions</span>
              </div>
              <For each={resolved().permissions}>
                {(entry) => {
                  const isMe = () => isSelf(entry);
                  const isOwner = () => isOwnerEntry(entry);
                  const editable = () => canModifyUser(entry);

                  return (
                    <div
                      class="access-row"
                      classList={{
                        "access-row-self": isMe(),
                        "access-row-owner": isOwner(),
                      }}
                    >
                      <div class="access-col-user">
                        <div class="access-avatar">
                          {entry.user.username[0].toUpperCase()}
                        </div>
                        <div class="access-user-info">
                          <span class="access-user-name">
                            {entry.user.username}
                          </span>
                          <span class="access-user-tags">
                            <Show when={isMe()}>
                              <span class="tag">you</span>
                            </Show>
                            <Show when={isOwner()}>
                              <span class="tag tag-owner">owner</span>
                            </Show>
                            <Show
                              when={entry.user.role === "admin"}
                            >
                              <span class="tag tag-admin">
                                platform admin
                              </span>
                            </Show>
                          </span>
                        </div>
                      </div>

                      <div class="access-col-level">
                        <Show
                          when={editable() && !isOwner()}
                          fallback={
                            <span
                              class="access-level-badge"
                              style={{
                                "border-color":
                                  LEVEL_COLORS[entry.level],
                                color: LEVEL_COLORS[entry.level],
                              }}
                            >
                              {entry.level}
                            </span>
                          }
                        >
                          <select
                            class="access-level-select"
                            value={entry.level}
                            disabled={
                              actionLoading() === entry.user.id
                            }
                            onChange={(e) =>
                              handleChangeLevel(
                                entry.user.id,
                                entry.user.username,
                                e.currentTarget
                                  .value as PermissionLevel,
                              )
                            }
                          >
                            <For each={grantableLevels()}>
                              {(l) => (
                                <option value={l.value}>
                                  {l.label}
                                </option>
                              )}
                            </For>
                            {/* If the user's current level isn't in our grantable list, still show it */}
                            <Show
                              when={
                                !grantableLevels().some(
                                  (l) => l.value === entry.level,
                                )
                              }
                            >
                              <option value={entry.level}>
                                {entry.level.charAt(0).toUpperCase() +
                                  entry.level.slice(1)}{" "}
                                (current)
                              </option>
                            </Show>
                          </select>
                        </Show>
                      </div>

                      <div class="access-col-actions">
                        <Show when={editable() && !isOwner()}>
                          <button
                            class="btn btn-sm btn-danger-outline"
                            disabled={
                              actionLoading() === entry.user.id
                            }
                            onClick={() =>
                              handleRevoke(
                                entry.user.id,
                                entry.user.username,
                              )
                            }
                            title={`Remove ${entry.user.username}'s access`}
                          >
                            {actionLoading() === entry.user.id
                              ? "…"
                              : "Remove"}
                          </button>
                        </Show>
                        <Show when={isOwner() || (isMe() && !editable())}>
                          <span
                            style={{
                              color: "var(--text-dim)",
                              "font-size": "0.8rem",
                            }}
                          >
                            {isOwner() ? "inherent" : "—"}
                          </span>
                        </Show>
                      </div>
                    </div>
                  );
                }}
              </For>

              <Show
                when={
                  resolved().permissions.length === 0
                }
              >
                <div class="access-empty">
                  No one has been granted access to this server yet.
                </div>
              </Show>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
};

export default ServerAccessManager;
