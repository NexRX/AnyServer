/**
 * Install → Start Lifecycle & Button State Tests
 *
 * Tests for two reported bugs:
 *
 * Bug 1: After creating and installing a server, clicking Start shows
 *         "crashed" status even though the server is running fine.
 *         Refreshing the page fixes it — indicating stale frontend state.
 *
 * Bug 2: Start and Restart buttons are enabled even before a server is
 *         installed, which doesn't make sense UX-wise.
 *
 * These tests verify:
 * - Correct status transitions during install → start flow
 * - No false "crashed" status after install then start
 * - Button disabled/enabled states respect installation status
 * - Restart button is only enabled when server is running
 * - Install dialog appears when starting an uninstalled server
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  startServer,
  stopServer,
  waitForStatus,
  cleanupAllServers,
  getServer,
  installServer,
  waitForPhaseComplete,
  markServerInstalled,
  createNonAdminUser,
  grantServerPermission,
  enableRunCommands,
} from "../helpers/api";
import type { CreateServerRequest } from "../../src/types/bindings";

/**
 * Helper: create a server config WITH install steps.
 * The install step is a simple echo command that completes immediately.
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
 * Helper: wait for a server to have installed=true in the API.
 * Covers the brief race window where phase status is "completed"
 * but the DB hasn't been updated yet.
 */
async function waitForInstalled(
  client: ReturnType<typeof createApiClient>,
  serverId: string,
  timeout = 10000,
  interval = 200,
): Promise<void> {
  const startTime = Date.now();
  while (Date.now() - startTime < timeout) {
    const server = await getServer(client, serverId);
    if (server.server.installed) return;
    await new Promise((resolve) => setTimeout(resolve, interval));
  }
  throw new Error(
    `Server ${serverId} did not become installed within ${timeout}ms`,
  );
}

test.describe("Install → Start Lifecycle", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("status shows running (not crashed) after install then start", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands for install pipeline
    await enableRunCommands(client, true);

    // Create a server with install steps
    const config = createServerWithInstallSteps("test-install-start");
    const server = await createServer(client, config);

    // Log in and navigate to the server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Verify server is initially stopped and NOT installed
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 5000 });

    // Install via API, then wait for completion
    await installServer(client, server.server.id);
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalled(client, server.server.id);

    // Verify server is now installed via API
    const serverAfterInstall = await getServer(client, server.server.id);
    expect(serverAfterInstall.server.installed).toBe(true);

    // Give UI a moment to receive the phase completion via WebSocket
    await page.waitForTimeout(500);

    // Start the server via API
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Verify the UI shows "running" status (NOT "crashed")
    await expect(statusBadge).toContainText(/running/i, { timeout: 10000 });

    // Double-check: the status should NOT show crashed at this point
    const statusText = await statusBadge.textContent();
    expect(statusText?.toLowerCase()).not.toContain("crashed");
  });

  test("status shows running after install then start via UI buttons", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands for install pipeline
    await enableRunCommands(client, true);

    // Create a server with install steps
    const config = createServerWithInstallSteps("test-ui-install-start");
    const server = await createServer(client, config);

    // Log in and navigate
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Install button
    const installBtn = page.locator("button:has-text('Install')").first();
    await expect(installBtn).toBeVisible({ timeout: 5000 });
    await installBtn.click();

    // Wait for install pipeline to complete via API
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalled(client, server.server.id);

    // Verify installed via API
    const serverAfterInstall = await getServer(client, server.server.id);
    expect(serverAfterInstall.server.installed).toBe(true);

    // The status should still be stopped after install
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });

    // Now click Start
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    // Wait for running status via API
    await waitForStatus(client, server.server.id, "running", 15000);

    // Verify the UI shows running — not crashed
    await expect(statusBadge).toContainText(/running/i, { timeout: 10000 });

    // Ensure we never see "crashed" flash in the badge
    const finalStatus = await statusBadge.textContent();
    expect(finalStatus?.toLowerCase()).not.toContain("crashed");
  });

  test("no crashed status flash during install-then-start transition", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands for install pipeline
    await enableRunCommands(client, true);

    // Create server with install steps
    const config = createServerWithInstallSteps("test-no-crash-flash");
    const server = await createServer(client, config);

    // Log in and navigate
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Track all status badge text changes to detect any flash of "crashed"
    const observedStatuses: string[] = [];
    const statusBadge = page.locator(".status-badge").first();

    // Set up a mutation observer to watch for status changes
    await page.evaluate(() => {
      (window as any).__observedStatuses = [];
      const badge = document.querySelector(".status-badge");
      if (badge) {
        const observer = new MutationObserver(() => {
          const text = badge.textContent?.trim().toLowerCase() ?? "";
          (window as any).__observedStatuses.push(text);
        });
        observer.observe(badge, {
          childList: true,
          characterData: true,
          subtree: true,
        });
      }
    });

    // Install via API
    await installServer(client, server.server.id);
    await waitForPhaseComplete(client, server.server.id, 30000);

    // Start via API
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Wait for UI to settle
    await page.waitForTimeout(2000);

    // Collect observed statuses
    const allStatuses: string[] = await page.evaluate(
      () => (window as any).__observedStatuses ?? [],
    );

    // "crashed" should NEVER appear in the observed statuses
    const crashedOccurrences = allStatuses.filter((s) => s.includes("crashed"));
    expect(crashedOccurrences).toHaveLength(0);

    // Final status should be running
    await expect(statusBadge).toContainText(/running/i, { timeout: 5000 });
  });

  test("status is correct after page refresh following install-then-start", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands for install pipeline
    await enableRunCommands(client, true);

    // Create, install, and start a server
    const config = createServerWithInstallSteps("test-refresh-status");
    const server = await createServer(client, config);
    await installServer(client, server.server.id);
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalled(client, server.server.id);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Navigate to the page (simulates refresh)
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Status should show running immediately — no stale state
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 10000 });
  });
});

