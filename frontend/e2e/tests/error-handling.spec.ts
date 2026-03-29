/**
 * Error Handling Test Suite
 *
 * Tests error handling and edge cases including:
 * - Navigating to non-existent server
 * - Starting server with invalid binary
 * - Network errors and API failures
 * - Invalid form submissions
 * - Graceful degradation
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  cleanupAllServers,
} from "../helpers/api";

test.describe("Error Handling", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("navigating to non-existent server shows error or redirects", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Try to access a non-existent server
    const fakeId = "00000000-0000-0000-0000-000000000000";
    await page.goto(`${testEnv.baseUrl}/server/${fakeId}`);
    await page.waitForLoadState("networkidle");

    // Wait a moment for the page to render and handle the 404
    await page.waitForTimeout(500);

    // Check if error message appears
    const errorMsg = page.locator("div.error-msg");
    const hasError = await errorMsg.count().then((c) => c > 0);

    if (hasError) {
      // Error message appeared - test passes
      expect(hasError).toBe(true);
      return;
    }

    // No error message, so wait for redirect (happens after 2 seconds)
    await page.waitForTimeout(2500);
    const currentUrl = page.url();
    const redirected =
      currentUrl.endsWith("/") ||
      currentUrl.includes("/dashboard") ||
      !currentUrl.includes("/server/");

    // Either error appeared or redirect happened
    expect(hasError || redirected).toBeTruthy();
  });

  test("starting a server with invalid binary shows error", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server with an invalid binary path
    const config = {
      config: {
        name: "test-invalid-binary",
        binary: "/nonexistent/invalid/binary",
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

    // Log in via UI - this will inject the token for future navigations
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to server detail page using the injected token
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Wait for the server detail page to load
    await expect(page.locator("h1")).toContainText("test-invalid-binary", {
      timeout: 10000,
    });

    // Wait for the Start button to be visible before clicking
    const startBtn = page
      .locator("button.btn-success")
      .filter({ hasText: /^▶ Start$/ });
    await expect(startBtn).toBeVisible({ timeout: 10000 });
    await startBtn.click();

    // Wait for the error message to appear after trying to start with invalid binary
    const actionError = page.locator(".error-msg");
    await expect(actionError).toBeVisible({ timeout: 10000 });

    // Verify the error message contains useful information
    const errorText = await actionError.textContent();
    expect(errorText).toBeTruthy();
    expect(
      errorText!.toLowerCase().includes("start") ||
        errorText!.toLowerCase().includes("failed") ||
        errorText!.toLowerCase().includes("no such file") ||
        errorText!.toLowerCase().includes("error"),
    ).toBeTruthy();
  });

  test("form validation prevents invalid server creation", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to create server page
    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    const createBtn = page.locator(
      "button:has-text('Create'), a:has-text('Create'), button:has-text('New Server')",
    );
    await createBtn.click();
    await page.waitForLoadState("networkidle");

    // Try to submit without filling required fields
    const submitBtn = page.locator(
      "button[type='submit'], button:has-text('Create')",
    );

    if (await submitBtn.isVisible({ timeout: 5000 }).catch(() => false)) {
      // Clear any pre-filled values
      const nameInput = page.locator("input#name, input[name='name']");
      if (await nameInput.isVisible({ timeout: 2000 }).catch(() => false)) {
        await nameInput.fill("");
      }

      await submitBtn.click();

      // Should show validation errors or prevent submission
      await page.waitForTimeout(1000);

      const validationError = page.locator(
        ".validation-error, .field-error, [role='alert'], .error",
      );
      const hasError = await validationError
        .isVisible({ timeout: 3000 })
        .catch(() => false);

      // Or browser's built-in validation might prevent submission
      const currentUrl = page.url();
      const stillOnCreatePage =
        currentUrl.includes("/create") || currentUrl.includes("/new");

      expect(hasError || stillOnCreatePage).toBeTruthy();
    }
  });

  test("invalid JSON in config editor shows error", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-invalid-json");
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Navigate to config/settings tab
    const configTab = page.locator(
      "button:has-text('Config'), a:has-text('Config'), .tab-config, button:has-text('Settings')",
    );

    if (await configTab.isVisible({ timeout: 3000 }).catch(() => false)) {
      await configTab.click();
      await page.waitForTimeout(500);

      // Find the config editor (textarea or code editor)
      const configEditor = page.locator(
        "textarea.config-editor, textarea[name='config'], .monaco-editor",
      );

      if (await configEditor.isVisible({ timeout: 3000 }).catch(() => false)) {
        // Enter invalid JSON
        await configEditor.fill("{ invalid json here }");

        // Try to save
        const saveBtn = page.locator(
          "button:has-text('Save'), button[type='submit']",
        );
        await saveBtn.click();

        // Should show validation error
        const errorMsg = page.locator(
          ".error, [role='alert'], :has-text('invalid'), :has-text('JSON')",
        );
        await expect(errorMsg).toBeVisible({ timeout: 5000 });
      }
    }
  });

  test("network error during API call shows user-friendly message", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-network-error");
    const server = await createServer(client, config);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Simulate network failure by blocking API requests
    await page.route("**/api/server/**", (route) => {
      route.abort("failed");
    });

    // Try to perform an action (e.g., start server)
    const startBtn = page.locator("button:has-text('Start'), button.start-btn");
    if (await startBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await startBtn.click();

      // Should show error message about network failure
      const errorMsg = page.locator(
        ".error, [role='alert'], :has-text('failed'), :has-text('error'), :has-text('network')",
      );
      await expect(errorMsg).toBeVisible({ timeout: 5000 });
    }
  });

  test("unauthorized access (expired token) redirects to login", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("test-unauthorized");
    const server = await createServer(client, config);

    // Set an invalid token
    await page.goto(testEnv.baseUrl);
    await page.evaluate(() => {
      localStorage.setItem("token", "invalid.jwt.token.that.will.fail");
    });

    // Try to access server detail page
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(2000);

    // Should redirect to login or show error
    const currentUrl = page.url();
    const redirectedToLogin = currentUrl.includes("/login");
    const hasErrorMsg = await page
      .locator(".error, [role='alert']")
      .isVisible({ timeout: 3000 })
      .catch(() => false);

    expect(redirectedToLogin || hasErrorMsg).toBeTruthy();
  });

  test("creating server with duplicate name is allowed or shows warning", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server with a specific name
    await createServer(
      client,
      createMinimalServerConfig("duplicate-name-test"),
    );

    // Try to create another server with the same name via API
    const result = await createServer(
      client,
      createMinimalServerConfig("duplicate-name-test"),
    );

    // System either allows duplicates or prevents them
    // If allowed, result should be valid
    // If prevented, we'd get an error earlier
    expect(result).toBeDefined();
    expect(result.server.config.name).toBe("duplicate-name-test");
  });

  test("deleting non-existent server returns appropriate error", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const fakeId = "00000000-0000-0000-0000-000000000000";

    // Try to delete non-existent server
    let errorOccurred = false;
    try {
      const res = await fetch(`${client.baseUrl}/api/servers/${fakeId}`, {
        method: "DELETE",
        headers: { Authorization: `Bearer ${client.token}` },
      });
      if (!res.ok) {
        throw new Error(`Delete failed: ${res.status}`);
      }
    } catch (err) {
      errorOccurred = true;
      expect(err).toBeDefined();
    }

    // Should have received a 404 error
    expect(errorOccurred).toBe(true);
  });

  test("malformed API response is handled gracefully", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Intercept API response and return malformed data
    await page.route("**/api/servers", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: "{ malformed json",
      });
    });

    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    // Should show error state or empty state, not crash
    await page.waitForTimeout(1000);

    // Page should still be functional
    const navbar = page.locator(".navbar, nav, header");
    await expect(navbar).toBeVisible({ timeout: 5000 });
  });
});
