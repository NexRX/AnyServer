import {
  type Component,
  createSignal,
  createResource,
  For,
  Show,
} from "solid-js";
import Loader from "../Loader";
import { listUserPermissions } from "../../api/client";

const UserPermissionsTab: Component = () => {
  const [data, { refetch }] = createResource(listUserPermissions);
  const [error, setError] = createSignal<string | null>(null);

  return (
    <div>
      <h2 style={{ "margin-bottom": "1rem" }}>User Permissions Overview</h2>
      <p
        style={{
          color: "var(--text-muted)",
          "font-size": "0.85rem",
          "margin-bottom": "1.5rem",
        }}
      >
        View and audit which users have access to which servers. Implicit
        permissions (from ownership or admin role) are shown with a{" "}
        <span style={{ color: "var(--text-dim)", "font-style": "italic" }}>
          dimmed style
        </span>
        .
      </p>

      <Show when={error()}>
        {(err) => <div class="error-msg">{err()}</div>}
      </Show>

      <Show when={data.loading}>
        <Loader message="Loading permissions" />
      </Show>

      <Show when={data()}>
        {(resolved) => (
          <div style={{ display: "grid", gap: "0.75rem" }}>
            <For each={resolved().users}>
              {(userSummary) => (
                <div
                  style={{
                    background: "var(--bg-card)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius)",
                    padding: "1rem 1.25rem",
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      "align-items": "center",
                      gap: "0.75rem",
                      "margin-bottom": "0.75rem",
                    }}
                  >
                    <span style={{ "font-weight": "600", "font-size": "1rem" }}>
                      👤 {userSummary.username}
                    </span>
                    <span
                      class="status-badge"
                      style={{
                        background:
                          userSummary.role === "admin"
                            ? "var(--orange-bg)"
                            : "var(--bg-elevated)",
                        color:
                          userSummary.role === "admin"
                            ? "var(--orange)"
                            : "var(--text-muted)",
                        padding: "0.15rem 0.5rem",
                        "border-radius": "999px",
                        "font-size": "0.7rem",
                      }}
                    >
                      {userSummary.role}
                    </span>
                  </div>

                  <Show
                    when={userSummary.server_permissions.length > 0}
                    fallback={
                      <p
                        style={{
                          "font-size": "0.8rem",
                          color: "var(--text-dim)",
                          "font-style": "italic",
                        }}
                      >
                        No server permissions
                      </p>
                    }
                  >
                    <div
                      style={{
                        display: "flex",
                        "flex-wrap": "wrap",
                        gap: "0.4rem",
                      }}
                    >
                      <For each={userSummary.server_permissions}>
                        {(perm) => (
                          <span
                            style={{
                              background: perm.is_implicit
                                ? "var(--bg-elevated)"
                                : "var(--primary-bg)",
                              color: perm.is_implicit
                                ? "var(--text-dim)"
                                : "var(--primary)",
                              padding: "0.2rem 0.6rem",
                              "border-radius": "999px",
                              "font-size": "0.75rem",
                              border: `1px solid ${perm.is_implicit ? "var(--border)" : "var(--primary)"}`,
                              opacity: perm.is_implicit ? "0.7" : "1",
                            }}
                          >
                            {perm.server_name}: <strong>{perm.level}</strong>
                            {perm.is_implicit ? " (implicit)" : ""}
                          </span>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        )}
      </Show>
    </div>
  );
};

export default UserPermissionsTab;
