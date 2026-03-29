/**
 * WebSocket Reconnect Stability Test Suite
 *
 * Regression tests ensuring the "Connection lost — reconnecting…" banner
 * never appears during normal operations under two conditions:
 *
 * 1. After creating a server, running the install pipeline, and then
 *    starting it — all without a full page refresh.
 * 2. After using the "Reset Server" button from the danger zone.
 *
 * Historical context:
 * These tests were originally written to reproduce a bug where the banner
 * would flash repeatedly due to a coupling between Console component
 * mount/unmount cycles and WebSocket lifecycle. The root cause was
 * structurally eliminated by the WebSocket architecture redesign:
 *
 * - WebSocket connections are now owned by the `useServerConsole` hook
 *   in ServerDetail, not by the Console component. Console is a pure
 *   rendering component that receives data via props.
 * - The `ConnectionBanner` component has built-in debounce logic,
 *   only showing after a sustained disconnect (default 3s).
 * - The `ReconnectingWebSocket` class handles reconnection with
 *   exponential backoff, jitter, and generation tracking.
 *
 * These tests remain as regression guards to ensure the banner never
 * appears during transient, expected WebSocket reconnections.
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  startServer,
  waitForStatus,
  cleanupAllServers,
  installServer,
  waitForPhaseComplete,
  resetServer,
  enableRunCommands,
  getServer,
} from "../helpers/api";

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

/**
 * Helper: assert the disconnect banner does NOT appear at all over a
 * monitoring window.  The banner should never be shown during normal
 * operations like pipeline completion or server reset — the
 * ConnectionBanner's built-in debounce ensures transient disconnects
 * (which resolve within the debounce window) never surface to the user.
 */
async function assertBannerNeverAppears(
  page: import("@playwright/test").Page,
  monitorMs = 6000,
  sampleIntervalMs = 200,
) {
  const banner = page.locator(".ws-disconnect-banner");

  let visibleCount = 0;
  const samples = Math.ceil(monitorMs / sampleIntervalMs);

  for (let i = 0; i < samples; i++) {
    const visible = await banner.isVisible().catch(() => false);
    if (visible) {
      visibleCount++;
    }
    await page.waitForTimeout(sampleIntervalMs);
  }

  expect(
    visibleCount,
    `Disconnect banner was visible for ${visibleCount}/${samples} samples ` +
      `over ${monitorMs}ms — expected 0. ` +
      `The banner should never appear during normal operations.`,
  ).toBe(0);
}

