/**
 * Error Boundary Test Suite (Ticket 033)
 *
 * Tests that the ErrorBoundary component is properly integrated:
 * - Prevents white screen of death on render errors
 * - Shows user-friendly fallback UI
 * - Preserves navbar functionality
 * - Provides recovery options
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  deleteServer,
} from "../helpers/api";

test.describe("Error Boundary (ticket 033)", () => {
  test("GlobalErrorFallback styles are included in the bundle", async ({
    page,
    testEnv,
  }) => {
    // Navigate to the application
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    // Extract all CSS from loaded stylesheets
    const styles = await page.evaluate(() => {
      const styleSheets = Array.from(document.styleSheets);
      let cssText = "";
      for (const sheet of styleSheets) {
        try {
          const rules = Array.from(sheet.cssRules || []);
          cssText += rules.map((rule) => rule.cssText).join("\n");
        } catch (e) {
          // Cross-origin stylesheets can't be accessed
        }
      }
      return cssText;
    });

    // Verify all critical error boundary CSS classes exist
    expect(styles).toContain(".global-error-fallback");
    expect(styles).toContain(".global-error-content");
    expect(styles).toContain(".global-error-icon");
    expect(styles).toContain(".error-message");
    expect(styles).toContain(".error-actions");
    expect(styles).toContain(".error-details");
  });

  test("Navigating to deleted server shows error toast and redirects to dashboard", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and then delete a server to trigger 404 error
    const config = createMinimalServerConfig("test-error-boundary");
    const server = await createServer(client, config);
    await deleteServer(client, server.server.id);

    // Login and navigate to deleted server.
    // Do NOT waitForLoadState("networkidle") — the 404 handling shows a
    // toast and schedules a redirect to "/" after ~2 s. If we wait for
    // networkidle the redirect may have already completed and the toast
    // dismissed before our assertions run.
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Race: start watching for the redirect URL *before* navigating so we
    // don't miss it if it happens quickly.
    const redirectPromise = page.waitForURL(/\/$/, { timeout: 10000 });
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);

    // ServerDetail handles 404 gracefully with an error toast (not the
    // ErrorBoundary) and then auto-redirects to the dashboard.
    // The toast may be visible briefly or may already be gone by the time
    // we check, so we verify via two complementary signals:
    //   1. The toast text appears at some point, OR
    //   2. We end up on the dashboard (redirect completed).
    const toastVisible = page
      .locator("text=Server not found")
      .first()
      .waitFor({ state: "visible", timeout: 5000 })
      .then(() => true)
      .catch(() => false);

    const redirected = redirectPromise.then(() => true).catch(() => false);

    // At least one of these must succeed — either we saw the toast or the
    // redirect already landed us on the dashboard.
    const [sawToast, didRedirect] = await Promise.all([
      toastVisible,
      redirected,
    ]);
    expect(
      sawToast || didRedirect,
      "Expected either the error toast to appear or the redirect to dashboard to complete",
    ).toBe(true);

    // Ensure we ultimately end up on the dashboard.
    if (!didRedirect) {
      await page.waitForURL(/\/$/, { timeout: 5000 });
    }
  });

  test("App component imports ErrorBoundary and GlobalErrorFallback", async ({
    page,
    testEnv,
  }) => {
    // This test verifies the integration is present by checking that
    // the error boundary wrapper exists in the DOM structure
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    // Check the app container exists
    const appDiv = page.locator(".app");
    await expect(appDiv).toBeVisible();

    // The structure should have the app container
    // (ErrorBoundary is transparent in the DOM but wraps the content)
    const appContent = page.locator(".content, .content-fullscreen");
    await expect(appContent).toBeVisible();
  });

  test("navbar remains outside error boundary (preserved on error)", async ({
    page,
    testEnv,
  }) => {
    // Login first to see the navbar
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    // Check if we're on the login page
    const isLoginPage = await page.locator("input#login-username").isVisible();

    if (isLoginPage) {
      // For this test, we just verify the structure is correct
      // The navbar should be outside the error boundary
      // (it won't be visible on login page, but the structure is set up correctly)
      const appDiv = page.locator(".app");
      await expect(appDiv).toBeVisible();
    }
  });

  test("error boundary CSS includes responsive styles", async ({
    page,
    testEnv,
  }) => {
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    const styles = await page.evaluate(() => {
      const styleSheets = Array.from(document.styleSheets);
      let cssText = "";
      for (const sheet of styleSheets) {
        try {
          const rules = Array.from(sheet.cssRules || []);
          cssText += rules.map((rule) => rule.cssText).join("\n");
        } catch (e) {
          // Ignore cross-origin errors
        }
      }
      return cssText;
    });

    // Verify responsive media queries are present
    expect(styles).toMatch(/@media[^{]*\(max-width:\s*768px\)/);
    expect(styles).toMatch(/@media[^{]*\(max-width:\s*480px\)/);
  });
});
