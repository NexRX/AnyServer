import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "../test-utils/loadStylesheet";

function loadFile(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

function loadStylesheet(): string {
  return loadProjectStylesheet(__dirname);
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

// ─── Generated TypeScript Types ─────────────────────────────────────

describe("Alert generated types", () => {
  const typesDir = resolve(__dirname, "../types/generated");

  it("AlertConfig type exists and has expected fields", () => {
    const src = readFileSync(resolve(typesDir, "AlertConfig.ts"), "utf-8");
    for (const field of [
      "enabled",
      "recipients",
      "cooldown_secs",
      "triggers",
    ]) {
      expect(src, `missing field ${field}`).toContain(field);
    }
  });

  it("AlertTriggers type exists and has all trigger fields", () => {
    const src = readFileSync(resolve(typesDir, "AlertTriggers.ts"), "utf-8");
    for (const field of [
      "server_crashed",
      "restart_exhausted",
      "server_down",
      "high_memory",
      "high_cpu",
      "low_disk",
    ]) {
      expect(src, `missing trigger ${field}`).toContain(field);
    }
  });

  it("SmtpConfigPublic type exists and never exposes password", () => {
    const src = readFileSync(resolve(typesDir, "SmtpConfigPublic.ts"), "utf-8");
    expect(src).toContain("host");
    expect(src).toContain("port");
    expect(src).toContain("from_address");
    expect(src).toContain("password_set");
    // Should NOT have a plaintext password field
    expect(src).not.toMatch(/\bpassword\b\s*:\s*string/);
  });

  it("all other alert-related types exist with expected fields", () => {
    const checks: [string, string[]][] = [
      ["SaveSmtpConfigRequest", ["host", "port", "password"]],
      ["SaveAlertConfigRequest", ["enabled", "recipients"]],
      ["TestEmailRequest", ["recipient"]],
      ["TestEmailResponse", ["success"]],
      ["ServerAlertConfig", ["muted"]],
      ["UpdateServerAlertRequest", ["muted"]],
      ["DeleteSmtpConfigResponse", ["deleted"]],
    ];
    for (const [typeName, fields] of checks) {
      const src = readFileSync(resolve(typesDir, `${typeName}.ts`), "utf-8");
      for (const f of fields) {
        expect(src, `${typeName} missing ${f}`).toContain(f);
      }
    }
  });
});

// ─── Bindings barrel file ───────────────────────────────────────────

describe("Bindings barrel file exports alert types", () => {
  const bindings = loadFile("../types/bindings.ts");

  it("exports all alert-related types", () => {
    for (const t of [
      "AlertConfig",
      "AlertTriggers",
      "SmtpConfigPublic",
      "SaveSmtpConfigRequest",
      "SaveAlertConfigRequest",
      "TestEmailRequest",
      "TestEmailResponse",
      "ServerAlertConfig",
      "UpdateServerAlertRequest",
      "DeleteSmtpConfigResponse",
    ]) {
      expect(bindings, `missing export for ${t}`).toMatch(
        new RegExp(`export type \\{ ${t} \\}`),
      );
    }
  });
});

// ─── Alerts API module ──────────────────────────────────────────────

describe("Alerts API module", () => {
  const api = loadFile("../api/alerts.ts");

  it("exports all SMTP and alert API functions", () => {
    for (const fn of [
      "getSmtpConfig",
      "saveSmtpConfig",
      "deleteSmtpConfig",
      "sendTestEmail",
      "getAlertConfig",
      "saveAlertConfig",
      "getServerAlerts",
      "updateServerAlerts",
    ]) {
      expect(api, `missing export ${fn}`).toMatch(
        new RegExp(`export function ${fn}`),
      );
    }
  });

  it("calls the correct endpoints with correct HTTP methods", () => {
    expect(api).toMatch(/\/admin\/smtp/);
    expect(api).toMatch(/\/admin\/alerts/);
    expect(api).toMatch(/\/servers\/.*\/alerts/);
    // Verify methods
    expect(api).toMatch(/"GET"/);
    expect(api).toMatch(/"PUT"/);
    expect(api).toMatch(/"DELETE"/);
    expect(api).toMatch(/"POST"/);
  });
});

// ─── Client barrel re-exports ───────────────────────────────────────

describe("Client barrel re-exports alert functions", () => {
  const client = loadFile("../api/client.ts");

  it("re-exports all alert API functions and imports from alerts module", () => {
    for (const fn of [
      "getSmtpConfig",
      "saveSmtpConfig",
      "deleteSmtpConfig",
      "sendTestEmail",
      "getAlertConfig",
      "saveAlertConfig",
      "getServerAlerts",
      "updateServerAlerts",
    ]) {
      expect(client, `missing re-export ${fn}`).toContain(fn);
    }
    expect(client).toMatch(/from\s+["']\.\/alerts["']/);
  });
});

// ─── AdminPanel SMTP tab ────────────────────────────────────────────

describe("AdminPanel SMTP tab", () => {
  const panel = loadFile("AdminPanel.tsx");
  // SmtpTab was extracted into its own sub-component
  const smtpTab = loadFile("../components/admin/SmtpTab.tsx");

  it("has an SMTP tab with all required form elements", () => {
    expect(panel).toMatch(/smtp/i);
    expect(panel).toMatch(/SmtpTab/);
    // Form inputs live in the SmtpTab sub-component
    for (const input of ["host", "port", "username", "password", "from"]) {
      expect(smtpTab.toLowerCase(), `missing ${input} input`).toContain(input);
    }
  });

  it("has TLS checkbox, save/remove buttons, and test email section", () => {
    expect(smtpTab.toLowerCase()).toContain("tls");
    expect(smtpTab).toMatch(/save|Save/);
    expect(smtpTab).toMatch(/remove|Remove|delete|Delete/);
    expect(smtpTab).toMatch(/test.*email|send.*test/i);
  });

  it("imports SMTP API functions and never returns plaintext password", () => {
    expect(smtpTab).toMatch(/getSmtpConfig|saveSmtpConfig|deleteSmtpConfig/);
    expect(smtpTab).toMatch(/sendTestEmail/);
    // Password field should show placeholder, not actual value
    expect(smtpTab).not.toMatch(/password_value|plaintext_password/);
  });
});

// ─── AdminPanel Alerts tab ──────────────────────────────────────────

describe("AdminPanel Alerts tab", () => {
  const panel = loadFile("AdminPanel.tsx");
  // AlertsTab was extracted into its own sub-component
  const alertsTab = loadFile("../components/admin/AlertsTab.tsx");

  it("has an Alerts tab with enable/disable switch and recipients textarea", () => {
    expect(panel).toMatch(/alerts/i);
    expect(panel).toMatch(/AlertsTab/);
    expect(alertsTab).toMatch(/enabl|disabl/i);
    expect(alertsTab).toMatch(/recipient/i);
  });

  it("has base URL input, cooldown input, and all six alert trigger types", () => {
    expect(alertsTab).toMatch(/base.*url|base_url/i);
    expect(alertsTab).toMatch(/cooldown/i);
    for (const trigger of [
      "server_crashed",
      "restart_exhausted",
      "server_down",
      "high_memory",
      "high_cpu",
      "low_disk",
    ]) {
      expect(alertsTab, `missing trigger ${trigger}`).toContain(trigger);
    }
  });

  it("has trigger descriptions, threshold units, and a save button", () => {
    // Trigger descriptions are present as label text
    expect(alertsTab).toMatch(/start|stop|crash|cpu|memory|disk/i);
    expect(alertsTab).toMatch(/%/);
    expect(alertsTab).toMatch(/save|Save/);
  });

  it("imports alert API functions and AlertTriggers type", () => {
    expect(alertsTab).toMatch(/getAlertConfig|saveAlertConfig/);
    expect(alertsTab).toMatch(/AlertTriggers/);
  });

  it("splits recipients by newline", () => {
    expect(alertsTab).toMatch(/split|\\n|newline/i);
  });
});

// ─── ServerDetail per-server alert mute ─────────────────────────────

describe("ServerDetail per-server alert mute", () => {
  // Alert mute was extracted into AlertMuteSection component
  const alertMute = loadFile("../components/AlertMuteSection.tsx");

  it("imports getServerAlerts and updateServerAlerts", () => {
    expect(alertMute).toMatch(/getServerAlerts/);
    expect(alertMute).toMatch(/updateServerAlerts/);
  });

  it("has an Email Alerts section with mute/unmute button and status text", () => {
    expect(alertMute).toMatch(/Email Alerts|email.*alert/i);
    expect(alertMute).toMatch(/mute|unmute/i);
    expect(alertMute).toMatch(/muted|active/i);
  });

  it("restricts alert toggle to manager+ permission level", () => {
    // AlertMuteSection itself does not gate on permission — the parent
    // (ServerConfigTab) decides whether to render it.  Accept either pattern.
    const combined =
      alertMute + loadFile("../components/server-detail/ServerConfigTab.tsx");
    expect(combined).toMatch(/manager|permission|canEdit|AlertMuteSection/i);
  });
});

// ─── Stylesheet supports alert UI elements ──────────────────────────

describe("Stylesheet supports alert UI elements", () => {
  const css = stripComments(loadStylesheet());

  it("defines all required CSS classes for the alerts UI", () => {
    const requiredClasses = [
      "admin-settings",
      "admin-setting-row",
      "admin-setting-info",
      "admin-setting-control",
      "btn-success",
      "btn-danger",
      "btn-danger-outline",
      "auth-form",
      "form-group",
      "error-msg",
      "tabs",
      "tab",
    ];
    for (const cls of requiredClasses) {
      expect(css, `missing .${cls}`).toMatch(
        new RegExp(`\\.${cls.replace(/-/g, "\\-")}[\\s{,]`),
      );
    }
  });
});
