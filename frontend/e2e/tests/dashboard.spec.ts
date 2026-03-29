/**
 * Dashboard Test Suite
 *
 * Tests dashboard functionality including:
 * - Listing all servers
 * - Live status updates via global WebSocket
 * - Clicking server cards to navigate
 * - Empty state when no servers exist
 * - Server filtering/search if implemented
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
} from "../helpers/api";

test.describe("Dashboard", () => {
  test.afterEach(async ({ testEnv }) => {
    // Clean up servers after each test
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("dashboard lists all servers", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create multiple servers
    const server1 = await createServer(
      client,
      createMinimalServerConfig("test-server-1"),
    );
    const server2 = await createServer(
      client,
      createMinimalServerConfig("test-server-2"),
    );
    const server3 = await createServer(
      client,
      createMinimalServerConfig("test-server-3"),
    );

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Should see all three servers
    await expect(
      page.locator(`.server-card:has-text("test-server-1")`),
    ).toBeVisible({ timeout: 5000 });
    await expect(
      page.locator(`.server-card:has-text("test-server-2")`),
    ).toBeVisible();
    await expect(
      page.locator(`.server-card:has-text("test-server-3")`),
    ).toBeVisible();

    // Count server cards
    const serverCards = page.locator(".server-card, .server-item");
    const count = await serverCards.count();
    expect(count).toBeGreaterThanOrEqual(3);
  });

  test("dashboard reflects live status changes via global WS", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-live-status");
    const server = await createServer(client, config);

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Find the server card
    const serverCard = page.locator(
      `.server-card:has-text("test-live-status"), [data-server-id="${server.server.id}"]`,
    );
    await expect(serverCard).toBeVisible({ timeout: 5000 });

    // Initial status should be stopped
    const statusBadge = serverCard.locator(
      ".status-badge, .server-status, [data-status]",
    );
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 5000 });

    // Start the server via API
    await startServer(client, server.server.id);

    // Status should update to running without page reload
    await expect(statusBadge).toContainText(/running/i, { timeout: 10000 });

    // Stop the server via API
    await stopServer(client, server.server.id);

    // Status should update back to stopped
    await expect(statusBadge).toContainText(/stopped/i, { timeout: 10000 });
  });

  test("clicking a server card navigates to detail page", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-navigation");
    const server = await createServer(client, config);

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Find and click the server card
    const serverCard = page.locator(
      `.server-card:has-text("test-navigation"), [data-server-id="${server.server.id}"]`,
    );
    await expect(serverCard).toBeVisible({ timeout: 5000 });
    await serverCard.click();

    // Should navigate to server detail page
    await expect(page).toHaveURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });

    // Server detail page should show the server name
    await expect(
      page.locator(
        `h1:has-text("test-navigation"), h2:has-text("test-navigation")`,
      ),
    ).toBeVisible({
      timeout: 5000,
    });
  });

  test("empty state shown when no servers exist", async ({ page, testEnv }) => {
    // Don't create any servers

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Should show empty state
    const emptyState = page.locator(".empty-state");
    await expect(emptyState).toBeVisible({ timeout: 5000 });

    // Should have a create server button or link
    const createBtn = page
      .locator(
        "button:has-text('Create'), a:has-text('Create'), button:has-text('New Server')",
      )
      .first();
    await expect(createBtn).toBeVisible();
  });

  test("dashboard shows correct server count", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a specific number of servers
    await createServer(client, createMinimalServerConfig("server-1"));
    await createServer(client, createMinimalServerConfig("server-2"));

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Count server cards
    const serverCards = page.locator(".server-card, .server-item");
    const count = await serverCards.count();
    expect(count).toBe(2);
  });

  test("dashboard updates when server is created via UI", async ({
    page,
    testEnv,
  }) => {
    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Initial state might be empty or have servers
    const initialCount = await page
      .locator(".server-card, .server-item")
      .count();

    // Navigate to create server page
    const createBtn = page
      .locator(
        "button:has-text('Create'), a:has-text('Create'), button:has-text('New Server'), a:has-text('New')",
      )
      .first();
    await createBtn.click();

    // Should navigate to create page
    await expect(page).toHaveURL(/.*\/(create|new)/, { timeout: 5000 });

    // Switch to wizard mode (basic mode was removed; template is default)
    const wizardBtn = page.locator(".creation-mode-btn:has-text('Wizard')");
    await wizardBtn.click();

    // Step 1: Parameters — nothing to fill, just click Next
    const nextBtn = () => page.locator("button:has-text('Next →')");
    await nextBtn().click();

    // Step 2: Basics — fill server name
    const nameInput = page.locator("input#wiz-name");
    await expect(nameInput).toBeVisible({ timeout: 5000 });
    await nameInput.fill("ui-created-server");
    await nextBtn().click();

    // Step 3: Start Command — fill binary and args
    const binaryInput = page.locator("input#wiz-binary");
    await expect(binaryInput).toBeVisible({ timeout: 5000 });
    await binaryInput.fill("/run/current-system/sw/bin/sleep");

    const argsInput = page.locator("input#wiz-args");
    if (await argsInput.isVisible({ timeout: 1000 }).catch(() => false)) {
      await argsInput.fill("infinity");
    }
    await nextBtn().click();

    // Step 4: Install Steps — skip
    await nextBtn().click();

    // Step 5: Update Steps — skip
    await nextBtn().click();

    // Step 6: Review & Create — submit
    const submitBtn = page.locator("button:has-text('Create Server')");
    await submitBtn.click();

    // Should redirect back to dashboard or server detail
    await page.waitForURL(/.*\/(server|$)/, { timeout: 10000 });

    // Always navigate to dashboard to ensure we're showing the server list
    await page.goto(testEnv.baseUrl, { waitUntil: "networkidle" });

    // Wait for dashboard to load and refetch servers
    await page.waitForSelector(".page-header h1:has-text('Servers')", {
      timeout: 5000,
    });

    // New server should appear in the list
    await expect(
      page.locator(`.server-card:has-text("ui-created-server")`),
    ).toBeVisible({ timeout: 10000 });

    // Server count should have increased
    const newCount = await page.locator(".server-card, .server-item").count();
    expect(newCount).toBe(initialCount + 1);
  });

  test("dashboard shows server status badges correctly", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create servers in different states
    const stoppedServer = await createServer(
      client,
      createMinimalServerConfig("stopped-server"),
    );

    const runningServer = await createServer(
      client,
      createMinimalServerConfig("running-server"),
    );
    await startServer(client, runningServer.server.id);
    await waitForStatus(client, runningServer.server.id, "running", 15000);

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Check stopped server badge
    const stoppedCard = page.locator(`.server-card:has-text("stopped-server")`);
    const stoppedBadge = stoppedCard.locator(
      ".status-badge, .server-status, [data-status]",
    );
    await expect(stoppedBadge).toContainText(/stopped/i, { timeout: 5000 });

    // Check running server badge
    const runningCard = page.locator(`.server-card:has-text("running-server")`);
    const runningBadge = runningCard.locator(
      ".status-badge, .server-status, [data-status]",
    );
    await expect(runningBadge).toContainText(/running/i, { timeout: 5000 });
  });

  test("dashboard refreshes when navigating back from server detail", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-back-navigation");
    const server = await createServer(client, config);

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to server detail
    const serverCard = page.locator(
      `.server-card:has-text("test-back-navigation")`,
    );
    await serverCard.click();
    await page.waitForLoadState("networkidle");

    // Go back to dashboard
    await page.goBack();
    await page.waitForLoadState("networkidle");

    // Server should still be visible
    await expect(serverCard).toBeVisible({ timeout: 5000 });
  });

  test("sidebar filters only appear when there are more than 10 servers", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Log in first
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Initially with 0 servers, sidebar should not be visible
    const sidebar = page.locator(".dashboard-sidebar");
    await expect(sidebar).not.toBeVisible();

    // Create exactly 10 servers
    for (let i = 1; i <= 10; i++) {
      await createServer(client, createMinimalServerConfig(`test-server-${i}`));
    }

    // Reload the page
    await page.reload({ waitUntil: "networkidle" });

    // With exactly 10 servers, sidebar should still not be visible
    await expect(sidebar).not.toBeVisible();

    // Create one more server (11 total)
    await createServer(client, createMinimalServerConfig("test-server-11"));

    // Reload the page
    await page.reload({ waitUntil: "networkidle" });

    // Now sidebar should be visible
    await expect(sidebar).toBeVisible({ timeout: 5000 });

    // Sidebar should contain the filter controls
    await expect(sidebar.locator(".search-input")).toBeVisible();
    await expect(sidebar.locator(".status-filter")).toBeVisible();
    await expect(sidebar.locator(".per-page-selector")).toBeVisible();
  });

  test("sidebar filters work correctly when visible", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create 12 servers with different names
    for (let i = 1; i <= 12; i++) {
      await createServer(
        client,
        createMinimalServerConfig(
          i % 2 === 0 ? `even-server-${i}` : `odd-server-${i}`,
        ),
      );
    }

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Sidebar should be visible
    const sidebar = page.locator(".dashboard-sidebar");
    await expect(sidebar).toBeVisible({ timeout: 5000 });

    // Test search functionality
    const searchInput = sidebar.locator(".search-input");
    await searchInput.fill("even");

    // Wait for debounce and filtering
    await page.waitForTimeout(500);

    // Should show only even servers
    await expect(
      page.locator(".server-card:has-text('even-server')"),
    ).toHaveCount(6, { timeout: 5000 });

    // Clear search
    const clearBtn = sidebar.locator(".sidebar-clear-btn");
    if (await clearBtn.isVisible()) {
      await clearBtn.click();
    } else {
      await searchInput.clear();
      await page.waitForTimeout(500);
    }

    // Should show all servers again
    const allCards = page.locator(".server-card");
    const count = await allCards.count();
    expect(count).toBe(12);
  });
});
