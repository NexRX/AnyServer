/**
 * Pipeline stale-state E2E tests.
 *
 * These tests target a class of bugs where the HTTP-fetched `data()` resource
 * in ServerDetail.tsx becomes stale after a pipeline completes via WebSocket.
 *
 * The original bug: after installing a server for the first time and clicking
 * Start, the "Server Not Installed" dialog reappears because `data().server.installed`
 * was never refetched after the WebSocket `PhaseProgress` message arrived with
 * status "completed".
 *
 * The fix adds a `createEffect` that watches `sc.phaseProgress()` and calls
 * `refetch()` when a phase completes or fails. These tests verify:
 *
 * Install → Start stale-state bug:
 *  1. Install via Start dialog → Start again works without dialog
 *  2. Install via Install button → Start works without dialog
 *  3. Install button label changes from "Install" to "Reinstall"
 *
 * Uninstall pipeline stale-state:
 *  4. Uninstall button appears after install completes
 *  5. Uninstall button disappears after uninstall completes
 *  6. After uninstall, Install button reverts to "Install"
 *  7. After uninstall, Start re-shows the install dialog
 *
 * Full pipeline round-trip stale-state:
 *  8. Full round-trip: install → uninstall → install → start
 *  9. Failed install does NOT mark server as installed
 * 10. Mark-as-installed → Start works without dialog
 *
 * Phase progress UI stale-state:
 * 11. Phase progress banner clears after pipeline completes
 * 12. Cancelling install pipeline leaves server uninstalled
 *
 * Reset stale-state:
 * 13. Server reset → Start shows install dialog again
 *
 * Update pipeline stale-state:
 * 14. Update pipeline completes without leaving stale state
 *
 * Multiple rapid pipeline cycles:
 * 15. Two install/uninstall cycles leave consistent state
 * 16. Start → Install dialog → Install → Reinstall → Start all work
 *
 * Page refresh preserves correct state after pipelines:
 * 17. Refresh after install reflects installed state correctly
 * 18. Refresh after uninstall reflects uninstalled state correctly
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  cleanupAllServers,
  getServer,
  installServer,
  waitForPhaseComplete,
  waitForStatus,
  stopServer,
  enableRunCommands,
  resetServer,
} from "../helpers/api";
import type { CreateServerRequest } from "../../src/types/bindings";

// ─── Helpers ────────────────────────────────────────────────────────────────

/**
 * Create a server config with install steps (quick echo).
 */
