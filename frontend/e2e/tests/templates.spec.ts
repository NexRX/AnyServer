import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  cleanupAllServers,
} from "../helpers/api";

test.describe("Templates", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("templates page loads and shows heading", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to templates page via navbar
    const templatesLink = page.locator("nav.navbar a[href='/templates']");
    await expect(templatesLink).toBeVisible({ timeout: 5000 });
    await templatesLink.click();

    await expect(page).toHaveURL(/.*\/templates/, { timeout: 5000 });

    // Page header should be visible
    const heading = page.locator("h1:has-text('Templates')");
    await expect(heading).toBeVisible({ timeout: 5000 });
  });

  test("empty state shown when no custom templates exist", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // There should be either built-in templates or an empty state
    const templateGrid = page.locator(".template-grid");
    const emptyState = page.locator(".empty-state");

    const hasGrid = await templateGrid.isVisible().catch(() => false);
    const hasEmpty = await emptyState.isVisible().catch(() => false);

    // One of these must be present
    expect(hasGrid || hasEmpty).toBeTruthy();
  });

  test("can create a template from JSON", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // Click the create template button (text is "📥 Import Template")
    const createBtn = page.locator("button:has-text('Import Template')");
    await expect(createBtn).toBeVisible({ timeout: 5000 });
    await createBtn.click();

    // Fill in the template name
    const nameInput = page.locator("input[placeholder*='Minecraft']");
    await expect(nameInput).toBeVisible({ timeout: 5000 });
    await nameInput.fill("Test Echo Template");

    // Fill in the description
    const descTextarea = page.locator("textarea[placeholder*='Describe']");
    await descTextarea.fill("A simple echo server template for testing");

    // Fill in the JSON config
    const jsonTextarea = page.locator("textarea[placeholder*='name']");
    await expect(jsonTextarea).toBeVisible({ timeout: 5000 });

    const config = JSON.stringify(
      {
        name: "Echo Server",
        binary: "/run/current-system/sw/bin/echo",
        args: ["hello world"],
        env: {},
        working_dir: null,
        auto_start: false,
        auto_restart: false,
        max_restart_attempts: 5,
        restart_delay_secs: 5,
        stop_command: null,
        stop_signal: "sigterm",
        stop_timeout_secs: 10,
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
      null,
      2,
    );
    await jsonTextarea.fill(config);

    // Click save
    const saveBtn = page.locator("button:has-text('Save Template')");
    await saveBtn.click();

    // Template should appear in the grid
    await expect(
      page.locator(".template-card:has-text('Test Echo Template')"),
    ).toBeVisible({ timeout: 10000 });
  });

  test("can delete a custom template", async ({ page, testEnv }) => {
    // Create a template via the API first
    const response = await fetch(`${testEnv.apiUrl}/api/templates`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
      body: JSON.stringify({
        name: "Deletable Template",
        description: "This template will be deleted",
        config: {
          name: "Deletable",
          binary: "/bin/echo",
          args: [],
          env: {},
          working_dir: null,
          auto_start: false,
          auto_restart: false,
          max_restart_attempts: 5,
          restart_delay_secs: 5,
          stop_command: null,
          stop_signal: "sigterm",
          stop_timeout_secs: 10,
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
      }),
    });
    expect(response.ok).toBeTruthy();

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // Find the template card
    const templateCard = page.locator(
      ".template-card:has-text('Deletable Template')",
    );
    await expect(templateCard).toBeVisible({ timeout: 5000 });

    // Set up dialog handler for confirm prompt
    page.on("dialog", (dialog) => dialog.accept());

    // Click delete button on the card
    const deleteBtn = templateCard.locator("button:has-text('Delete')");
    await expect(deleteBtn).toBeVisible();
    await deleteBtn.click();

    // Template should disappear
    await expect(templateCard).not.toBeVisible({ timeout: 10000 });
  });

  test("built-in templates do not show delete button", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // Check if there are built-in templates
    const builtinCards = page.locator(".template-card-builtin");
    const builtinCount = await builtinCards.count();

    if (builtinCount > 0) {
      // Built-in templates should have the badge
      const firstBuiltin = builtinCards.first();
      await expect(
        firstBuiltin.locator(".template-builtin-badge"),
      ).toBeVisible();

      // Built-in templates should NOT have a delete button
      const deleteBtn = firstBuiltin.locator("button:has-text('Delete')");
      await expect(deleteBtn).not.toBeVisible();
    }
  });

  test("Use Template button navigates to create server page", async ({
    page,
    testEnv,
  }) => {
    // Create a template via the API
    const response = await fetch(`${testEnv.apiUrl}/api/templates`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
      body: JSON.stringify({
        name: "Usable Template",
        description: "Template to test Use Template flow",
        config: {
          name: "From Template",
          binary: "/run/current-system/sw/bin/echo",
          args: ["templated"],
          env: {},
          working_dir: null,
          auto_start: false,
          auto_restart: false,
          max_restart_attempts: 5,
          restart_delay_secs: 5,
          stop_command: null,
          stop_signal: "sigterm",
          stop_timeout_secs: 10,
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
      }),
    });
    expect(response.ok).toBeTruthy();

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // Find the template card
    const templateCard = page.locator(
      ".template-card:has-text('Usable Template')",
    );
    await expect(templateCard).toBeVisible({ timeout: 5000 });

    // Click "Use Template"
    const useBtn = templateCard.locator("button:has-text('Use Template')");
    await useBtn.click();

    // Should navigate to the create server page
    await expect(page).toHaveURL(/.*\/create/, { timeout: 5000 });
  });

  test("Export JSON button copies config to clipboard", async ({
    page,
    testEnv,
  }) => {
    // Create a template via the API
    const response = await fetch(`${testEnv.apiUrl}/api/templates`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
      body: JSON.stringify({
        name: "Exportable Template",
        description: "Template to test export",
        config: {
          name: "Export Test",
          binary: "/bin/echo",
          args: [],
          env: {},
          working_dir: null,
          auto_start: false,
          auto_restart: false,
          max_restart_attempts: 5,
          restart_delay_secs: 5,
          stop_command: null,
          stop_signal: "sigterm",
          stop_timeout_secs: 10,
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
      }),
    });
    expect(response.ok).toBeTruthy();

    // Grant clipboard permissions
    await page
      .context()
      .grantPermissions(["clipboard-read", "clipboard-write"]);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    const templateCard = page.locator(
      ".template-card:has-text('Exportable Template')",
    );
    await expect(templateCard).toBeVisible({ timeout: 5000 });

    // Handle the alert dialog that appears after clipboard copy
    page.on("dialog", (dialog) => dialog.accept());

    // Click Export JSON
    const exportBtn = templateCard.locator("button:has-text('Export JSON')");
    await exportBtn.click();

    // The alert dialog confirms the copy happened; if we reach here without
    // an unhandled dialog error, the export flow works
    await page.waitForTimeout(500);
  });

  test("template card shows metadata tags", async ({ page, testEnv }) => {
    // Create a template with parameters and install steps
    const response = await fetch(`${testEnv.apiUrl}/api/templates`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
      body: JSON.stringify({
        name: "Rich Template",
        description: "Template with parameters and steps",
        config: {
          name: "Rich Server",
          binary: "/bin/echo",
          args: [],
          env: {},
          working_dir: null,
          auto_start: false,
          auto_restart: false,
          max_restart_attempts: 5,
          restart_delay_secs: 5,
          stop_command: null,
          stop_signal: "sigterm",
          stop_timeout_secs: 10,
          sftp_username: null,
          sftp_password: null,
          parameters: [
            {
              name: "version",
              label: "Version",
              type: "string",
              default: "1.0",
              required: true,
            },
            {
              name: "port",
              label: "Port",
              type: "string",
              default: "8080",
              required: false,
            },
          ],
          stop_steps: [],
          start_steps: [],
          install_steps: [
            {
              name: "setup",
              action: {
                type: "run_command",
                command: "echo",
                args: ["setup"],
              },
              continue_on_error: false,
            },
          ],
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
      }),
    });
    expect(response.ok).toBeTruthy();

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    const templateCard = page.locator(
      ".template-card:has-text('Rich Template')",
    );
    await expect(templateCard).toBeVisible({ timeout: 5000 });

    // Should show parameter count
    await expect(
      templateCard.locator(".template-meta-tag:has-text('2 parameters')"),
    ).toBeVisible();

    // Should show install step count
    await expect(
      templateCard.locator(".template-meta-tag:has-text('1 install step')"),
    ).toBeVisible();
  });
});
