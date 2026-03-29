/**
 * E2E tests for SteamCMD integration.
 *
 * Tests cover:
 * - Backend API: /api/system/steamcmd-status
 * - Backend API: /api/steamcmd/validate-app
 * - Template list includes steamcmd_available flag and requires_steamcmd per template
 * - Templates requiring SteamCMD are visually marked / disabled when steamcmd is unavailable
 * - Steam App ID field in server config editor (validation flow)
 * - SteamCMD step action types in pipeline editor
 * - Server creation with steam_app_id
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI, loginViaToken } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  getServer,
  createMinimalServerConfig,
  cleanupAllServers,
  enableRunCommands,
} from "../helpers/api";
import type { ApiClient } from "../helpers/api";

// ─── Helper: raw API request with auth ───────────────────────────────────────

async function apiGet<T>(client: ApiClient, path: string): Promise<T> {
  const res = await fetch(`${client.baseUrl}${path}`, {
    headers: { Authorization: `Bearer ${client.token}` },
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`GET ${path} => ${res.status}: ${text}`);
  }
  return res.json() as Promise<T>;
}

async function apiPost<T>(
  client: ApiClient,
  path: string,
  body: unknown,
): Promise<T> {
  const res = await fetch(`${client.baseUrl}${path}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${client.token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`POST ${path} => ${res.status}: ${text}`);
  }
  return res.json() as Promise<T>;
}

// ─── API-level tests ─────────────────────────────────────────────────────────

test.describe("SteamCMD API", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("GET /api/system/steamcmd-status returns a valid status object", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const status = await apiGet<{
      available: boolean;
      path: string | null;
      message: string | null;
    }>(client, "/api/system/steamcmd-status");

    // The response must have the `available` boolean regardless of host
    expect(typeof status.available).toBe("boolean");

    if (status.available) {
      expect(typeof status.path).toBe("string");
      expect(status.path).toBeTruthy();
    } else {
      expect(typeof status.message).toBe("string");
      expect(status.message).toBeTruthy();
    }
  });

  test("GET /api/steamcmd/validate-app returns valid=false for app_id=0", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const resp = await apiGet<{
      valid: boolean;
      app: null;
      error: string;
    }>(client, "/api/steamcmd/validate-app?app_id=0");

    expect(resp.valid).toBe(false);
    expect(resp.error).toBeTruthy();
    expect(resp.app).toBeNull();
  });

  test("GET /api/steamcmd/validate-app returns valid=false for non-existent app", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    // Use a very large unlikely app ID
    const resp = await apiGet<{
      valid: boolean;
      app: { app_id: number; name: string } | null;
      error: string | null;
    }>(client, "/api/steamcmd/validate-app?app_id=999999999");

    expect(resp.valid).toBe(false);
  });

  test("GET /api/steamcmd/validate-app returns valid=true for known app (Valheim Dedicated Server)", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const resp = await apiGet<{
      valid: boolean;
      app: { app_id: number; name: string } | null;
      error: string | null;
    }>(client, "/api/steamcmd/validate-app?app_id=896660");

    expect(resp.valid).toBe(true);
    expect(resp.app).not.toBeNull();
    expect(resp.app!.app_id).toBe(896660);
    expect(typeof resp.app!.name).toBe("string");
    expect(resp.app!.name.length).toBeGreaterThan(0);
  });
});

// ─── Template list tests ─────────────────────────────────────────────────────

test.describe("SteamCMD Template Metadata", () => {
  test("GET /api/templates includes steamcmd_available and requires_steamcmd fields", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const resp = await apiGet<{
      templates: Array<{
        id: string;
        name: string;
        requires_steamcmd: boolean;
        config: { steam_app_id: number | null };
      }>;
      steamcmd_available: boolean;
    }>(client, "/api/templates");

    // The response must contain the steamcmd_available flag
    expect(typeof resp.steamcmd_available).toBe("boolean");

    // Every template must have requires_steamcmd
    for (const t of resp.templates) {
      expect(typeof t.requires_steamcmd).toBe("boolean");
    }

    // The built-in Valheim template should require steamcmd
    const valheim = resp.templates.find((t) =>
      t.name.toLowerCase().includes("valheim"),
    );
    expect(valheim).toBeDefined();
    expect(valheim!.requires_steamcmd).toBe(true);
    expect(valheim!.config.steam_app_id).toBe(896660);

    // The built-in Minecraft Paper template should NOT require steamcmd
    const paper = resp.templates.find((t) =>
      t.name.toLowerCase().includes("paper"),
    );
    expect(paper).toBeDefined();
    expect(paper!.requires_steamcmd).toBe(false);
    expect(paper!.config.steam_app_id).toBeNull();
  });

  test("Valheim template uses SteamCmdInstall and SteamCmdUpdate step actions", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const resp = await apiGet<{
      templates: Array<{
        name: string;
        config: {
          install_steps: Array<{ action: { type: string } }>;
          update_steps: Array<{ action: { type: string } }>;
        };
      }>;
      steamcmd_available: boolean;
    }>(client, "/api/templates");

    const valheim = resp.templates.find((t) =>
      t.name.toLowerCase().includes("valheim"),
    );
    expect(valheim).toBeDefined();

    // Install steps should contain a steam_cmd_install action
    const installTypes = valheim!.config.install_steps.map(
      (s) => s.action.type,
    );
    expect(installTypes).toContain("steam_cmd_install");

    // Update steps should contain a steam_cmd_update action
    const updateTypes = valheim!.config.update_steps.map(
      (s) => s.action.type,
    );
    expect(updateTypes).toContain("steam_cmd_update");
  });
});

// ─── Server creation with steam_app_id ───────────────────────────────────────

test.describe("SteamCMD Server Config", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("can create a server with steam_app_id set", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const config = createMinimalServerConfig("SteamCMD Test Server");
    (config.config as any).steam_app_id = 896660;

    const created = await createServer(client, config);
    expect(created).toBeDefined();
    expect(created.server.config.steam_app_id).toBe(896660);

    // Verify it persists when fetched
    const fetched = await getServer(client, created.server.id);
    expect(fetched.server.config.steam_app_id).toBe(896660);
  });

  test("can create a server without steam_app_id (null)", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const config = createMinimalServerConfig("No Steam Server");
    const created = await createServer(client, config);
    expect(created).toBeDefined();
    expect(created.server.config.steam_app_id).toBeNull();
  });

  test("can create a server with SteamCmdInstall step action", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const config = createMinimalServerConfig("SteamCMD Pipeline Server");
    (config.config as any).steam_app_id = 896660;
    (config.config as any).install_steps = [
      {
        name: "Install via SteamCMD",
        description: "Download and install the server using SteamCMD",
        action: {
          type: "steam_cmd_install",
          app_id: null,
          anonymous: true,
          extra_args: [],
        },
        condition: null,
        continue_on_error: false,
      },
    ];
    (config.config as any).update_steps = [
      {
        name: "Update via SteamCMD",
        description: "Update the server using SteamCMD",
        action: {
          type: "steam_cmd_update",
          app_id: null,
          anonymous: true,
          extra_args: [],
        },
        condition: null,
        continue_on_error: false,
      },
    ];

    const created = await createServer(client, config);
    expect(created).toBeDefined();
    expect(created.server.config.steam_app_id).toBe(896660);
    expect(created.server.config.install_steps.length).toBe(1);
    expect(created.server.config.install_steps[0].action.type).toBe(
      "steam_cmd_install",
    );
    expect(created.server.config.update_steps.length).toBe(1);
    expect(created.server.config.update_steps[0].action.type).toBe(
      "steam_cmd_update",
    );
  });

  test("SteamCmdInstall step with app_id override persists", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const config = createMinimalServerConfig("Override App ID Server");
    (config.config as any).install_steps = [
      {
        name: "Install custom app",
        description: null,
        action: {
          type: "steam_cmd_install",
          app_id: 740,
          anonymous: true,
          extra_args: ["-beta", "experimental"],
        },
        condition: null,
        continue_on_error: false,
      },
    ];

    const created = await createServer(client, config);
    const step = created.server.config.install_steps[0];
    expect(step.action.type).toBe("steam_cmd_install");

    const action = step.action as {
      type: "steam_cmd_install";
      app_id: number | null;
      anonymous: boolean;
      extra_args: string[];
    };
    expect(action.app_id).toBe(740);
    expect(action.anonymous).toBe(true);
    expect(action.extra_args).toEqual(["-beta", "experimental"]);
  });
});

// ─── Template creation with SteamCMD ─────────────────────────────────────────

test.describe("SteamCMD Template Creation", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("creating a template with steam_app_id sets requires_steamcmd=true", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const template = await apiPost<{
      id: string;
      name: string;
      requires_steamcmd: boolean;
      config: { steam_app_id: number | null };
    }>(client, "/api/templates", {
      name: "SteamCMD Template",
      description: "Template that uses SteamCMD",
      config: {
        name: "Steam Server",
        binary: "./server",
        args: [],
        env: {},
        working_dir: null,
        auto_start: false,
        auto_restart: false,
        max_restart_attempts: 0,
        restart_delay_secs: 5,
        stop_command: null,
        stop_signal: "sigterm",
        stop_timeout_secs: 10,
        sftp_username: null,
        sftp_password: null,
        parameters: [],
        stop_steps: [],
        start_steps: [],
        install_steps: [
          {
            name: "Install",
            action: {
              type: "steam_cmd_install",
              app_id: null,
              anonymous: true,
              extra_args: [],
            },
            continue_on_error: false,
          },
        ],
        update_steps: [],
        uninstall_steps: [],
        isolation: {
          enabled: true,
          extra_read_paths: [],
          extra_rw_paths: [],
          pids_max: null,
        },
        update_check: null,
        steam_app_id: 896660,
      },
    });

    expect(template.requires_steamcmd).toBe(true);
    expect(template.config.steam_app_id).toBe(896660);

    // Clean up template
    await fetch(
      `${client.baseUrl}/api/templates/${encodeURIComponent(template.id)}`,
      {
        method: "DELETE",
        headers: { Authorization: `Bearer ${client.token}` },
      },
    );
  });

  test("creating a template without steam features sets requires_steamcmd=false", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const template = await apiPost<{
      id: string;
      name: string;
      requires_steamcmd: boolean;
      config: { steam_app_id: number | null };
    }>(client, "/api/templates", {
      name: "Plain Template",
      description: "No SteamCMD involved",
      config: {
        name: "Plain Server",
        binary: "/bin/echo",
        args: [],
        env: {},
        working_dir: null,
        auto_start: false,
        auto_restart: false,
        max_restart_attempts: 0,
        restart_delay_secs: 5,
        stop_command: null,
        stop_signal: "sigterm",
        stop_timeout_secs: 10,
        sftp_username: null,
        sftp_password: null,
        parameters: [],
        stop_steps: [],
        start_steps: [],
        install_steps: [],
        update_steps: [],
        uninstall_steps: [],
        isolation: {
          enabled: true,
          extra_read_paths: [],
          extra_rw_paths: [],
          pids_max: null,
        },
        update_check: null,
      },
    });

    expect(template.requires_steamcmd).toBe(false);
    expect(template.config.steam_app_id).toBeNull();

    // Clean up
    await fetch(
      `${client.baseUrl}/api/templates/${encodeURIComponent(template.id)}`,
      {
        method: "DELETE",
        headers: { Authorization: `Bearer ${client.token}` },
      },
    );
  });
});

// ─── Frontend UI tests ───────────────────────────────────────────────────────

test.describe("SteamCMD UI", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("Templates page shows SteamCMD badge on Valheim template", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // Look for the Valheim template card
    const valheimCard = page.locator(
      ".template-card:has-text('Valheim')",
    );
    await expect(valheimCard).toBeVisible({ timeout: 10000 });

    // It should have a SteamCMD badge (either green or red depending on availability)
    const steamBadge = valheimCard.locator(
      ":text('SteamCMD'), :text('SteamCMD Required')",
    );
    await expect(steamBadge).toBeVisible({ timeout: 5000 });
  });

  test("Templates page shows Steam App ID metadata on Valheim", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // Look for Steam App ID meta tag
    const valheimCard = page.locator(
      ".template-card:has-text('Valheim')",
    );
    await expect(valheimCard).toBeVisible({ timeout: 10000 });

    const appIdTag = valheimCard.locator(
      ".template-meta-tag:has-text('Steam App 896660')",
    );
    await expect(appIdTag).toBeVisible({ timeout: 5000 });
  });

  test("Minecraft Paper template does NOT have SteamCMD badge", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    const paperCard = page.locator(
      ".template-card:has-text('Minecraft Paper')",
    );
    await expect(paperCard).toBeVisible({ timeout: 10000 });

    // It should NOT have a SteamCMD badge
    const steamBadge = paperCard.locator(
      ":text('SteamCMD'), :text('SteamCMD Required')",
    );
    await expect(steamBadge).not.toBeVisible({ timeout: 2000 });
  });

  test("Create server page shows SteamCMD warning on Valheim when steamcmd is unavailable", async ({
    page,
    testEnv,
  }) => {
    // First check if steamcmd is available — if it IS available, skip this test
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const status = await apiGet<{ available: boolean }>(
      client,
      "/api/system/steamcmd-status",
    );

    if (status.available) {
      test.skip();
      return;
    }

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    // Ensure we're on the template tab
    const templateTab = page.locator(
      "button:has-text('Template'), .mode-tab:has-text('Template')",
    );
    const isVisible = await templateTab.isVisible({ timeout: 3000 }).catch(() => false);
    if (isVisible) {
      await templateTab.click();
      await page.waitForTimeout(500);
    }

    // Wait for templates to load
    await page.waitForSelector(".template-select-card", { timeout: 10000 });

    // Find the Valheim template card
    const valheimCard = page.locator(
      ".template-select-card:has-text('Valheim')",
    );
    await expect(valheimCard).toBeVisible({ timeout: 5000 });

    // It should be disabled (grayed out)
    await expect(valheimCard).toHaveClass(/template-select-card-disabled/, {
      timeout: 5000,
    });

    // It should show the SteamCMD warning text
    const warningText = valheimCard.locator(
      ":text('Requires SteamCMD')",
    );
    await expect(warningText).toBeVisible({ timeout: 5000 });
  });

  test("Create server page allows selecting non-SteamCMD templates", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    // Ensure we're on the template tab
    const templateTab = page.locator(
      "button:has-text('Template'), .mode-tab:has-text('Template')",
    );
    const isVisible = await templateTab.isVisible({ timeout: 3000 }).catch(() => false);
    if (isVisible) {
      await templateTab.click();
      await page.waitForTimeout(500);
    }

    // Wait for templates to load
    await page.waitForSelector(".template-select-card", { timeout: 10000 });

    // Find the Minecraft Paper template card (does NOT require SteamCMD)
    const paperCard = page.locator(
      ".template-select-card:has-text('Minecraft Paper')",
    );
    await expect(paperCard).toBeVisible({ timeout: 5000 });

    // It should NOT be disabled
    await expect(paperCard).not.toHaveClass(/template-select-card-disabled/);

    // Click it to select
    await paperCard.click();
    await page.waitForTimeout(500);

    // The template should be applied (we should see parameter fields or the template name)
    const serverNameOrParam = page.locator(
      "input[value*='Paper'], input[value*='Minecraft'], :text('Minecraft Paper')",
    );
    await expect(serverNameOrParam.first()).toBeVisible({ timeout: 5000 });
  });

  test("Config editor shows SteamCMD section with Steam App ID field", async ({
    page,
    testEnv,
  }) => {
    // Create a server first
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const config = createMinimalServerConfig("SteamCMD UI Test");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to Configuration tab
    const configTab = page.locator(
      "button:has-text('Configuration'), .tab-button:has-text('Configuration'), [role='tab']:has-text('Config')",
    );
    const tabVisible = await configTab.isVisible({ timeout: 5000 }).catch(() => false);
    if (tabVisible) {
      await configTab.click();
      await page.waitForTimeout(500);
    }

    // Look for the SteamCMD section heading
    const steamHeading = page.locator("h3:has-text('SteamCMD')");
    await expect(steamHeading).toBeVisible({ timeout: 10000 });

    // Look for the Steam App ID input
    const appIdInput = page.locator("input#cfg-steam-app-id");
    await expect(appIdInput).toBeVisible({ timeout: 5000 });
    expect(await appIdInput.inputValue()).toBe("");
  });

  test("Steam App ID field validates on blur and shows app name", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const config = createMinimalServerConfig("Steam Validate UI");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to Configuration tab
    const configTab = page.locator(
      "button:has-text('Configuration'), .tab-button:has-text('Configuration'), [role='tab']:has-text('Config')",
    );
    const tabVisible = await configTab.isVisible({ timeout: 5000 }).catch(() => false);
    if (tabVisible) {
      await configTab.click();
      await page.waitForTimeout(500);
    }

    // Find and fill the Steam App ID input
    const appIdInput = page.locator("input#cfg-steam-app-id");
    await expect(appIdInput).toBeVisible({ timeout: 10000 });

    await appIdInput.fill("896660");

    // Trigger blur to initiate validation
    await appIdInput.blur();

    // Should show "Validating..." temporarily
    // Then show the validated app name (green checkmark)
    const successIndicator = page.locator(
      ":text('✓')",
    );
    await expect(successIndicator).toBeVisible({ timeout: 15000 });
  });

  test("Steam App ID field shows error for invalid app ID", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    const config = createMinimalServerConfig("Steam Invalid UI");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to Configuration tab
    const configTab = page.locator(
      "button:has-text('Configuration'), .tab-button:has-text('Configuration'), [role='tab']:has-text('Config')",
    );
    const tabVisible = await configTab.isVisible({ timeout: 5000 }).catch(() => false);
    if (tabVisible) {
      await configTab.click();
      await page.waitForTimeout(500);
    }

    const appIdInput = page.locator("input#cfg-steam-app-id");
    await expect(appIdInput).toBeVisible({ timeout: 10000 });

    await appIdInput.fill("999999999");
    await appIdInput.blur();

    // Should show error indicator
    const errorIndicator = page.locator(":text('✗')");
    await expect(errorIndicator).toBeVisible({ timeout: 15000 });
  });
});

// ─── Pipeline step action types ──────────────────────────────────────────────

test.describe("SteamCMD Pipeline Steps", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("SteamCMD Install and Update actions appear in step action type dropdown", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    // Switch to wizard mode if available
    const wizardTab = page.locator(
      "button:has-text('Wizard'), .mode-tab:has-text('Wizard')",
    );
    const isVisible = await wizardTab.isVisible({ timeout: 3000 }).catch(() => false);
    if (isVisible) {
      await wizardTab.click();
      await page.waitForTimeout(500);
    }

    // Navigate to the Install Steps step in the wizard
    // Click through the wizard steps to reach Install
    const installStep = page.locator(
      ".wizard-stepper-item:has-text('Install'), button:has-text('Install Steps')",
    );
    const installVisible = await installStep.isVisible({ timeout: 3000 }).catch(() => false);
    if (installVisible) {
      await installStep.click();
      await page.waitForTimeout(500);
    } else {
      // Try clicking "Next" buttons to get to install step
      for (let i = 0; i < 4; i++) {
        const nextBtn = page.locator("button:has-text('Next')");
        const nextVisible = await nextBtn.isVisible({ timeout: 1000 }).catch(() => false);
        if (nextVisible) {
          await nextBtn.click();
          await page.waitForTimeout(300);
        }
      }
    }

    // Look for an "Add Step" button and click it
    const addStepBtn = page.locator(
      "button:has-text('Add Step'), button:has-text('Add step'), button:has-text('+ Step'), button:has-text('+ Add')",
    );
    const addVisible = await addStepBtn.isVisible({ timeout: 3000 }).catch(() => false);
    if (addVisible) {
      await addStepBtn.click();
      await page.waitForTimeout(500);
    }

    // Look for the action type dropdown
    const actionSelect = page.locator(
      "select",
    ).last();
    const selectVisible = await actionSelect.isVisible({ timeout: 3000 }).catch(() => false);

    if (selectVisible) {
      // Check that SteamCMD options exist in the dropdown
      const options = await actionSelect.locator("option").allTextContents();
      const hasInstall = options.some((o) =>
        o.toLowerCase().includes("steamcmd install"),
      );
      const hasUpdate = options.some((o) =>
        o.toLowerCase().includes("steamcmd update"),
      );

      expect(hasInstall).toBe(true);
      expect(hasUpdate).toBe(true);
    }
  });
});