function createServerWithInstallSteps(name: string): CreateServerRequest {
  return {
    config: {
      name,
      binary: "/run/current-system/sw/bin/sleep",
      args: ["infinity"],
      env: {},
      working_dir: null,
      auto_start: false,
      auto_restart: false,
      max_restart_attempts: 5,
      restart_delay_secs: 5,
      stop_command: null,
      stop_signal: "sigterm" as const,
      stop_timeout_secs: 5,
      sftp_username: null,
      sftp_password: null,
      parameters: [],
      stop_steps: [],
      start_steps: [],
      install_steps: [
        {
          name: "Setup files",
          description: null,
          action: {
            type: "run_command",
            command: "/run/current-system/sw/bin/echo",
            args: ["installed successfully"],
            working_dir: null,
            env: {},
          },
          continue_on_error: false,
          condition: null,
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
      log_to_disk: false,
      max_log_size_mb: 50,
      enable_java_helper: false,
      enable_dotnet_helper: false,
      steam_app_id: null,
    },
    parameter_values: {},
    source_template_id: null,
  };
}

/**
 * Create a server config with both install AND uninstall steps.
 */
function createServerWithInstallAndUninstall(
  name: string,
): CreateServerRequest {
  const config = createServerWithInstallSteps(name);
  config.config.uninstall_steps = [
    {
      name: "Cleanup files",
      description: null,
      action: {
        type: "run_command",
        command: "/run/current-system/sw/bin/echo",
        args: ["uninstalled successfully"],
        working_dir: null,
        env: {},
      },
      continue_on_error: false,
      condition: null,
    },
  ];
  return config;
}

/**
 * Create a server config with install, update, AND uninstall steps.
 */
function createServerWithAllPipelines(name: string): CreateServerRequest {
  const config = createServerWithInstallAndUninstall(name);
  config.config.update_steps = [
    {
      name: "Apply update",
      description: null,
      action: {
        type: "run_command",
        command: "/run/current-system/sw/bin/echo",
        args: ["updated successfully"],
        working_dir: null,
        env: {},
      },
      continue_on_error: false,
      condition: null,
    },
  ];
  return config;
}

/**
 * Create a server config with a FAILING install step.
 */
function createServerWithFailingInstall(name: string): CreateServerRequest {
  const config = createServerWithInstallSteps(name);
  config.config.install_steps = [
    {
      name: "Failing step",
      description: null,
      action: {
        type: "run_command",
        command: "/run/current-system/sw/bin/false",
        args: [],
        working_dir: null,
        env: {},
      },
      continue_on_error: false,
      condition: null,
    },
  ];
  return config;
}

/**
 * Create a server config with a slow install step (for cancel tests).
 */
function createServerWithSlowInstall(name: string): CreateServerRequest {
  const config = createServerWithInstallSteps(name);
  config.config.install_steps = [
    {
      name: "Slow step",
      description: null,
      action: {
        type: "run_command",
        command: "/run/current-system/sw/bin/sleep",
        args: ["30"],
        working_dir: null,
        env: {},
      },
      continue_on_error: false,
      condition: null,
    },
  ];
  return config;
}

/**
 * Poll until `server.server.installed` reaches the expected value.
 */
async function waitForInstalledState(
  client: ReturnType<typeof createApiClient>,
  serverId: string,
  expectedInstalled: boolean,
  timeout = 15000,
  interval = 250,
): Promise<void> {
  const startTime = Date.now();
  while (Date.now() - startTime < timeout) {
    const server = await getServer(client, serverId);
    if (server.server.installed === expectedInstalled) return;
    await new Promise((resolve) => setTimeout(resolve, interval));
  }
  throw new Error(
    `Server ${serverId} did not reach installed=${expectedInstalled} within ${timeout}ms`,
  );
}

// ─── Test Suites ────────────────────────────────────────────────────────────

test.describe("Install → Start stale-state bug", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 1. Core bug: Start → Install dialog → Install → Start again
  // ────────────────────────────────────────────────────────────────────────

  test("Start → Install dialog → Install → Start again should NOT show install dialog", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallSteps("bug-repro-start-install");
    const server = await createServer(client, config);
    expect(server.server.installed).toBe(false);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Click Start — should show install dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeVisible({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await expect(installDialog).toBeVisible({ timeout: 5000 });
    await expect(installDialog).toContainText("Server Not Installed");

    // Click Install in the dialog
    const installOption = installDialog.locator(
      "button.btn-success:has-text('Install')",
    );
    await expect(installOption).toBeVisible({ timeout: 3000 });
    await installOption.click();

    // Dialog should close
    await expect(installDialog).not.toBeVisible({ timeout: 5000 });

    // Wait for install to finish
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    const serverAfterInstall = await getServer(client, server.server.id);
    expect(serverAfterInstall.server.installed).toBe(true);

    // Wait for status to return to stopped
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Click Start again — the bug made this show the dialog again
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    // The install dialog must NOT reappear
    await page.waitForTimeout(1000);
    await expect(installDialog).not.toBeVisible();

    // Server should transition to running
    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 2. Install button → Start should not show dialog
  // ────────────────────────────────────────────────────────────────────────

  test("Install button → wait → Start should NOT show install dialog", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallSteps("bug-repro-install-btn");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Click the dedicated Install button
    const installBtn = page.locator("button:has-text('Install')").first();
    await expect(installBtn).toBeVisible({ timeout: 5000 });
    await installBtn.click();

    // Wait for install to complete
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Click Start — must not show install dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await page.waitForTimeout(1000);
    await expect(installDialog).not.toBeVisible();

    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 3. Install button label changes Install → Reinstall
  // ────────────────────────────────────────────────────────────────────────

  test("Install button text changes from 'Install' to 'Reinstall' after install completes", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallSteps("bug-repro-reinstall-label");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Before install, button should say "Install" (not "Reinstall")
    const installBtn = page.locator("button:has-text('Install')").first();
    await expect(installBtn).toBeVisible({ timeout: 5000 });
    await expect(installBtn).not.toContainText("Reinstall");

    // Run install
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    // After install, button should say "Reinstall"
    const reinstallBtn = page.locator("button:has-text('Reinstall')").first();
    await expect(reinstallBtn).toBeVisible({ timeout: 15000 });
  });
});

test.describe("Uninstall pipeline stale-state", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 4. Uninstall button appears after install
  // ────────────────────────────────────────────────────────────────────────

  test("Uninstall button appears after install completes", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall("uninstall-appears");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Before install, Uninstall button should NOT be visible
    // (Show condition: hasUninstallSteps() && server().installed)
    const uninstallBtn = page.locator("button:has-text('Uninstall')");
    await expect(uninstallBtn).not.toBeVisible({ timeout: 3000 });

    // Run install
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    // After install, Uninstall button should now appear
    await expect(uninstallBtn).toBeVisible({ timeout: 15000 });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 5. Uninstall button disappears after uninstall
  // ────────────────────────────────────────────────────────────────────────

  test("Uninstall button disappears after uninstall completes", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall("uninstall-disappears");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Install first
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    // Uninstall button should be visible
    const uninstallBtn = page.locator("button:has-text('Uninstall')");
    await expect(uninstallBtn).toBeVisible({ timeout: 15000 });

    // Click Uninstall
    await uninstallBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, false);

    // Uninstall button should disappear
    await expect(uninstallBtn).not.toBeVisible({ timeout: 15000 });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 6. After uninstall, Install button reverts to "Install"
  // ────────────────────────────────────────────────────────────────────────

  test("after uninstall, Install button reverts from 'Reinstall' to 'Install'", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall(
      "reinstall-label-revert",
    );
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Install
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    // Should now say "Reinstall"
    await expect(
      page.locator("button:has-text('Reinstall')").first(),
    ).toBeVisible({ timeout: 15000 });

    // Uninstall
    const uninstallBtn = page.locator("button:has-text('Uninstall')");
    await expect(uninstallBtn).toBeVisible({ timeout: 5000 });
    await uninstallBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, false);

    // Should revert to "Install" (not "Reinstall")
    const freshInstallBtn = page
      .locator("button:has-text('Install'):not(:has-text('Reinstall'))")
      .first();
    await expect(freshInstallBtn).toBeVisible({ timeout: 15000 });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 7. After uninstall, Start shows the install dialog again
  // ────────────────────────────────────────────────────────────────────────

  test("after uninstall, clicking Start shows install dialog again", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall("uninstall-then-start");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Install first
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Verify Start works without dialog after install
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await page.waitForTimeout(500);
    await expect(installDialog).not.toBeVisible();

    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });

    // Stop the server
    await stopServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "stopped", 15000);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Uninstall
    const uninstallBtn = page.locator("button:has-text('Uninstall')");
    await expect(uninstallBtn).toBeVisible({ timeout: 10000 });
    await uninstallBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, false);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Now Start should show the install dialog again
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();
    await expect(installDialog).toBeVisible({ timeout: 5000 });
    await expect(installDialog).toContainText("Server Not Installed");

    // Dismiss dialog
    const cancelBtn = installDialog.locator("button:has-text('Cancel')");
    await cancelBtn.click();
    await expect(installDialog).not.toBeVisible({ timeout: 3000 });
  });
});

