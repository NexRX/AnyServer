import { type Component, createSignal, Show } from "solid-js";
import { getServerAlerts, updateServerAlerts } from "../api/client";

export interface AlertMuteSectionProps {
  serverId: string;
}

const AlertMuteSection: Component<AlertMuteSectionProps> = (props) => {
  const [alertMuted, setAlertMuted] = createSignal<boolean | null>(null);
  const [alertSaving, setAlertSaving] = createSignal(false);
  const [alertError, setAlertError] = createSignal<string | null>(null);

  const loadAlertState = async () => {
    try {
      const config = await getServerAlerts(props.serverId);
      setAlertMuted(config.muted);
    } catch {}
  };
  loadAlertState();

  const toggleMute = async () => {
    const current = alertMuted();
    if (current === null) return;
    setAlertSaving(true);
    setAlertError(null);
    try {
      const result = await updateServerAlerts(props.serverId, {
        muted: !current,
      });
      setAlertMuted(result.muted);
    } catch (e: unknown) {
      setAlertError(
        e instanceof Error ? e.message : "Failed to update alert settings",
      );
    } finally {
      setAlertSaving(false);
    }
  };

  return (
    <div class="alert-mute-section">
      <div class="alert-mute-header">
        <div>
          <h4 class="alert-mute-title">Email Alerts</h4>
          <p class="alert-mute-description">
            {alertMuted()
              ? "Alerts are muted for this server. No email notifications will be sent."
              : "Alerts are active for this server (when globally enabled)."}
          </p>
        </div>
        <Show when={alertMuted() !== null}>
          <button
            class={`btn btn-sm ${alertMuted() ? "btn-success" : "btn-danger-outline"}`}
            onClick={toggleMute}
            disabled={alertSaving()}
          >
            {alertSaving() ? "Saving..." : alertMuted() ? "Unmute" : "Mute"}
          </button>
        </Show>
      </div>
      <Show when={alertError()}>
        {(err) => <div class="error-msg alert-mute-error">{err()}</div>}
      </Show>
    </div>
  );
};

export default AlertMuteSection;
