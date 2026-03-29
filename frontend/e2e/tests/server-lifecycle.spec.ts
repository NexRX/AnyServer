/**
 * Server Lifecycle Test Suite
 *
 * Tests server lifecycle operations including:
 * - Starting servers
 * - Stopping servers
 * - Restarting servers
 * - Killing servers
 * - Kill during stopping state
 * - Button state management
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  startServer,
  stopServer,
  restartServer,
  killServer,
  waitForStatus,
  ensureStopped,
  cleanupAllServers,
  getServer,
} from "../helpers/api";

test.describe("Server Lifecycle", () => {
  test.afterEach(async ({ testEnv }) => {
    // Clean up servers after each test
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("start transitions status to running", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-start-server");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click start button
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeVisible({ timeout: 5000 });
    await expect(startBtn).toBeEnabled();
    await startBtn.click();

    // Wait for status to change to running
    await waitForStatus(client, server.server.id, "running", 15000);

    // Verify UI shows running status
    const statusBadge = page
      .locator(".status-badge, .server-status, [data-status]")
      .first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 5000 });
  });

  test("stop transitions status to stopped", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and start a server
    const config = createMinimalServerConfig("test-stop-server");
    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click stop button
    const stopBtn = page.locator("button.btn-danger:has-text('Stop')");
    await expect(stopBtn).toBeVisible({ timeout: 5000 });
    await expect(stopBtn).toBeEnabled();
    await stopBtn.click();

    // Wait for status to change to stopped
    await waitForStatus(client, server.server.id, "stopped", 15000);

    // Verify UI shows stopped status
    const statusBadge = page
      .locator(".status-badge, .server-status, [data-status]")
      .first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 5000 });
  });

  test("restart cycles through to running", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and start a server
    const config = createMinimalServerConfig("test-restart-server");
    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click restart button
    const restartBtn = page.locator("button.btn-warning:has-text('Restart')");
    await expect(restartBtn).toBeVisible({ timeout: 5000 });
    await expect(restartBtn).toBeEnabled();
    await restartBtn.click();

    // Should go through stopping -> starting -> running
    // Wait for final running status
    await waitForStatus(client, server.server.id, "running", 20000);

    // Verify UI shows running status
    const statusBadge = page
      .locator(".status-badge, .server-status, [data-status]")
      .first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 5000 });
  });

  test("kill immediately terminates the process", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and start a server
    const config = createMinimalServerConfig("test-kill-server");
    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click kill button - use more specific selector to avoid confusion with Stop button
    // The Kill button has "⚡ Kill" text and a danger class with the title attribute
    const killBtn = page.locator('button.btn-danger[title*="SIGKILL"]');
    await expect(killBtn).toBeVisible({ timeout: 5000 });
    await expect(killBtn).toBeEnabled();

    // Accept the confirmation dialog that appears when clicking Kill
    page.once("dialog", (dialog) => dialog.accept());
    await killBtn.click();

    // Wait for status to change to stopped (kill should be immediate)
    await waitForStatus(client, server.server.id, "stopped", 10000);

    // Verify UI shows stopped status
    const statusBadge = page
      .locator(".status-badge, .server-status, [data-status]")
      .first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 5000 });
  });

  test("kill is available during stopping state", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server with a shell script that traps SIGTERM and delays shutdown
    // This ensures the server stays in "stopping" state long enough to test Kill
    const config = createMinimalServerConfig("test-kill-during-stop");
    config.config.binary = "/bin/sh";
    config.config.args = ["-c", "trap 'sleep 10' TERM; sleep infinity & wait"];
    config.config.stop_timeout_secs = 30; // Long timeout
    const server = await createServer(client, config);

    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click stop button
    const stopBtn = page.locator("button.btn-danger:has-text('Stop')");
    await stopBtn.click();

    // Wait for server to enter stopping state via API
    const maxAttempts = 50;
    let inStoppingState = false;
    for (let i = 0; i < maxAttempts; i++) {
      const status = await getServer(client, server.server.id);
      if (status.runtime.status === "stopping") {
        inStoppingState = true;
        break;
      }
      await page.waitForTimeout(100);
    }

    if (!inStoppingState) {
      throw new Error("Server did not enter stopping state within timeout");
    }

    // Kill button should now be available - use specific selector with title attribute
    const killBtn = page.locator('button.btn-danger[title*="SIGKILL"]');
    await expect(killBtn).toBeVisible({ timeout: 5000 });
    await expect(killBtn).toBeEnabled();

    // Accept the confirmation dialog that appears when clicking Kill
    page.once("dialog", (dialog) => dialog.accept());
    // Click kill to force stop
    await killBtn.click();

    // Should transition to stopped quickly
    await waitForStatus(client, server.server.id, "stopped", 10000);

    // Verify UI shows stopped status
    const statusBadge = page
      .locator(".status-badge, .server-status, [data-status]")
      .first();
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 5000 });
  });

  test("start button disabled while server is running", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and start a server
    const config = createMinimalServerConfig("test-button-states");
    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Verify status is running
    // Verify UI shows running status
    const statusBadge = page
      .locator(".status-badge, .server-status, [data-status]")
      .first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 5000 });

    // Start button should be disabled
    const startBtn = page.locator("button.btn-success:has-text('Start')");
    await expect(startBtn).toBeDisabled({ timeout: 5000 });

    // Stop button should be enabled
    const stopBtn = page.locator("button.btn-danger:has-text('Stop')");
    await expect(stopBtn).toBeEnabled();
  });

  test("button states update correctly during transitions", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-button-transitions");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const startBtn = page.locator("button.btn-success:has-text('Start')");
    const stopBtn = page.locator("button.btn-danger:has-text('Stop')");

    // Initially stopped: start enabled, stop disabled
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await expect(stopBtn).toBeDisabled();

    // Start the server
    await startBtn.click();
    await waitForStatus(client, server.server.id, "running", 15000);

    // After starting: start disabled, stop enabled
    await expect(startBtn).toBeDisabled({ timeout: 5000 });
    await expect(stopBtn).toBeEnabled();

    // Stop the server
    await stopBtn.click();
    await waitForStatus(client, server.server.id, "stopped", 15000);

    // After stopping: back to initial state
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await expect(stopBtn).toBeDisabled();
  });

  test("restart button available when server is running", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and start a server
    const config = createMinimalServerConfig("test-restart-availability");
    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Restart button should be enabled when running
    const restartBtn = page.locator("button.btn-warning:has-text('Restart')");
    await expect(restartBtn).toBeVisible({ timeout: 5000 });
    await expect(restartBtn).toBeEnabled();
  });
});