test.describe("Full pipeline round-trip stale-state", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 8. Full lifecycle: install → uninstall → install → start
  // ────────────────────────────────────────────────────────────────────────

  test("install → uninstall → install → start keeps state consistent", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall("full-roundtrip");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    const installDialog = page.locator('[data-testid="install-dialog"]');

    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // ── Round 1: Install ──
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Verify button says "Reinstall"
    await expect(
      page.locator("button:has-text('Reinstall')").first(),
    ).toBeVisible({ timeout: 10000 });

    // ── Round 2: Uninstall ──
    const uninstallBtn = page.locator("button:has-text('Uninstall')");
    await expect(uninstallBtn).toBeVisible({ timeout: 5000 });
    await uninstallBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, false);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Verify uninstall button gone and install label reverted
    await expect(uninstallBtn).not.toBeVisible({ timeout: 10000 });

    // Start should show dialog since we're uninstalled again
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();
    await expect(installDialog).toBeVisible({ timeout: 5000 });
    // Dismiss dialog
    await installDialog.locator("button:has-text('Cancel')").click();
    await expect(installDialog).not.toBeVisible({ timeout: 3000 });

    // ── Round 3: Install again ──
    const installBtnAgain = page.locator("button:has-text('Install')").first();
    await installBtnAgain.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // ── Round 4: Start — should go straight to running ──
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    await page.waitForTimeout(500);
    await expect(installDialog).not.toBeVisible();

    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 9. Failed install does NOT mark server as installed
  // ────────────────────────────────────────────────────────────────────────

  test("failed install does NOT mark server as installed", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithFailingInstall("failing-install");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Run install — it should fail
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);

    // Wait a beat for refetch to settle
    await page.waitForTimeout(2000);

    // Verify NOT installed via API
    const serverAfter = await getServer(client, server.server.id);
    expect(serverAfter.server.installed).toBe(false);

    // The install button should still say "Install" (not "Reinstall")
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });
    await expect(installBtn).toBeVisible({ timeout: 5000 });
    await expect(installBtn).not.toContainText("Reinstall");

    // Clicking Start should still show the install dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await expect(installDialog).toBeVisible({ timeout: 5000 });
    await expect(installDialog).toContainText("Server Not Installed");

    // Dismiss
    await installDialog.locator("button:has-text('Cancel')").click();
  });

  // ────────────────────────────────────────────────────────────────────────
  // 10. Mark as installed → Start works without dialog
  // ────────────────────────────────────────────────────────────────────────

  test("Mark as installed via dialog → Start works without install dialog", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const config = createServerWithInstallSteps("mark-installed-start");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Click Start to open the install dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await expect(installDialog).toBeVisible({ timeout: 5000 });

    // Click "Mark as Installed"
    const markBtn = installDialog.locator(
      "button:has-text('Mark as Installed')",
    );
    await expect(markBtn).toBeVisible({ timeout: 3000 });
    await markBtn.click();

    // Dialog closes
    await expect(installDialog).not.toBeVisible({ timeout: 5000 });

    // Wait for the mark-installed to be reflected
    await waitForInstalledState(client, server.server.id, true);

    // Small delay for the refetch triggered by handleInstallDialogMarkInstalled
    await page.waitForTimeout(1000);

    // Now click Start — should go straight to starting without dialog
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    await page.waitForTimeout(500);
    await expect(installDialog).not.toBeVisible();

    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });
  });
});

