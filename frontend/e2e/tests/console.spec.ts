/**
 * Console Test Suite
 *
 * Tests WebSocket console functionality including:
 * - Real-time server output streaming
 * - Sending commands via stdin
 * - Connection status indicators
 * - Console output persistence across tab switches
 * - WebSocket reconnection handling
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createVerboseServerConfig,
  createMinimalServerConfig,
  startServer,
  waitForStatus,
  sendCommand,
  cleanupAllServers,
} from "../helpers/api";
import { connectToConsole } from "../helpers/websocket";

test.describe("Console", () => {
  test.afterEach(async ({ testEnv }) => {
    // Clean up servers after each test
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("console shows server output in real time", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server that outputs messages
    const config = createVerboseServerConfig("test-console-output");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Wait a moment for auth to fully settle
    await page.waitForTimeout(500);

    // Navigate to server detail page
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Verify we're on the right page by checking for server name
    await expect(page.locator("h1, h2, .server-name")).toContainText(
      "test-console-output",
      { timeout: 5000 },
    );

    // Wait for the Start button to be visible before clicking
    const startBtn = page
      .locator("button.btn-success")
      .filter({ hasText: /^▶ Start$/ });
    await expect(startBtn).toBeVisible({ timeout: 10000 });
    await startBtn.click();

    // Wait for server to reach running state
    await waitForStatus(client, server.server.id, "running", 15000);

    // Wait for console output container to appear
    const consoleOutput = page.locator(".console-output");
    await expect(consoleOutput).toBeVisible({ timeout: 15000 });

    // Wait for actual content to appear in the console (not just empty state)
    await page.waitForFunction(
      () => {
        const output = document.querySelector(".console-output");
        const text = output?.textContent || "";
        // Check if there's actual log content (not just empty state messages)
        return text.length > 50 && !text.includes("No output yet");
      },
      { timeout: 15000 },
    );

    // Get the console output text and verify it contains server output
    // The "Server starting..." message may not always appear due to timing,
    // but we should see "Tick:" messages from the running process
    const outputText = await consoleOutput.textContent();
    expect(outputText).toMatch(/Tick:|Server starting/);
  });

  test("sending a command produces echoed output", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server that can receive commands (echo server)
    const config = {
      config: {
        name: "test-echo-server",
        binary: "/run/current-system/sw/bin/cat",
        args: [],
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
      parameter_values: {},
      source_template_id: null,
    };

    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Find command input (inside .console-input div)
    const commandInput = page.locator(".console-input input");

    await expect(commandInput).toBeVisible({ timeout: 5000 });

    // Send a test command by typing and pressing Enter
    await commandInput.fill("test command");
    await commandInput.press("Enter");

    // Wait a moment for the output to appear
    await page.waitForTimeout(1000);

    // Check that the command appears in the output
    const outputText = await page.locator(".console-output").textContent();
    expect(outputText).toContain("test command");
  });

  test("console shows connection status indicator", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-console-status");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Check for connection status indicator (in .console-status)
    const statusIndicator = page.locator(".console-status");

    // Should show connected status
    await expect(statusIndicator).toBeVisible({ timeout: 5000 });

    // The indicator should show "Connected" or "Connecting"
    const statusText = await statusIndicator.textContent();
    expect(statusText?.toLowerCase()).toMatch(/connect/);
  });

  test("console preserves output across tab switches", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server that outputs messages
    const config = createVerboseServerConfig("test-console-persistence");
    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Wait for console output to appear
    await page.waitForTimeout(2000);

    // Get initial console content
    const consoleContainer = page.locator(".console-output");
    const initialContent = await consoleContainer.textContent();
    expect(initialContent).toBeTruthy();
    expect(initialContent!.length).toBeGreaterThan(0);

    // Switch to another tab (e.g., Files or Config)
    const filesTab = page.locator("button.tab:has-text('Files')");
    if (await filesTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await filesTab.click();
      await page.waitForTimeout(500);
    }

    // Switch back to Console tab
    const consoleTab = page.locator("button.tab:has-text('Console')");
    await consoleTab.click();
    await page.waitForTimeout(500);

    // Console content should still be there
    const afterSwitchContent = await consoleContainer.textContent();
    expect(afterSwitchContent).toBeTruthy();
    // Should contain at least the initial content (may have more from new messages)
    expect(afterSwitchContent!.length).toBeGreaterThanOrEqual(
      initialContent!.length * 0.8,
    ); // Allow some variation
  });

  test("console handles rapid log output without freezing", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server that outputs rapidly
    const config = {
      config: {
        name: "test-rapid-output",
        binary: "/run/current-system/sw/bin/sh",
        args: [
          "-c",
          'for i in $(seq 1 100); do echo "Log line $i"; done; sleep infinity',
        ],
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
      parameter_values: {},
      source_template_id: null,
    };

    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Wait for logs to appear
    await page.waitForTimeout(2000);

    // Page should still be responsive
    const consoleContainer = page.locator(".console-output");
    await expect(consoleContainer).toBeVisible();

    // Should have received many log lines
    const content = await consoleContainer.textContent();
    expect(content).toContain("Log line");

    // UI should still be interactive - check that any control button is visible
    const stopBtn = page.locator("button:has-text('Stop')");
    await expect(stopBtn).toBeVisible();
  });

  test("WebSocket connection reconnects after disconnect", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-ws-reconnect");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Check initial connection
    const statusIndicator = page.locator(
      ".ws-status, .connection-status, .console-status, [data-ws-status]",
    );
    await expect(statusIndicator).toBeVisible({ timeout: 5000 });

    // Simulate WebSocket disconnect by evaluating in browser context
    await page.evaluate(() => {
      // Find and close any open WebSocket connections
      const wsConnections = (window as any).__wsConnections || [];
      wsConnections.forEach((ws: WebSocket) => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.close();
        }
      });
    });

    // Wait a bit for reconnection attempt
    await page.waitForTimeout(2000);

    // Should reconnect and show connected status again
    const reconnectedText = await statusIndicator.textContent();
    expect(reconnectedText?.toLowerCase()).toMatch(/connect|online|ready/);
  });

  test("console shows different log streams (stdout/stderr) if supported", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server that outputs to both stdout and stderr
    // Get server that produces both stdout and stderr
    const config = {
      config: {
        name: "test-dual-stream",
        binary: "/run/current-system/sw/bin/sh",
        args: [
          "-c",
          'echo "stdout message"; echo "stderr message" >&2; sleep infinity',
        ],
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
      parameter_values: {},
      source_template_id: null,
    };

    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Wait for logs to appear
    await page.waitForTimeout(2000);

    // Check that both messages appear
    const consoleContainer = page.locator(".console-output");
    const content = await consoleContainer.textContent();

    expect(content).toContain("stdout message");
    expect(content).toContain("stderr message");
  });

  test("console reflects new output after backend reconnect", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Use a typed base config so it stays aligned with CreateServerRequest.
    const config = createMinimalServerConfig("test-console-reconnect-output");
    // Switch to a stdin-echo process so assertions are deterministic.
    config.config.binary = "/run/current-system/sw/bin/cat";
    config.config.args = [];

    const server = await createServer(client, config);
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    const statusIndicator = page.locator(".console-status");
    const consoleOutput = page.locator(".console-output");

    await expect(statusIndicator).toContainText(/connected/i, {
      timeout: 10000,
    });
    await expect(consoleOutput).toBeVisible({ timeout: 10000 });

    // Prove the baseline path works before reconnect.
    const beforeMarker = `before-reconnect-${Date.now()}`;
    await sendCommand(client, server.server.id, beforeMarker);
    await expect(consoleOutput).toContainText(beforeMarker, { timeout: 10000 });

    // Simulate backend outage and recovery.
    await testEnv.killBackend();
    await expect(statusIndicator).toContainText(
      /connecting|reconnecting|disconnected/i,
      { timeout: 15000 },
    );

    await testEnv.restartBackend();

    // Ensure server is running again (it may need to be restarted after backend restart).
    await startServer(client, server.server.id).catch(() => {});
    await waitForStatus(client, server.server.id, "running", 15000);

    await expect(statusIndicator).toContainText(/connected/i, {
      timeout: 30000,
    });

    // Regression check: action is accepted by backend but must also appear in console.
    const afterMarker = `after-reconnect-${Date.now()}`;
    await sendCommand(client, server.server.id, afterMarker);

    await expect(
      consoleOutput,
      "Command after reconnect was accepted by backend but not rendered in console output",
    ).toContainText(afterMarker, { timeout: 15000 });
  });
});
