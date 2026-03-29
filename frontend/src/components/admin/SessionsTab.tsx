import {
  type Component,
  createSignal,
  createResource,
  For,
  Show,
} from "solid-js";
import Loader from "../Loader";
import { formatDateTimePadded } from "../../utils/format";
import { listSessions, revokeSession } from "../../api/client";
import type { SessionInfo } from "../../types/bindings";

const SessionsTab: Component = () => {
  const [sessions, { refetch }] = createResource(listSessions);
  const [revoking, setRevoking] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);

  const handleRevoke = async (familyId: string) => {
    setRevoking(familyId);
    setError(null);
    setSuccess(null);

    try {
      await revokeSession({ family_id: familyId });
      setSuccess("Session revoked successfully");
      setTimeout(() => setSuccess(null), 3000);
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to revoke session");
    } finally {
      setRevoking(null);
    }
  };

  return (
    <div class="admin-sessions">
      <h2>Active Sessions</h2>
      <p style={{ "margin-bottom": "1.5rem", color: "var(--text-secondary)" }}>
        Manage your active login sessions. You can revoke any session except the
        current one.
      </p>

      <Show when={error()}>
        {(err) => (
          <div
            style={{
              background: "var(--error-bg)",
              border: "1px solid rgba(239, 68, 68, 0.3)",
              "border-radius": "var(--radius-sm)",
              padding: "0.7rem 1rem",
              color: "var(--error)",
              "margin-bottom": "1rem",
              "font-size": "0.9rem",
            }}
          >
            {err()}
          </div>
        )}
      </Show>

      <Show when={success()}>
        {(msg) => (
          <div
            style={{
              background: "var(--success-bg)",
              border: "1px solid rgba(34, 197, 94, 0.3)",
              "border-radius": "var(--radius-sm)",
              padding: "0.7rem 1rem",
              color: "var(--success)",
              "margin-bottom": "1rem",
              "font-size": "0.9rem",
            }}
          >
            ✓ {msg()}
          </div>
        )}
      </Show>

      <Show when={sessions.loading}>
        <Loader />
      </Show>

      <Show when={sessions.error}>
        <div class="error-msg">
          Failed to load sessions: {String(sessions.error)}
        </div>
      </Show>

      <Show when={!sessions.loading ? sessions() : undefined}>
        {(data) => {
          const sessionList = data().sessions || [];
          return (
            <Show
              when={sessionList.length > 0}
              fallback={
                <p style={{ color: "var(--text-secondary)" }}>
                  No active sessions found.
                </p>
              }
            >
              <div class="sessions-list">
                <For each={sessionList}>
                  {(session: SessionInfo) => (
                    <div
                      class="session-card"
                      style={{
                        background: session.is_current
                          ? "var(--bg-secondary)"
                          : "var(--bg-primary)",
                        border: session.is_current
                          ? "2px solid var(--accent)"
                          : "1px solid var(--border)",
                        "border-radius": "var(--radius-md)",
                        padding: "1rem",
                        "margin-bottom": "1rem",
                        display: "flex",
                        "justify-content": "space-between",
                        "align-items": "center",
                      }}
                    >
                      <div style={{ flex: "1" }}>
                        <div style={{ "margin-bottom": "0.5rem" }}>
                          <strong>
                            {session.is_current && (
                              <span
                                style={{
                                  color: "var(--accent)",
                                  "margin-right": "0.5rem",
                                }}
                              >
                                ● Current Session
                              </span>
                            )}
                            {!session.is_current && (
                              <span style={{ color: "var(--text-secondary)" }}>
                                Session
                              </span>
                            )}
                          </strong>
                        </div>
                        <div
                          style={{
                            "font-size": "0.85rem",
                            color: "var(--text-secondary)",
                          }}
                        >
                          <div>
                            Created: {formatDateTimePadded(session.created_at)}
                          </div>
                          <div>
                            Expires: {formatDateTimePadded(session.expires_at)}
                          </div>
                          <div
                            style={{
                              "font-family": "monospace",
                              "font-size": "0.75rem",
                              "margin-top": "0.25rem",
                            }}
                          >
                            ID: {session.family_id.substring(0, 16)}...
                          </div>
                        </div>
                      </div>
                      <div>
                        <Show when={!session.is_current}>
                          <button
                            class="btn btn-danger-outline"
                            style={{ "font-size": "0.85rem" }}
                            onClick={() => handleRevoke(session.family_id)}
                            disabled={revoking() === session.family_id}
                          >
                            {revoking() === session.family_id
                              ? "Revoking..."
                              : "Revoke"}
                          </button>
                        </Show>
                        <Show when={session.is_current}>
                          <span
                            style={{
                              color: "var(--text-secondary)",
                              "font-size": "0.85rem",
                            }}
                          >
                            (Active)
                          </span>
                        </Show>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          );
        }}
      </Show>
    </div>
  );
};

export default SessionsTab;