test.describe("Phase progress UI stale-state", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 11. Phase progress clears after pipeline completes
  // ────────────────────────────────────────────────────────────────────────

  test("phase progress is not stuck showing after install completes", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallSteps("progress-clears");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Run install
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();

    // Wait for install to complete
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    // After completion, the status should be stopped (not installing)
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // The "Cancel Pipeline" button should NOT be visible (no phase running)
    const cancelPipelineBtn = page.locator(
      "button:has-text('Cancel Pipeline')",
    );
    await expect(cancelPipelineBtn).not.toBeVisible({ timeout: 5000 });

    // Start and stop buttons should be in correct states
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 12. Cancel pipeline → server stays uninstalled
  // ────────────────────────────────────────────────────────────────────────

  test("cancelling install pipeline leaves server uninstalled", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    // Use a slow install so we can cancel it
    const config = createServerWithSlowInstall("cancel-install");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Start install
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();

    // The backend does NOT set runtime status to "installing" — it stays
    // "stopped" while the pipeline runs.  The pipeline progress is tracked
    // separately via PhaseProgress, which drives the "Cancel Pipeline"
    // button visibility.  Wait for that button instead of a status change.
    const cancelBtn = page.locator("button:has-text('Cancel Pipeline')");
    await expect(cancelBtn).toBeVisible({ timeout: 15000 });

    // Cancel the pipeline
    await cancelBtn.click();

    // Wait for the pipeline to actually stop
    await waitForPhaseComplete(client, server.server.id, 15000);

    // Wait for status to return to stopped
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Verify NOT installed
    const serverAfter = await getServer(client, server.server.id);
    expect(serverAfter.server.installed).toBe(false);

    // Start should still show install dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await expect(installDialog).toBeVisible({ timeout: 5000 });

    // Dismiss
    await installDialog.locator("button:has-text('Cancel')").click();
  });
});