test.describe("WebSocket reconnect stability", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("no banner flapping after create → install → start (without page refresh)", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands so the install pipeline can execute
    await enableRunCommands(client, true);

    // Create a server with a trivial install pipeline so we can exercise
    // the install-then-start flow without a page refresh.
    const config = createMinimalServerConfig("test-install-start-stability");
    config.config.install_steps = [
      {
        name: "create-marker",
        description: "Write a marker file to prove install ran",
        action: {
          type: "run_command",
          command: "/run/current-system/sw/bin/echo",
          args: ["installed"],
          working_dir: null,
          env: {},
        },
        condition: null,
        continue_on_error: false,
      },
    ];

    const server = await createServer(client, config);

    // Log in and navigate directly to the server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Verify we see the server name
    await expect(page.locator("h1")).toContainText(
      "test-install-start-stability",
      { timeout: 10000 },
    );

    // Wait for console to connect (the status dot should show "Connected")
    await expect(page.locator(".console-status")).toContainText("Connected", {
      timeout: 10000,
    });

    // --- Step 1: Run the install pipeline via the UI button ---
    const installBtn = page.locator("button").filter({ hasText: /Install/ });
    await expect(installBtn).toBeVisible({ timeout: 5000 });
    await expect(installBtn).toBeEnabled();
    await installBtn.click();

    // Wait for the install pipeline to finish via API polling
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalled(client, server.server.id);

    // Give the UI a moment to settle after pipeline completion
    await page.waitForTimeout(1500);

    // The console should re-establish and show "Connected"
    await expect(page.locator(".console-status")).toContainText("Connected", {
      timeout: 15000,
    });

    // --- Step 2: Start the server (without refreshing the page) ---
    const startBtn = page
      .locator("button.btn-success")
      .filter({ hasText: /Start/ });
    await expect(startBtn).toBeVisible({ timeout: 5000 });
    await expect(startBtn).toBeEnabled({ timeout: 5000 });
    await startBtn.click();

    // Wait for server to be running
    await waitForStatus(client, server.server.id, "running", 15000);

    // --- Step 3: Monitor for the disconnect banner ---
    // The banner should never appear during normal operations.
    // The useServerConsole hook handles reconnection internally, and
    // the ConnectionBanner's debounce prevents transient disconnects
    // from surfacing to the user.
    await assertBannerNeverAppears(page, 6000);

    // Final sanity check: page should still be fully functional
    await expect(page.locator(".console-status")).toContainText("Connected", {
      timeout: 5000,
    });
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 5000 });
  });

  test("no banner flapping after server reset", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable run commands so the install pipeline can execute
    await enableRunCommands(client, true);

    // Create and start a server so there is an active process + WS channel
    const config = createMinimalServerConfig("test-reset-stability");
    config.config.install_steps = [
      {
        name: "setup",
        description: null,
        action: {
          type: "run_command",
          command: "/run/current-system/sw/bin/echo",
          args: ["setup-done"],
          working_dir: null,
          env: {},
        },
        condition: null,
        continue_on_error: false,
      },
    ];
    const server = await createServer(client, config);

    // Install via API so the server is in "installed" state
    await installServer(client, server.server.id);
    await waitForPhaseComplete(client, server.server.id, 30000);
    await waitForInstalled(client, server.server.id);

    // Start via API
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Log in and navigate to the server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Confirm the page is fully loaded and the WS is connected
    await expect(page.locator("h1")).toContainText("test-reset-stability", {
      timeout: 10000,
    });
    await expect(page.locator(".console-status")).toContainText("Connected", {
      timeout: 10000,
    });
    const statusBadge = page.locator(".status-badge").first();
    await expect(statusBadge).toContainText(/running/i, { timeout: 5000 });

    // --- Step 1: Navigate to the Configuration tab and click Reset Server ---
    const configTab = page
      .locator("button.tab")
      .filter({ hasText: /Configuration/ });
    await expect(configTab).toBeVisible({ timeout: 5000 });
    await configTab.click();

    // The Reset Server button is in the danger zone
    const resetBtn = page
      .locator("button.btn-danger")
      .filter({ hasText: /Reset Server/ });
    await expect(resetBtn).toBeVisible({ timeout: 5000 });

    // Accept the confirmation dialog that will appear
    page.once("dialog", (dialog) => dialog.accept());
    await resetBtn.click();

    // Wait for the reset to complete — the server should become
    // uninstalled (stopped).  We can detect this via the API.
    await waitForStatus(client, server.server.id, "stopped", 20000);

    // Give the UI a moment to process the reset
    await page.waitForTimeout(1500);

    // Switch back to Console tab so the Console component is mounted
    const consoleTab = page
      .locator("button.tab")
      .filter({ hasText: /Console/ });
    await expect(consoleTab).toBeVisible({ timeout: 5000 });
    await consoleTab.click();

    // Wait for the console to reconnect after the reset
    await expect(page.locator(".console-status")).toContainText("Connected", {
      timeout: 15000,
    });

    // --- Step 2: Monitor for the disconnect banner ---
    await assertBannerNeverAppears(page, 6000);

    // Final sanity: page should be functional, showing stopped status
    await expect(page.locator(".console-status")).toContainText("Connected", {
      timeout: 5000,
    });
  });
});