test.describe("Button State Management - Installation", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("restart button is disabled when server is stopped", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server (no install steps, minimal config)
    const config = createMinimalServerConfig("test-restart-disabled");
    const server = await createServer(client, config);

    // Navigate to server detail
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Verify server is stopped
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 5000 });

    // Restart button should be disabled when server is not running
    const restartBtn = page.locator("button.btn-warning:has-text('Restart')");
    await expect(restartBtn).toBeDisabled({ timeout: 5000 });
  });

  test("restart button is enabled when server is running", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and start a server
    const config = createMinimalServerConfig("test-restart-enabled");
    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Navigate to server detail
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Verify running
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 5000 });

    // Restart button should be enabled when running
    const restartBtn = page.locator("button.btn-warning:has-text('Restart')");
    await expect(restartBtn).toBeEnabled({ timeout: 5000 });
  });

  test("start button shows install dialog for uninstalled server with install steps", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands so install option works
    await enableRunCommands(client, true);

    // Create a server with install steps but don't install it
    const config = createServerWithInstallSteps("test-install-dialog");
    const server = await createServer(client, config);

    // Navigate to server detail
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Start
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    // An install dialog should appear with Install, Mark as Installed, Cancel options
    const dialog = page.locator(
      "[data-testid='install-dialog'], .install-dialog",
    );
    await expect(dialog).toBeVisible({ timeout: 5000 });

    // Verify Install option exists (use .btn-success to distinguish from "Mark as Installed")
    const installOption = dialog.locator(
      "button.btn-success:has-text('Install')",
    );
    await expect(installOption).toBeVisible();

    // Verify Mark as Installed option exists (admin should see this)
    const markInstalledOption = dialog.locator(
      "button:has-text('Mark as Installed')",
    );
    await expect(markInstalledOption).toBeVisible();

    // Verify Cancel option exists
    const cancelOption = dialog.locator("button:has-text('Cancel')");
    await expect(cancelOption).toBeVisible();

    // Click Cancel to dismiss
    await cancelOption.click();
    await expect(dialog).not.toBeVisible({ timeout: 3000 });
  });

  test("install dialog Cancel does not start the server", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands
    await enableRunCommands(client, true);

    // Create server with install steps, uninstalled
    const config = createServerWithInstallSteps("test-cancel-no-start");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Start -> dialog appears
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await startBtn.click();

    const dialog = page.locator(
      "[data-testid='install-dialog'], .install-dialog",
    );
    await expect(dialog).toBeVisible({ timeout: 5000 });

    // Click Cancel
    const cancelBtn = dialog.locator("button:has-text('Cancel')");
    await cancelBtn.click();

    // Server should still be stopped
    const serverStatus = await getServer(client, server.server.id);
    expect(serverStatus.runtime.status).toBe("stopped");

    // Badge should show stopped
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 5000 });
  });

  test("install dialog Install option triggers install pipeline", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands for install pipeline
    await enableRunCommands(client, true);

    // Create server with install steps
    const config = createServerWithInstallSteps("test-dialog-install");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Start -> dialog appears
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await startBtn.click();

    const dialog = page.locator(
      "[data-testid='install-dialog'], .install-dialog",
    );
    await expect(dialog).toBeVisible({ timeout: 5000 });

    // Click the Install button inside the dialog (not the page-level Install button)
    const installOption = dialog.locator(
      "button.btn-success:has-text('Install')",
    );
    await installOption.click();

    // Dialog should close
    await expect(dialog).not.toBeVisible({ timeout: 5000 });

    // Wait for the install pipeline to complete
    await waitForPhaseComplete(client, server.server.id, 30000);

    // Server should now be installed
    const serverAfterInstall = await getServer(client, server.server.id);
    expect(serverAfterInstall.server.installed).toBe(true);
  });

  test("install dialog Mark as Installed option marks server installed without pipeline", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands
    await enableRunCommands(client, true);

    // Create server with install steps
    const config = createServerWithInstallSteps("test-mark-installed");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Start -> dialog
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await startBtn.click();

    const dialog = page.locator(
      "[data-testid='install-dialog'], .install-dialog",
    );
    await expect(dialog).toBeVisible({ timeout: 5000 });

    // Click "Mark as Installed"
    const markBtn = dialog.locator("button:has-text('Mark as Installed')");
    await markBtn.click();

    // Dialog should close
    await expect(dialog).not.toBeVisible({ timeout: 5000 });

    // Server should be marked installed via API (no pipeline ran)
    // Give it a moment
    await page.waitForTimeout(1000);
    const serverAfter = await getServer(client, server.server.id);
    expect(serverAfter.server.installed).toBe(true);
  });

  test("Mark as Installed option is NOT shown to non-admin users", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands
    await enableRunCommands(client, true);

    // Create a server with install steps
    const config = createServerWithInstallSteps("test-no-mark-for-operator");
    const server = await createServer(client, config);

    // Create a non-admin user and grant them operator permission
    const user = await createNonAdminUser(
      client,
      "operator_user",
      "Operator123",
    );
    const operatorClient = createApiClient(testEnv.apiUrl, user.token);
    await grantServerPermission(
      client,
      server.server.id,
      user.userId,
      "operator",
    );

    // Log in as operator
    await loginViaUI(page, testEnv.baseUrl, "operator_user", "Operator123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Start -> dialog appears
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    const dialog = page.locator(
      "[data-testid='install-dialog'], .install-dialog",
    );
    await expect(dialog).toBeVisible({ timeout: 5000 });

    // "Mark as Installed" should NOT be visible for non-admin
    const markBtn = dialog.locator("button:has-text('Mark as Installed')");
    await expect(markBtn).not.toBeVisible({ timeout: 3000 });

    // But Install and Cancel should still be visible
    const installBtn = dialog.locator("button:has-text('Install')").first();
    await expect(installBtn).toBeVisible();
    const cancelBtn = dialog.locator("button:has-text('Cancel')");
    await expect(cancelBtn).toBeVisible();

    // Clean up
    await cancelBtn.click();
  });

  test("start button works normally for server without install steps", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server WITHOUT install steps
    const config = createMinimalServerConfig("test-no-install-steps");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Start — should NOT show install dialog (no install steps)
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await startBtn.click();

    // No dialog should appear
    const dialog = page.locator(
      "[data-testid='install-dialog'], .install-dialog",
    );
    await expect(dialog).not.toBeVisible({ timeout: 2000 });

    // Server should start directly
    await waitForStatus(client, server.server.id, "running", 15000);
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 10000 });
  });

  test("start button works normally for already-installed server", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands for install pipeline
    await enableRunCommands(client, true);

    // Create server with install steps, install it
    const config = createServerWithInstallSteps("test-already-installed");
    const server = await createServer(client, config);
    await installServer(client, server.server.id);
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalled(client, server.server.id);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click Start — should NOT show install dialog (already installed)
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    // No dialog
    const dialog = page.locator(
      "[data-testid='install-dialog'], .install-dialog",
    );
    await expect(dialog).not.toBeVisible({ timeout: 2000 });

    // Server should start directly
    await waitForStatus(client, server.server.id, "running", 15000);
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 10000 });
  });

  test("restart button disabled for uninstalled server with install steps", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands
    await enableRunCommands(client, true);

    // Create server with install steps but don't install
    const config = createServerWithInstallSteps("test-restart-uninstalled");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Restart should be disabled (server is stopped)
    const restartBtn = page.locator("button.btn-warning:has-text('Restart')");
    await expect(restartBtn).toBeDisabled({ timeout: 5000 });
  });

  test("button states transition correctly through full lifecycle", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands for install pipeline
    await enableRunCommands(client, true);

    // Create server with install steps
    const config = createServerWithInstallSteps("test-full-lifecycle-btns");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const startBtn = page.locator("button.btn-success:has-text('Start')");
    const restartBtn = page.locator("button.btn-warning:has-text('Restart')");
    const stopBtn = page.locator("button.btn-danger:has-text('Stop')");

    // Phase 1: Uninstalled, stopped
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await expect(restartBtn).toBeDisabled();
    await expect(stopBtn).toBeDisabled();

    // Install the server
    await installServer(client, server.server.id);
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalled(client, server.server.id);

    // Phase 2: Installed, stopped — need to refresh to pick up installed state
    await page.reload();
    await page.waitForLoadState("networkidle");

    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await expect(restartBtn).toBeDisabled();
    await expect(stopBtn).toBeDisabled();

    // Start the server
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Phase 3: Running
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 10000 });
    await expect(startBtn).toBeDisabled({ timeout: 5000 });
    await expect(restartBtn).toBeEnabled();
    await expect(stopBtn).toBeEnabled();

    // Stop the server
    await stopServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "stopped", 15000);

    // Phase 4: Stopped again
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await expect(restartBtn).toBeDisabled();
    await expect(stopBtn).toBeDisabled();
  });
});