test.describe("Reset stale-state", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 13. Reset marks server uninstalled → Start shows dialog
  // ────────────────────────────────────────────────────────────────────────

  test("after server reset, Start shows install dialog again", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallSteps("reset-then-start");
    const server = await createServer(client, config);

    // Install via API
    await installServer(client, server.server.id);
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Verify Start works (no dialog, since we're installed)
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await page.waitForTimeout(500);
    await expect(installDialog).not.toBeVisible();

    // Wait for running
    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });

    // Stop the server
    await stopServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "stopped", 15000);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Reset the server via UI — use the page.on("dialog") to accept confirm
    page.on("dialog", (dialog) => dialog.accept());
    const resetBtn = page.locator("button:has-text('Reset')");
    // The reset button might be in a dropdown or further down the page
    // If not visible, check for the API-based reset
    const resetExists = await resetBtn.count();
    if (resetExists > 0) {
      await resetBtn.click();
      await page.waitForTimeout(2000);
    } else {
      // Reset via API and reload
      await resetServer(client, server.server.id);
      await page.reload({ waitUntil: "networkidle" });
    }

    // Verify reset worked
    await waitForInstalledState(client, server.server.id, false);

    // Reload page to pick up the reset state
    await page.reload({ waitUntil: "networkidle" });
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Now Start should show the install dialog
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();
    await expect(installDialog).toBeVisible({ timeout: 5000 });
    await expect(installDialog).toContainText("Server Not Installed");

    // Dismiss
    await installDialog.locator("button:has-text('Cancel')").click();
  });
});

test.describe("Update pipeline stale-state", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 14. Update pipeline completes → UI reflects updated state
  // ────────────────────────────────────────────────────────────────────────

  test("update pipeline completes without leaving stale installing state", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithAllPipelines("update-stale-state");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Install first
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Now run Update
    const updateBtn = page.locator("button:has-text('Update')");
    await expect(updateBtn).toBeVisible({ timeout: 5000 });
    await updateBtn.click();

    // The Update button now opens a version-selection dialog.
    // Confirm the update inside the dialog.
    const updateDialog = page.locator('[data-testid="update-dialog"]');
    await expect(updateDialog).toBeVisible({ timeout: 5000 });
    const confirmBtn = page.locator('[data-testid="update-dialog-confirm"]');
    await expect(confirmBtn).toBeVisible({ timeout: 5000 });
    await confirmBtn.click();

    // Wait for update to complete
    await waitForPhaseComplete(client, server.server.id, 30000);

    // After update, status should be stopped (not updating)
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // "Cancel Pipeline" should not be visible
    const cancelPipelineBtn = page.locator(
      "button:has-text('Cancel Pipeline')",
    );
    await expect(cancelPipelineBtn).not.toBeVisible({ timeout: 5000 });

    // Server should still be installed, and start should work
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await page.waitForTimeout(500);
    await expect(installDialog).not.toBeVisible();

    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });
  });
});

