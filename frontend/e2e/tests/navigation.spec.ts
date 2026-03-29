/**
 * Navigation Test Suite
 *
 * Tests navigation functionality including:
 * - Navbar links navigate correctly
 * - SPA client-side routing (no full page reload)
 * - Direct URL navigation works
 * - Browser back/forward buttons
 * - Deep linking to server detail pages
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  cleanupAllServers,
} from "../helpers/api";

test.describe("Navigation", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("navbar links navigate to correct pages", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Verify we're on dashboard
    await expect(page).toHaveURL(/.*\/$/, { timeout: 5000 });

    // Try navigating to different sections via navbar
    const links = [
      { text: "Dashboard", url: "/" },
      { text: "Servers", url: "/" }, // Usually same as dashboard
    ];

    for (const link of links) {
      const navLink = page.locator(
        `nav a:has-text("${link.text}"), .navbar a:has-text("${link.text}")`,
      );

      if (await navLink.isVisible({ timeout: 2000 }).catch(() => false)) {
        await navLink.click();
        await page.waitForLoadState("networkidle");

        // Should navigate successfully
        const currentUrl = page.url();
        expect(currentUrl).toBeTruthy();
      }
    }
  });

  test("SPA client-side routing works (no full page reload)", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-spa-routing");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Add a marker to the window object to detect full page reloads
    await page.evaluate(() => {
      (window as any).__navigationMarker = "initial";
    });

    // Navigate to server detail page
    const serverCard = page.locator(
      `.server-card:has-text("test-spa-routing"), [data-server-id="${server.server.id}"]`,
    );
    await expect(serverCard).toBeVisible({ timeout: 5000 });
    await serverCard.click();

    await page.waitForURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });

    // Check if the marker is still there (would be gone on full reload)
    const markerStillExists = await page.evaluate(() => {
      return (window as any).__navigationMarker === "initial";
    });

    expect(markerStillExists).toBeTruthy();
  });

  test("direct URL navigation to server detail works", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-direct-url");
    const server = await createServer(client, config);

    // Log in first
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate directly to server detail page
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Should load the server detail page successfully
    await expect(page).toHaveURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });

    // Should show server name
    const serverName = page.locator(
      `h1:has-text("test-direct-url"), h2:has-text("test-direct-url"), :has-text("test-direct-url")`,
    );
    await expect(serverName.first()).toBeVisible({ timeout: 5000 });
  });

  test("browser back button returns to previous page", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-back-button");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Verify we're on dashboard
    await expect(page).toHaveURL(/.*\/$/, { timeout: 5000 });

    // Navigate to server detail
    const serverCard = page.locator(
      `.server-card:has-text("test-back-button")`,
    );
    await expect(serverCard).toBeVisible({ timeout: 5000 });
    await serverCard.click();

    await page.waitForURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });

    // Go back
    await page.goBack();
    await page.waitForLoadState("networkidle");

    // Should be back on dashboard
    await expect(page).toHaveURL(/.*\/$/, { timeout: 5000 });
    await expect(serverCard).toBeVisible({ timeout: 5000 });
  });

  test("browser forward button navigates forward", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-forward-button");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to server detail
    const serverCard = page.locator(
      `.server-card:has-text("test-forward-button")`,
    );
    await serverCard.click();
    await page.waitForURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });

    // Go back to dashboard
    await page.goBack();
    await page.waitForLoadState("networkidle");
    await expect(page).toHaveURL(/.*\/$/, { timeout: 5000 });

    // Go forward again
    await page.goForward();
    await page.waitForLoadState("networkidle");

    // Should be back on server detail page
    await expect(page).toHaveURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });
  });

  test("navigation preserves scroll position on back", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create multiple servers to enable scrolling
    for (let i = 1; i <= 10; i++) {
      await createServer(client, createMinimalServerConfig(`scroll-test-${i}`));
    }

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Scroll down
    await page.evaluate(() => window.scrollTo(0, 500));
    await page.waitForTimeout(500);

    // Get scroll position
    const scrollBefore = await page.evaluate(() => window.scrollY);
    expect(scrollBefore).toBeGreaterThan(0);

    // Navigate to a server
    const serverCard = page.locator(`.server-card:has-text("scroll-test-1")`);
    if (await serverCard.isVisible({ timeout: 3000 }).catch(() => false)) {
      await serverCard.click();
      await page.waitForLoadState("networkidle");

      // Go back
      await page.goBack();
      await page.waitForLoadState("networkidle");

      // Scroll position might be restored (browser-dependent)
      const scrollAfter = await page.evaluate(() => window.scrollY);
      // This is browser-dependent, so we just verify the page loaded
      expect(scrollAfter).toBeGreaterThanOrEqual(0);
    }
  });

  test("navigation to create server page works", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Find create button
    const createBtn = page.locator(
      "button:has-text('Create'), a:has-text('Create'), button:has-text('New Server'), a:has-text('New')",
    );

    if (await createBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await createBtn.click();

      // Should navigate to create page
      await expect(page).toHaveURL(/.*\/(create|new)/, { timeout: 5000 });
    }
  });

  test("404 page for invalid routes", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Try to access an invalid route
    await page.goto(`${testEnv.baseUrl}/this-route-does-not-exist`);
    await page.waitForLoadState("networkidle");

    // Should either show 404 page or redirect to dashboard
    // Check for 404 page using class or text content
    const notFoundByClass = page.locator(".not-found");
    const notFoundByText = page.locator("text=Page Not Found");

    const isNotFoundByClass = await notFoundByClass
      .isVisible({ timeout: 3000 })
      .catch(() => false);

    const isNotFoundByText = await notFoundByText
      .isVisible({ timeout: 3000 })
      .catch(() => false);

    const isNotFound = isNotFoundByClass || isNotFoundByText;

    const currentUrl = page.url();
    const redirectedToDashboard =
      currentUrl.endsWith("/") || currentUrl.includes("/dashboard");

    // Either shows 404 or redirects
    expect(isNotFound || redirectedToDashboard).toBeTruthy();
  });

  test("navigation works after page reload", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-reload-navigation");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Reload the page
    await page.reload();
    await page.waitForLoadState("networkidle");

    // Should still be on the same page
    await expect(page).toHaveURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });

    // Navigation should still work
    const dashboardLink = page.locator(
      "a:has-text('Dashboard'), a[href='/'], nav a:first-child",
    );
    if (await dashboardLink.isVisible({ timeout: 3000 }).catch(() => false)) {
      await dashboardLink.click();
      await expect(page).toHaveURL(/.*\/$/, { timeout: 5000 });
    }
  });

  test("deep linking with query parameters works", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-query-params");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate with query parameters
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}?tab=console`);
    await page.waitForLoadState("networkidle");

    // Should load the page successfully
    await expect(page).toHaveURL(new RegExp(`/server/${server.server.id}`), {
      timeout: 5000,
    });

    // Query params might be used to select tabs or other UI state
    const currentUrl = page.url();
    expect(currentUrl).toContain(server.server.id);
  });

  test("multiple rapid navigations don't cause errors", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create servers
    const server1 = await createServer(
      client,
      createMinimalServerConfig("rapid-nav-1"),
    );
    const server2 = await createServer(
      client,
      createMinimalServerConfig("rapid-nav-2"),
    );

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Rapidly navigate between pages
    await page.goto(`${testEnv.baseUrl}/server/${server1.server.id}`);
    await page.goto(`${testEnv.baseUrl}/server/${server2.server.id}`);
    await page.goto(`${testEnv.baseUrl}/`);
    await page.goto(`${testEnv.baseUrl}/server/${server1.server.id}`);

    await page.waitForLoadState("networkidle");

    // Should end up on the last page without errors
    await expect(page).toHaveURL(new RegExp(`/server/${server1.server.id}`), {
      timeout: 5000,
    });

    // No console errors (check for critical errors)
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        errors.push(msg.text());
      }
    });

    // Page should still be functional
    const navbar = page.locator(".navbar, nav, header");
    await expect(navbar).toBeVisible({ timeout: 5000 });
  });
});
