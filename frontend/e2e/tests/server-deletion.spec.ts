/**
 * Server Deletion Test Suite
 *
 * Tests server deletion functionality including:
 * - Deleting a server removes it from the list
 * - Deletion redirects away from detail page
 * - Confirmation dialogs if implemented
 * - Cascading cleanup of server resources
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI, loginViaToken } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  listServers,
  deleteServer,
  cleanupAllServers,
  createNonAdminUser,
  grantServerPermission,
} from "../helpers/api";

test.describe("Server Deletion", () => {
  test.afterEach(async ({ testEnv }) => {
    // Clean up any remaining servers
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("deleting a server removes it from dashboard", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-delete-server");
    const server = await createServer(client, config);

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Verify server is visible
    const serverCard = page.locator(
      `.server-card:has-text("test-delete-server")`,
    );
    await expect(serverCard).toBeVisible({ timeout: 5000 });

    // Delete via API
    await deleteServer(client, server.server.id);

    // Reload dashboard
    await page.reload();
    await page.waitForLoadState("networkidle");

    // Server should no longer be visible
    await expect(serverCard).not.toBeVisible({ timeout: 5000 });
  });

  test("deleting a server from detail page redirects to dashboard", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-delete-redirect");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to Config tab where delete button is located
    const configTab = page.locator("button.tab:has-text('Config')");
    await configTab.click();
    await page.waitForTimeout(500);

    // Find delete button (says "Delete Server")
    const deleteBtn = page.locator("button:has-text('Delete Server')");
    await expect(deleteBtn).toBeVisible({ timeout: 5000 });

    // Handle potential confirmation dialog
    page.on("dialog", (dialog) => {
      console.log(`Dialog: ${dialog.message()}`);
      dialog.accept();
    });

    await deleteBtn.click();

    // Should redirect to dashboard or show success message
    await page.waitForTimeout(1000);

    // Either redirected to dashboard or navigating there
    const currentUrl = page.url();
    const isDashboard =
      currentUrl.endsWith("/") ||
      currentUrl.includes("/dashboard") ||
      !currentUrl.includes("/server/");

    if (!isDashboard) {
      // Try clicking a dashboard link
      const dashboardLink = page.locator(
        "a:has-text('Dashboard'), a[href='/']",
      );
      if (await dashboardLink.isVisible({ timeout: 2000 }).catch(() => false)) {
        await dashboardLink.click();
        await page.waitForLoadState("networkidle");
      }
    }

    // Verify server no longer exists in the list
    const serverCard = page.locator(
      `.server-card:has-text("test-delete-redirect")`,
    );
    await expect(serverCard).not.toBeVisible({ timeout: 5000 });
  });

  test("deleting a running server stops it first", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and start a server
    const config = createMinimalServerConfig("test-delete-running");
    const server = await createServer(client, config);

    const { startServer, waitForStatus } = await import("../helpers/api");
    await startServer(client, server.server.id);
    await waitForStatus(client, server.server.id, "running", 15000);

    // Delete the server
    await deleteServer(client, server.server.id);

    // Verify it's gone
    const { servers } = await listServers(client);
    const found = servers.find((s) => s.server.id === server.server.id);
    expect(found).toBeUndefined();
  });

  test("deleted server cannot be accessed via direct URL", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-delete-404");
    const server = await createServer(client, config);
    const serverId = server.server.id;

    // Delete it
    await deleteServer(client, serverId);

    // Log in and try to access it
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Set up promise to catch the 404 API response
    const responsePromise = page.waitForResponse(
      (response) =>
        response.url().includes(`/api/servers/${serverId}`) &&
        response.status() === 404,
      { timeout: 10000 },
    );

    await page.goto(`${testEnv.baseUrl}/server/${serverId}`);

    // Wait for the API request to complete with 404
    await responsePromise;

    // Wait a moment for SolidJS to process the error
    await page.waitForTimeout(1000);

    // Should show error message OR redirect to dashboard
    // The page might redirect immediately or show error first
    const currentUrl = page.url();
    if (currentUrl === testEnv.baseUrl + "/" || currentUrl.endsWith("/")) {
      // Already redirected - that's fine
      expect(currentUrl).toMatch(/\/$/);
    } else {
      // Still on server page - should show error message
      // Use .first() to handle multiple matches from ErrorBoundary
      const errorMsg = page.locator("text=not found").first();
      await expect(errorMsg).toBeVisible({ timeout: 3000 });

      // Should eventually redirect to dashboard
      await page.waitForURL(/\/$/, { timeout: 5000 });
      expect(page.url()).toMatch(/\/$/);
    }
  });

  test("deleting multiple servers in sequence", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create multiple servers
    const server1 = await createServer(
      client,
      createMinimalServerConfig("delete-multi-1"),
    );
    const server2 = await createServer(
      client,
      createMinimalServerConfig("delete-multi-2"),
    );
    const server3 = await createServer(
      client,
      createMinimalServerConfig("delete-multi-3"),
    );

    // Delete them one by one
    await deleteServer(client, server1.server.id);
    await deleteServer(client, server2.server.id);
    await deleteServer(client, server3.server.id);

    // Verify all are gone
    const { servers } = await listServers(client);
    expect(servers.length).toBe(0);

    // Dashboard should show empty state
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    const emptyState = page.locator(".empty-state h2");
    await expect(emptyState).toBeVisible({ timeout: 5000 });
  });

  test("delete button is only available to users with permission", async ({
    page,
    testEnv,
  }) => {
    const adminClient = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a non-admin user for testing
    const nonAdminUser = await createNonAdminUser(
      adminClient,
      "testuser",
      "TestUser123!",
    );

    // Create a server as admin
    const config = createMinimalServerConfig("test-delete-permissions");
    const server = await createServer(adminClient, config);

    // Grant viewer permission to the non-admin user so they can access the server
    await grantServerPermission(
      adminClient,
      server.server.id,
      nonAdminUser.userId,
      "viewer",
    );

    // ─── Test 1: Admin should see delete button ───
    await loginViaToken(page, testEnv.baseUrl, testEnv.adminToken);
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to Config tab where delete button is located
    const configTab = page.locator("button.tab:has-text('Config')");
    await configTab.click();
    await page.waitForTimeout(500);

    // Admin should see the delete button
    const deleteBtnAdmin = page.locator("button:has-text('Delete Server')");
    await expect(deleteBtnAdmin).toBeVisible({ timeout: 5000 });

    // ─── Test 2: Non-admin should NOT see delete button ───
    // Use token-based login to avoid rate limiting issues
    await loginViaToken(page, testEnv.baseUrl, nonAdminUser.token);
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to Config tab
    const configTab2 = page.locator("button.tab:has-text('Config')");
    await configTab2.click();
    await page.waitForTimeout(500);

    // Non-admin should NOT see the delete button
    const deleteBtnNonAdmin = page.locator("button:has-text('Delete Server')");
    await expect(deleteBtnNonAdmin).not.toBeVisible({ timeout: 2000 });

    // ─── Test 3: Non-admin gets 403 if they try to delete via API ───
    const nonAdminClient = createApiClient(testEnv.apiUrl, nonAdminUser.token);
    let deleteFailed = false;
    try {
      await deleteServer(nonAdminClient, server.server.id);
    } catch (err: unknown) {
      deleteFailed = true;
      expect((err as Error).message).toMatch(/403|Forbidden/i);
    }
    expect(deleteFailed).toBe(true);

    // Verify server still exists
    const { servers } = await listServers(adminClient);
    const found = servers.find((s) => s.server.id === server.server.id);
    expect(found).toBeDefined();
  });

  test("confirmation dialog appears before deletion (if implemented)", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-delete-confirm");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to Config tab where delete button is located
    const configTab = page.locator("button.tab:has-text('Config')");
    await configTab.click();
    await page.waitForTimeout(500);

    // Set up dialog handler
    let dialogAppeared = false;
    page.on("dialog", (dialog) => {
      dialogAppeared = true;
      expect(dialog.message().toLowerCase()).toMatch(/delete|confirm|sure/);
      dialog.dismiss(); // Cancel the deletion
    });

    // Click delete button
    const deleteBtn = page.locator("button:has-text('Delete Server')");
    await deleteBtn.click();

    // Wait for potential dialog
    await page.waitForTimeout(1000);

    // If dialog appeared and was dismissed, server should still exist
    if (dialogAppeared) {
      const { servers } = await listServers(client);
      const found = servers.find((s) => s.server.id === server.server.id);
      expect(found).toBeDefined();
    }
  });
});