test.describe("Update dialog version selection", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 14b. Load versions button populates dropdown in update dialog
  // ────────────────────────────────────────────────────────────────────────

  test("load versions button populates dropdown after manual mode fallback", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    // Create a server with a version parameter that has options_from,
    // plus install and update steps so the Update button appears.
    const config: CreateServerRequest = {
      config: {
        name: "load-versions-test",
        binary: "/run/current-system/sw/bin/sleep",
        args: ["infinity"],
        env: {},
        working_dir: null,
        auto_start: false,
        auto_restart: false,
        max_restart_attempts: 5,
        restart_delay_secs: 5,
        stop_command: null,
        stop_signal: "sigterm" as const,
        stop_timeout_secs: 5,
        sftp_username: null,
        sftp_password: null,
        parameters: [
          {
            name: "game_version",
            label: "Game Version",
            description: null,
            param_type: "string" as const,
            default: "1.21.3",
            required: true,
            options: [],
            regex: null,
            is_version: true,
            options_from: {
              url: "https://mock.test/versions",
              path: "versions",
              value_key: null,
              label_key: null,
              sort: "desc" as const,
              limit: 10,
              cache_secs: 0,
            },
            github_repo: null,
          },
        ],
        stop_steps: [],
        start_steps: [],
        install_steps: [
          {
            name: "Install",
            description: null,
            action: {
              type: "run_command",
              command: "/run/current-system/sw/bin/echo",
              args: ["installed"],
              working_dir: null,
              env: {},
            },
            continue_on_error: false,
            condition: null,
          },
        ],
        update_steps: [
          {
            name: "Update",
            description: null,
            action: {
              type: "run_command",
              command: "/run/current-system/sw/bin/echo",
              args: ["updated"],
              working_dir: null,
              env: {},
            },
            continue_on_error: false,
            condition: null,
          },
        ],
        uninstall_steps: [],
        isolation: {
          enabled: true,
          extra_read_paths: [],
          extra_rw_paths: [],
          pids_max: null,
        },
        update_check: null,
        log_to_disk: false,
        max_log_size_mb: 50,
        enable_java_helper: false,
        enable_dotnet_helper: false,
        steam_app_id: null,
      },
      parameter_values: { game_version: "1.21.3" },
      source_template_id: null,
    };

    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Install first so the Update button is enabled
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // The initial auto-fetch inside the dialog will hit the mock URL via
    // the backend proxy (/api/templates/fetch-options?url=...).
    // We intercept the proxy call at the browser level so it never
    // reaches the backend, guaranteeing a controlled response.

    // Step 1: Make the FIRST fetch fail so the dialog falls back to
    // manual mode (text input + "Load versions" button).
    let fetchCount = 0;
    await page.route("**/api/templates/fetch-options**", (route) => {
      fetchCount++;
      if (fetchCount === 1) {
        // First call (auto-fetch on mount) → fail
        route.fulfill({
          status: 400,
          contentType: "application/json",
          body: JSON.stringify({ error: "Simulated failure" }),
        });
      } else {
        // Subsequent calls (manual "Load versions") → succeed
        route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            options: [
              { value: "1.21.4", label: "1.21.4" },
              { value: "1.21.3", label: "1.21.3" },
              { value: "1.20.6", label: "1.20.6" },
            ],
            cached: false,
          }),
        });
      }
    });

    // Click Update to open the dialog
    const updateBtn = page.locator("button:has-text('Update')");
    await expect(updateBtn).toBeVisible({ timeout: 5000 });
    await updateBtn.click();

    // Dialog should appear
    const updateDialog = page.locator('[data-testid="update-dialog"]');
    await expect(updateDialog).toBeVisible({ timeout: 5000 });

    // The auto-fetch failed, so we should be in manual mode:
    // a text input should be visible and no <select> dropdown.
    const textInput = updateDialog.locator('input[type="text"]');
    await expect(textInput).toBeVisible({ timeout: 5000 });

    // "Load versions" button should be visible (since options_from exists)
    const loadBtn = updateDialog.locator("button:has-text('Load versions')");
    await expect(loadBtn).toBeVisible({ timeout: 5000 });

    // Click "Load versions" — this time the mock returns success
    await loadBtn.click();

    // After a successful load, the SearchableSelect combobox should appear
    // and the text input should be gone (replaced by the dropdown).
    const dropdown = updateDialog.locator('[role="combobox"]');
    await expect(dropdown).toBeVisible({ timeout: 10000 });

    // The combobox being visible proves versions loaded successfully
    // (the component only renders it when versions().length > 0).
    // Verify it shows the current version as the selected value.
    await expect(dropdown).toContainText("1.21.3", { timeout: 5000 });

    // The text input should no longer be visible (replaced by dropdown)
    await expect(textInput).not.toBeVisible({ timeout: 5000 });

    // Clean up: close the dialog
    const cancelBtn = updateDialog.locator("button:has-text('Cancel')");
    await cancelBtn.click();
    await expect(updateDialog).not.toBeVisible({ timeout: 5000 });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 14c. Switching to manual mode and back via Load versions works
  // ────────────────────────────────────────────────────────────────────────

  test("manual mode pencil button then load versions restores dropdown", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    // Same server config as above
    const config: CreateServerRequest = {
      config: {
        name: "manual-mode-toggle-test",
        binary: "/run/current-system/sw/bin/sleep",
        args: ["infinity"],
        env: {},
        working_dir: null,
        auto_start: false,
        auto_restart: false,
        max_restart_attempts: 5,
        restart_delay_secs: 5,
        stop_command: null,
        stop_signal: "sigterm" as const,
        stop_timeout_secs: 5,
        sftp_username: null,
        sftp_password: null,
        parameters: [
          {
            name: "game_version",
            label: "Game Version",
            description: null,
            param_type: "string" as const,
            default: "1.21.3",
            required: true,
            options: [],
            regex: null,
            is_version: true,
            options_from: {
              url: "https://mock.test/versions",
              path: "versions",
              value_key: null,
              label_key: null,
              sort: "desc" as const,
              limit: 10,
              cache_secs: 0,
            },
            github_repo: null,
          },
        ],
        stop_steps: [],
        start_steps: [],
        install_steps: [
          {
            name: "Install",
            description: null,
            action: {
              type: "run_command",
              command: "/run/current-system/sw/bin/echo",
              args: ["installed"],
              working_dir: null,
              env: {},
            },
            continue_on_error: false,
            condition: null,
          },
        ],
        update_steps: [
          {
            name: "Update",
            description: null,
            action: {
              type: "run_command",
              command: "/run/current-system/sw/bin/echo",
              args: ["updated"],
              working_dir: null,
              env: {},
            },
            continue_on_error: false,
            condition: null,
          },
        ],
        uninstall_steps: [],
        isolation: {
          enabled: true,
          extra_read_paths: [],
          extra_rw_paths: [],
          pids_max: null,
        },
        update_check: null,
        log_to_disk: false,
        max_log_size_mb: 50,
        enable_java_helper: false,
        enable_dotnet_helper: false,
        steam_app_id: null,
      },
      parameter_values: { game_version: "1.21.3" },
      source_template_id: null,
    };

    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Install first
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // ALL fetch-options calls succeed this time
    await page.route("**/api/templates/fetch-options**", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          options: [
            { value: "1.21.4", label: "1.21.4" },
            { value: "1.21.3", label: "1.21.3" },
            { value: "1.20.6", label: "1.20.6" },
          ],
          cached: false,
        }),
      });
    });

    // Open update dialog
    const updateBtn = page.locator("button:has-text('Update')");
    await expect(updateBtn).toBeVisible({ timeout: 5000 });
    await updateBtn.click();

    const updateDialog = page.locator('[data-testid="update-dialog"]');
    await expect(updateDialog).toBeVisible({ timeout: 5000 });

    // Auto-fetch succeeded → SearchableSelect combobox should be visible
    const dropdown = updateDialog.locator('[role="combobox"]');
    await expect(dropdown).toBeVisible({ timeout: 10000 });

    // Click ✏️ button to switch to manual mode
    const pencilBtn = updateDialog.locator(
      'button[title="Enter version manually"]',
    );
    await expect(pencilBtn).toBeVisible({ timeout: 5000 });
    await pencilBtn.click();

    // Now a text input should be visible instead of the dropdown
    const textInput = updateDialog.locator('input[type="text"]');
    await expect(textInput).toBeVisible({ timeout: 5000 });
    await expect(dropdown).not.toBeVisible({ timeout: 5000 });

    // Click "Load versions" to go back to dropdown mode
    const loadBtn = updateDialog.locator("button:has-text('Load versions')");
    await expect(loadBtn).toBeVisible({ timeout: 5000 });
    await loadBtn.click();

    // Dropdown should reappear
    await expect(dropdown).toBeVisible({ timeout: 10000 });
    await expect(textInput).not.toBeVisible({ timeout: 5000 });

    // Clean up
    const cancelBtn = updateDialog.locator("button:has-text('Cancel')");
    await cancelBtn.click();
    await expect(updateDialog).not.toBeVisible({ timeout: 5000 });
  });
});

