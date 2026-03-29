/**
 * Auth helper functions for E2E tests.
 * Provides utilities for login, token management, and auth state.
 *
 * After JWT refactoring (#026), tokens are stored in-memory (not localStorage)
 * and refreshed via httpOnly cookies. For E2E tests, we use addInitScript to
 * inject tokens into the in-memory storage before page loads.
 */

import type { Page } from "@playwright/test";

/**
 * Inject a JWT token into the app's in-memory storage using addInitScript.
 * This ensures the token is available before any page JavaScript runs.
 *
 * This should be called before navigating to authenticated pages.
 */
export async function injectToken(page: Page, token: string): Promise<void> {
  await page.addInitScript((tokenValue) => {
    // This script runs before the page's JavaScript, so we need to set up
    // a way to inject the token into the app's in-memory storage.
    // We'll use a global variable that the app can check on initialization.
    (window as any).__E2E_AUTH_TOKEN__ = tokenValue;
  }, token);
}

/**
 * Clear any injected token from the page context.
 */
export async function clearInjectedToken(page: Page): Promise<void> {
  await page.addInitScript(() => {
    delete (window as any).__E2E_AUTH_TOKEN__;
  });
}

/**
 * Log in via the UI using username and password.
 * After successful login, extracts the token and injects it for future navigations.
 *
 * This helper properly waits for:
 * 1. The login API call to complete
 * 2. Redirect to dashboard
 * 3. Auth context to initialize (navbar appears)
 * 4. Token to be injected for future navigations
 */
export async function loginViaUI(
  page: Page,
  baseUrl: string,
  username: string,
  password: string,
): Promise<void> {
  // Navigate to login page
  await page.goto(`${baseUrl}/login`);
  await page.waitForLoadState("networkidle");

  // Set up a promise to capture the login response
  const loginResponsePromise = page.waitForResponse(
    (response) =>
      response.url().includes("/api/auth/login") && response.status() === 200,
    { timeout: 15000 },
  );

  // Fill and submit login form
  await page.fill("input#login-username", username);
  await page.fill("input#login-password", password);
  await page.click('button[type="submit"]');

  // Wait for the login response and extract the token
  const response = await loginResponsePromise;
  const body = await response.json();
  const token = body.token;

  if (!token) {
    throw new Error("Login succeeded but no token in response");
  }

  // Wait for redirect to dashboard
  await page.waitForURL(/.*\//, { timeout: 10000 });
  await page.waitForLoadState("networkidle");

  // Wait for navbar to appear (confirms auth context loaded)
  await page.waitForSelector("nav.navbar", { timeout: 10000 });

  // Inject the token for future page loads
  await injectToken(page, token);

  // Set up promise to wait for /me API call after reload
  const meResponsePromise = page.waitForResponse(
    (response) =>
      response.url().includes("/api/auth/me") &&
      (response.status() === 200 || response.status() === 401),
    { timeout: 15000 },
  );

  // Reload the current page to ensure the init script takes effect
  // This ensures the E2E token is available in the auth context
  await page.reload({ waitUntil: "networkidle" });

  // Wait for the /me API call to complete (validates injected token)
  const meResponse = await meResponsePromise;
  if (meResponse.status() !== 200) {
    throw new Error(
      `Auth initialization failed after token injection: /api/auth/me returned ${meResponse.status()}`,
    );
  }

  // Wait for navbar to appear again, ensuring auth context is fully loaded
  await page.waitForSelector("nav.navbar", { timeout: 10000 });

  // Small delay to ensure auth state is settled
  await page.waitForTimeout(300);
}

/**
 * Log in by using an API token directly.
 * Injects the token for all subsequent page loads.
 *
 * This properly waits for the auth context to validate the token by:
 * 1. Injecting the token via addInitScript
 * 2. Navigating to the dashboard
 * 3. Waiting for /api/auth/me to complete
 * 4. Waiting for navbar to appear (confirms auth succeeded)
 */
export async function loginViaToken(
  page: Page,
  baseUrl: string,
  token: string,
): Promise<void> {
  // Inject the token before any page navigation
  await injectToken(page, token);

  // Set up promise to wait for /me API call
  const meResponsePromise = page.waitForResponse(
    (response) =>
      response.url().includes("/api/auth/me") &&
      (response.status() === 200 || response.status() === 401),
    { timeout: 15000 },
  );

  // Navigate to dashboard - the injected token will be available
  await page.goto(`${baseUrl}/`);
  await page.waitForLoadState("networkidle");

  // Wait for the /me API call to complete (validates token)
  const meResponse = await meResponsePromise;
  if (meResponse.status() !== 200) {
    throw new Error(
      `Token validation failed: /api/auth/me returned ${meResponse.status()}`,
    );
  }

  // Wait for the navbar to appear, indicating successful auth
  await page.waitForSelector("nav.navbar", { timeout: 10000 });

  // Give a small delay to ensure auth context is fully settled
  await page.waitForTimeout(300);
}

/**
 * Navigate to a URL while preserving authentication.
 * The token must have been previously injected via loginViaUI or loginViaToken.
 */
export async function navigateAuthenticated(
  page: Page,
  url: string,
): Promise<void> {
  await page.goto(url);
  await page.waitForLoadState("networkidle");
}

/**
 * Log out via the UI.
 */
export async function logout(page: Page): Promise<void> {
  const logoutBtn = page.locator(
    "button.nav-logout, button:has-text('Logout'), button:has-text('Sign Out')",
  );
  await logoutBtn.click();
  await page.waitForURL(/.*\/login/, { timeout: 5000 });

  // Clear the injected token
  await clearInjectedToken(page);
}

/**
 * Check if the user is currently logged in by checking for the navbar.
 * Note: With in-memory tokens, we can't check localStorage anymore.
 */
export async function isLoggedIn(page: Page): Promise<boolean> {
  try {
    const navbar = await page
      .locator("nav.navbar")
      .isVisible({ timeout: 2000 });
    return navbar;
  } catch {
    return false;
  }
}

/**
 * Ensure the user is logged in with the provided token.
 * If not logged in or on a different session, logs in with the token.
 */
export async function ensureLoggedIn(
  page: Page,
  baseUrl: string,
  token: string,
): Promise<void> {
  const loggedIn = await isLoggedIn(page);

  if (!loggedIn) {
    await loginViaToken(page, baseUrl, token);
  } else {
    // Already logged in, just inject the token for future navigations
    await injectToken(page, token);
  }
}

/**
 * Ensure the user is logged out.
 */
export async function ensureLoggedOut(
  page: Page,
  baseUrl: string,
): Promise<void> {
  const loggedIn = await isLoggedIn(page);

  if (loggedIn) {
    await logout(page);
  } else {
    // Clear any injected tokens just in case
    await clearInjectedToken(page);
  }

  // Navigate to login page
  await page.goto(`${baseUrl}/login`);
  await page.waitForLoadState("networkidle");
}
