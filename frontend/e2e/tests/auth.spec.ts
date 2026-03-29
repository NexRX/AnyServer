/**
 * Auth & Setup Test Suite
 *
 * Tests authentication flows including:
 * - Login page loading
 * - Invalid credentials handling
 * - Valid credentials authentication
 * - Sign out functionality
 * - Token expiration handling
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaToken } from "../helpers/auth";

test.describe("Auth & Setup", () => {
  test("login page loads", async ({ page, testEnv }) => {
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    // Check that the login form is visible
    const usernameInput = page.locator("input#login-username");
    await expect(usernameInput).toBeVisible({ timeout: 5000 });

    const passwordInput = page.locator("input#login-password");
    await expect(passwordInput).toBeVisible();

    const submitButton = page.locator('button[type="submit"]');
    await expect(submitButton).toBeVisible();

    // Verify the AnyServer branding is present
    const heading = page.locator("h1:has-text('AnyServer')");
    await expect(heading).toBeVisible();
  });

  test("login with invalid credentials shows error", async ({
    page,
    testEnv,
  }) => {
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    await page.fill("input#login-username", "wronguser");
    await page.fill("input#login-password", "wrongpass");
    await page.click('button[type="submit"]');

    // Should show an error message
    const errorMsg = page.locator(".error-msg");
    await expect(errorMsg).toBeVisible({ timeout: 5000 });
  });

  test("login with valid credentials reaches dashboard", async ({
    page,
    testEnv,
  }) => {
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    await page.fill("input#login-username", "admin");
    await page.fill("input#login-password", "Admin123");

    // Click submit button
    await page.click('button[type="submit"]');

    // Check if there's an error message
    const errorMsg = page.locator(".error-msg");
    const hasError = await errorMsg.isVisible().catch(() => false);
    if (hasError) {
      const errorText = await errorMsg.textContent();
      throw new Error(`Login failed with error: ${errorText}`);
    }

    // Wait for redirect to dashboard (SPA navigation)
    await page.waitForURL(/.*\/$/, { timeout: 15000 });
    await page.waitForLoadState("networkidle");

    // Should show the navbar (indicating successful login)
    const navbar = page.locator("nav.navbar");
    await expect(navbar).toBeVisible({ timeout: 10000 });

    // Should show the Dashboard link in navbar
    const dashboardLink = page.locator("nav.navbar a:has-text('Dashboard')");
    await expect(dashboardLink).toBeVisible();
  });

  test("sign out returns to login page", async ({ page, testEnv }) => {
    // Log in via UI (token-based login has issues)
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    await page.fill("input#login-username", "admin");
    await page.fill("input#login-password", "Admin123");

    // Click submit and wait for redirect
    await page.click('button[type="submit"]');

    // Check for errors
    const errorMsg = page.locator(".error-msg");
    const hasError = await errorMsg.isVisible().catch(() => false);
    if (hasError) {
      const errorText = await errorMsg.textContent();
      throw new Error(`Login failed with error: ${errorText}`);
    }

    await page.waitForURL(/.*\/$/, { timeout: 15000 });
    await page.waitForLoadState("networkidle");

    // Wait for navbar to be visible (indicating auth is loaded)
    const navbar = page.locator("nav.navbar");
    await expect(navbar).toBeVisible({ timeout: 10000 });

    // Find and click logout button (has class "nav-logout" and text "Sign Out")
    const logoutBtn = page.locator("button.nav-logout");
    await expect(logoutBtn).toBeVisible({ timeout: 5000 });
    await logoutBtn.click();

    // Should redirect to login page
    await expect(page).toHaveURL(/.*\/login/, { timeout: 5000 });

    // Login form should be visible
    const usernameInput = page.locator("input#login-username");
    await expect(usernameInput).toBeVisible();
  });

  test("accessing protected route without token redirects to login", async ({
    page,
    testEnv,
  }) => {
    // Navigate to app without any token
    await page.goto(testEnv.baseUrl);

    // Clear any existing token
    await page.evaluate(() => {
      localStorage.removeItem("token");
    });

    // Try to access dashboard
    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    // Should redirect to login or setup page (setup is already complete via admin creation)
    // but the frontend might check setup_complete from settings
    const currentUrl = page.url();
    expect(
      currentUrl.includes("/login") || currentUrl.includes("/setup"),
    ).toBeTruthy();
  });

  test("expired/invalid token redirects to login", async ({
    page,
    testEnv,
  }) => {
    // Navigate to the app first
    await page.goto(testEnv.baseUrl);
    await page.waitForLoadState("networkidle");

    // Set an invalid token
    await page.evaluate(() => {
      localStorage.setItem("token", "invalid.jwt.token");
    });

    // Try to access dashboard
    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    // Give the auth context time to validate the token
    // The auth context will try to fetch /me, fail, and clear the token
    await page.waitForTimeout(2000);

    // Should redirect to login or setup after token validation fails
    const currentUrl = page.url();
    expect(
      currentUrl.includes("/login") || currentUrl.includes("/setup"),
    ).toBeTruthy();
  });

  test(
    "token persists across page reloads",
    {
      annotation: {
        type: "issue",
        description:
          "Test is flaky due to frontend startup race conditions. Retries added for robustness.",
      },
    },
    async ({ page, testEnv }) => {
      // Log in via UI - this sets the httpOnly refresh cookie
      await page.goto(`${testEnv.baseUrl}/login`);
      await page.waitForLoadState("networkidle");

      await page.fill("input#login-username", "admin");
      await page.fill("input#login-password", "Admin123");

      // Set up response promise to wait for login API call
      const loginResponsePromise = page.waitForResponse(
        (response) =>
          response.url().includes("/api/auth/login") &&
          response.status() === 200,
        { timeout: 15000 },
      );

      // Click submit and wait for redirect
      await page.click('button[type="submit"]');

      // Wait for the login API call to complete
      await loginResponsePromise;

      // Check for errors
      const errorMsg = page.locator(".error-msg");
      const hasError = await errorMsg.isVisible().catch(() => false);
      if (hasError) {
        const errorText = await errorMsg.textContent();
        throw new Error(`Login failed with error: ${errorText}`);
      }

      // Wait for redirect to dashboard
      await page.waitForURL(/.*\/$/, { timeout: 15000 });
      await page.waitForLoadState("networkidle");

      // Wait for navbar to appear (auth loaded)
      const navbar = page.locator("nav.navbar");
      await expect(navbar).toBeVisible({ timeout: 10000 });

      // Verify we can see the username before reload
      const usernameBeforeReload = page.locator(".nav-username");
      await expect(usernameBeforeReload).toBeVisible();
      await expect(usernameBeforeReload).toContainText("admin");

      // Set up response promise for /me API call that will happen after reload
      const meResponsePromise = page.waitForResponse(
        (response) =>
          response.url().includes("/api/auth/me") &&
          (response.status() === 200 || response.status() === 401),
        { timeout: 15000 },
      );

      // Reload the page - this clears in-memory token, auth context should use refresh cookie
      await page.reload({ waitUntil: "networkidle" });

      // Wait for the /me API call to complete (validates token from refresh)
      const meResponse = await meResponsePromise;
      if (meResponse.status() !== 200) {
        throw new Error(
          `Auth refresh failed after reload: /api/auth/me returned ${meResponse.status()}`,
        );
      }

      // Wait for navbar again after reload (confirms auth context initialized)
      await expect(navbar).toBeVisible({ timeout: 10000 });

      // Should still be logged in (not redirected to login)
      await expect(page).not.toHaveURL(/.*\/login/);

      // Should still see the username in the navbar (token persisted via refresh cookie)
      const usernameAfterReload = page.locator(".nav-username");
      await expect(usernameAfterReload).toBeVisible({ timeout: 5000 });
      await expect(usernameAfterReload).toContainText("admin");
    },
  );
});