test.describe("Multiple rapid pipeline cycles", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 15. Two back-to-back install → uninstall cycles keep state correct
  // ────────────────────────────────────────────────────────────────────────

  test("two install/uninstall cycles leave consistent state", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall("rapid-cycles");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    const installDialog = page.locator('[data-testid="install-dialog"]');
    const uninstallBtn = page.locator("button:has-text('Uninstall')");

    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    for (let cycle = 1; cycle <= 2; cycle++) {
      // Install
      const installBtn = page.locator("button:has-text('Install')").first();
      await expect(installBtn).toBeVisible({ timeout: 10000 });
      await installBtn.click();
      await waitForPhaseComplete(client, server.server.id, 30000);
      await waitForInstalledState(client, server.server.id, true);
      await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

      // Verify installed state in UI
      await expect(
        page.locator("button:has-text('Reinstall')").first(),
      ).toBeVisible({ timeout: 10000 });
      await expect(uninstallBtn).toBeVisible({ timeout: 5000 });

      // Start should NOT show dialog
      await expect(startBtn).toBeEnabled({ timeout: 5000 });
      await startBtn.click();
      await page.waitForTimeout(500);
      await expect(installDialog).not.toBeVisible();
      await expect(statusBadge).toContainText(/running|starting/i, {
        timeout: 15000,
      });

      // Stop
      await stopServer(client, server.server.id);
      await waitForStatus(client, server.server.id, "stopped", 15000);
      await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

      // Uninstall
      await expect(uninstallBtn).toBeVisible({ timeout: 10000 });
      await uninstallBtn.click();
      await waitForPhaseComplete(client, server.server.id, 30000);
      await waitForInstalledState(client, server.server.id, false);
      await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

      // Verify uninstalled in UI
      await expect(uninstallBtn).not.toBeVisible({ timeout: 10000 });

      // Start should show dialog
      await expect(startBtn).toBeEnabled({ timeout: 5000 });
      await startBtn.click();
      await expect(installDialog).toBeVisible({ timeout: 5000 });
      await installDialog.locator("button:has-text('Cancel')").click();
      await expect(installDialog).not.toBeVisible({ timeout: 3000 });
    }
  });

  // ────────────────────────────────────────────────────────────────────────
  // 16. Install via dialog, then immediately install again (Reinstall)
  // ────────────────────────────────────────────────────────────────────────

  test("Start → Install dialog → Install → Reinstall → Start all work", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallSteps("dialog-reinstall");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    const installDialog = page.locator('[data-testid="install-dialog"]');

    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // First: Start → Install dialog → Install
    await startBtn.click();
    await expect(installDialog).toBeVisible({ timeout: 5000 });
    const installOption = installDialog.locator(
      "button.btn-success:has-text('Install')",
    );
    await installOption.click();
    await expect(installDialog).not.toBeVisible({ timeout: 5000 });

    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);

    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Now click "Reinstall" to re-run the install pipeline
    const reinstallBtn = page.locator("button:has-text('Reinstall')").first();
    await expect(reinstallBtn).toBeVisible({ timeout: 10000 });
    await reinstallBtn.click();

    await waitForPhaseComplete(client, server.server.id, 30000);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // After reinstall, server is still installed
    const serverAfter = await getServer(client, server.server.id);
    expect(serverAfter.server.installed).toBe(true);

    // Start should work without dialog
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();
    await page.waitForTimeout(500);
    await expect(installDialog).not.toBeVisible();
    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });
  });
});

