import {
  type Component,
  createSignal,
  Show,
} from "solid-js";
import Loader from "../Loader";
import { getAlertConfig, saveAlertConfig } from "../../api/client";
import type { AlertTriggers } from "../../types/bindings";

const AlertsTab: Component = () => {
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal(false);

  const [enabled, setEnabled] = createSignal(false);
  const [recipients, setRecipients] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [cooldownSecs, setCooldownSecs] = createSignal(300);
  const [triggers, setTriggers] = createSignal<AlertTriggers>({
    server_crashed: true,
    restart_exhausted: true,
    server_down: false,
    down_threshold_mins: 10,
    high_memory: false,
    memory_threshold_percent: 90,
    high_cpu: false,
    cpu_threshold_percent: 95,
    low_disk: false,
    disk_threshold_mb: 1024,
  });

  const loadConfig = async () => {
    setLoading(true);
    try {
      const config = await getAlertConfig();
      setEnabled(config.enabled);
      setRecipients(config.recipients.join("\n"));
      setBaseUrl(config.base_url || "");
      setCooldownSecs(config.cooldown_secs);
      setTriggers(config.triggers);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load alert config");
    } finally {
      setLoading(false);
    }
  };

  loadConfig();

  const updateTrigger = <K extends keyof AlertTriggers>(
    key: K,
    value: AlertTriggers[K],
  ) => {
    setTriggers((prev) => ({ ...prev, [key]: value }));
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(false);
    try {
      const recipientList = recipients()
        .split("\n")
        .map((r) => r.trim())
        .filter((r) => r.length > 0);

      const config = await saveAlertConfig({
        enabled: enabled(),
        recipients: recipientList,
        base_url: baseUrl().trim() || null,
        cooldown_secs: cooldownSecs(),
        triggers: triggers(),
      });
      setEnabled(config.enabled);
      setRecipients(config.recipients.join("\n"));
      setBaseUrl(config.base_url || "");
      setCooldownSecs(config.cooldown_secs);
      setTriggers(config.triggers);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 4000);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to save alert config");
    } finally {
      setSaving(false);
    }
  };

  const triggerRow = (
    label: string,
    description: string,
    toggleKey: keyof AlertTriggers,
    thresholdKey?: keyof AlertTriggers,
    thresholdLabel?: string,
    thresholdUnit?: string,
  ) => {
    const t = triggers();
    return (
      <div
        style={{
          display: "flex",
          "align-items": "flex-start",
          gap: "1rem",
          padding: "0.75rem 0",
          "border-bottom": "1px solid var(--border)",
        }}
      >
        <div style={{ flex: 1 }}>
          <label
            style={{
              display: "flex",
              "align-items": "center",
              gap: "0.5rem",
              cursor: "pointer",
              "font-weight": "500",
            }}
          >
            <input
              type="checkbox"
              checked={t[toggleKey] as boolean}
              onChange={(e) =>
                updateTrigger(toggleKey, e.currentTarget.checked as never)
              }
            />
            {label}
          </label>
          <p
            style={{
              color: "var(--text-dim)",
              "font-size": "0.85rem",
              margin: "0.25rem 0 0 1.6rem",
            }}
          >
            {description}
          </p>
        </div>
        <Show when={thresholdKey}>
          <div style={{ "min-width": "120px", "text-align": "right" }}>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "0.4rem",
                "justify-content": "flex-end",
              }}
            >
              <input
                type="number"
                value={t[thresholdKey!] as number}
                onInput={(e) =>
                  updateTrigger(
                    thresholdKey!,
                    parseFloat(e.currentTarget.value) || (0 as never),
                  )
                }
                style={{ width: "80px", "text-align": "right" }}
                disabled={!t[toggleKey]}
              />
              <span
                style={{
                  color: "var(--text-dim)",
                  "font-size": "0.85rem",
                  "white-space": "nowrap",
                }}
              >
                {thresholdUnit}
              </span>
            </div>
            <Show when={thresholdLabel}>
              <small
                style={{ color: "var(--text-dim)", "font-size": "0.75rem" }}
              >
                {thresholdLabel}
              </small>
            </Show>
          </div>
        </Show>
      </div>
    );
  };

  return (
    <div class="admin-settings" style={{ "max-width": "640px" }}>
      <h2>Email Alert Configuration</h2>
      <p
        style={{
          color: "var(--text-muted)",
          "font-size": "0.9rem",
          "margin-bottom": "1.5rem",
        }}
      >
        Configure when and where AnyServer sends email notifications. SMTP must
        be configured first.
      </p>

      <Show when={error()}>
        {(err) => (
          <div class="error-msg" style={{ "margin-bottom": "1rem" }}>
            {err()}
          </div>
        )}
      </Show>
      <Show when={success()}>
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
          ✓ Alert configuration saved.
        </div>
      </Show>

      <Show
        when={!loading()}
        fallback={<Loader message="Loading alert configuration" />}
      >
        <div class="admin-setting-row" style={{ "margin-bottom": "1.5rem" }}>
          <div class="admin-setting-info">
            <h4>Master Switch</h4>
            <p>
              When disabled, no email alerts are sent regardless of other
              settings.
            </p>
          </div>
          <div class="admin-setting-control">
            <button
              class={`btn ${enabled() ? "btn-danger" : "btn-success"}`}
              onClick={() => setEnabled(!enabled())}
            >
              {enabled() ? "Disable Alerts" : "Enable Alerts"}
            </button>
          </div>
        </div>

        <div class="form-group">
          <label for="alert-recipients">Recipients (one per line)</label>
          <textarea
            id="alert-recipients"
            value={recipients()}
            onInput={(e) => setRecipients(e.currentTarget.value)}
            placeholder={"admin@example.com\nops@example.com"}
            rows={3}
            style={{
              resize: "vertical",
              "font-family": "inherit",
              "font-size": "0.9rem",
              width: "100%",
              padding: "0.5rem 0.75rem",
              background: "var(--bg-input)",
              color: "var(--text)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-sm)",
            }}
          />
        </div>

        <div style={{ display: "flex", gap: "1rem" }}>
          <div class="form-group" style={{ flex: 2 }}>
            <label for="alert-base-url">Base URL (for email links)</label>
            <input
              id="alert-base-url"
              type="text"
              value={baseUrl()}
              onInput={(e) => setBaseUrl(e.currentTarget.value)}
              placeholder="https://my.server.com:3001"
            />
          </div>
          <div class="form-group" style={{ flex: 1 }}>
            <label for="alert-cooldown">Cooldown (sec)</label>
            <input
              id="alert-cooldown"
              type="number"
              value={cooldownSecs()}
              onInput={(e) =>
                setCooldownSecs(parseInt(e.currentTarget.value) || 0)
              }
              min={0}
            />
          </div>
        </div>

        <h3 style={{ "margin-top": "1.5rem", "margin-bottom": "0.5rem" }}>
          Alert Triggers
        </h3>
        <p
          style={{
            color: "var(--text-dim)",
            "font-size": "0.85rem",
            "margin-bottom": "0.75rem",
          }}
        >
          Choose which events trigger an email notification.
        </p>

        {triggerRow(
          "Server Crashed",
          "A server process exited unexpectedly.",
          "server_crashed",
        )}
        {triggerRow(
          "Restart Attempts Exhausted",
          "Auto-restart reached the maximum number of attempts.",
          "restart_exhausted",
        )}
        {triggerRow(
          "Server Down",
          "A server has been stopped or crashed for too long.",
          "server_down",
          "down_threshold_mins",
          "threshold",
          "min",
        )}
        {triggerRow(
          "High Memory Usage",
          "A server's memory exceeds the threshold (% of system total).",
          "high_memory",
          "memory_threshold_percent",
          "threshold",
          "%",
        )}
        {triggerRow(
          "High CPU Usage",
          "A server's CPU usage exceeds the threshold.",
          "high_cpu",
          "cpu_threshold_percent",
          "threshold",
          "%",
        )}
        {triggerRow(
          "Low Disk Space",
          "The data partition has less free space than the threshold.",
          "low_disk",
          "disk_threshold_mb",
          "threshold",
          "MB",
        )}

        <div style={{ "margin-top": "1.5rem" }}>
          <button
            class="btn btn-primary"
            onClick={handleSave}
            disabled={saving()}
          >
            {saving() ? "Saving..." : "Save Alert Configuration"}
          </button>
        </div>
      </Show>
    </div>
  );
};

export default AlertsTab;