test.describe("Page refresh preserves correct state after pipelines", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  // ────────────────────────────────────────────────────────────────────────
  // 17. Refresh after install — state is correct
  // ────────────────────────────────────────────────────────────────────────

  test("page refresh after install reflects installed state correctly", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall("refresh-after-install");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Install
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Refresh the page
    await page.reload({ waitUntil: "networkidle" });

    // After refresh: button should say "Reinstall", Uninstall visible
    await expect(
      page.locator("button:has-text('Reinstall')").first(),
    ).toBeVisible({ timeout: 10000 });
    await expect(page.locator("button:has-text('Uninstall')")).toBeVisible({
      timeout: 5000,
    });

    // Start should work without dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await page.waitForTimeout(500);
    await expect(installDialog).not.toBeVisible();
    await expect(statusBadge).toContainText(/running|starting/i, {
      timeout: 15000,
    });
  });

  // ────────────────────────────────────────────────────────────────────────
  // 18. Refresh after uninstall — state is correct
  // ────────────────────────────────────────────────────────────────────────

  test("page refresh after uninstall reflects uninstalled state correctly", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await enableRunCommands(client, true);

    const config = createServerWithInstallAndUninstall(
      "refresh-after-uninstall",
    );
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Install then uninstall
    const installBtn = page.locator("button:has-text('Install')").first();
    await installBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, true);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    const uninstallBtn = page.locator("button:has-text('Uninstall')");
    await expect(uninstallBtn).toBeVisible({ timeout: 10000 });
    await uninstallBtn.click();
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalledState(client, server.server.id, false);
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 15000 });

    // Refresh the page
    await page.reload({ waitUntil: "networkidle" });

    // After refresh: button says "Install", no Uninstall button
    const freshInstallBtn = page
      .locator("button:has-text('Install'):not(:has-text('Reinstall'))")
      .first();
    await expect(freshInstallBtn).toBeVisible({ timeout: 10000 });
    await expect(page.locator("button:has-text('Uninstall')")).not.toBeVisible({
      timeout: 3000,
    });

    // Start should show dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const installDialog = page.locator('[data-testid="install-dialog"]');
    await expect(installDialog).toBeVisible({ timeout: 5000 });
    await expect(installDialog).toContainText("Server Not Installed");

    // Dismiss
    await installDialog.locator("button:has-text('Cancel')").click();
  });
});
