/**
 * Runtime Helper Toggles E2E Test Suite
 *
 * Tests the manual Java and .NET runtime helper toggles:
 * - Toggles are visible and functional in the wizard
 * - Enabling toggles shows runtime selectors even for non-detected binaries
 * - Toggles persist when creating templates
 * - Toggles work correctly when using templates
 * - Both helpers can be enabled simultaneously
 * - Disabling toggles hides selectors when binary doesn't match auto-detection
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  cleanupAllServers,
  getServer,
} from "../helpers/api";

test.describe("Runtime Helper Toggles", () => {
  test.afterEach(async ({ testEnv }) => {
    // Clean up servers and templates after each test
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("toggles are visible in wizard start step", async ({
    page,
    testEnv,
  }) => {
    // Log in
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to create server page
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    // Switch to wizard mode if not already
    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Navigate to the "Start Command" step
    let maxClicks = 10;
    while (maxClicks-- > 0) {
      const heading = page.locator("h3:has-text('Start Command')");
      if (await heading.isVisible()) {
        break;
      }
      const nextBtn = page.locator("button:has-text('Next')");
      if (await nextBtn.isVisible()) {
        await nextBtn.click();
        await page.waitForTimeout(300);
      } else {
        break;
      }
    }

    // Verify we're on the start step
    const binaryInput = page.locator(
      'input[id="wiz-binary"], input[placeholder*="binary"]',
    );
    await expect(binaryInput).toBeVisible({ timeout: 5000 });

    // Scroll to the bottom to see the runtime helper section
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);

    // Look for the Runtime Helpers heading
    const helpersHeading = page.locator("h4:has-text('Runtime Helpers')");
    await expect(helpersHeading).toBeVisible({ timeout: 5000 });

    // Look for both toggle checkboxes
    const javaToggle = page.locator(
      "label:has-text('Enable Java Runtime Helper')",
    );
    const dotnetToggle = page.locator(
      "label:has-text('Enable .NET Runtime Helper')",
    );

    await expect(javaToggle).toBeVisible();
    await expect(dotnetToggle).toBeVisible();

    // Verify they are unchecked by default
    const javaCheckbox = javaToggle.locator("input[type='checkbox']");
    const dotnetCheckbox = dotnetToggle.locator("input[type='checkbox']");

    await expect(javaCheckbox).not.toBeChecked();
    await expect(dotnetCheckbox).not.toBeChecked();
  });

  test("enabling java toggle shows java selector for custom binary", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Navigate to start step
    let maxClicks = 10;
    while (maxClicks-- > 0) {
      const binaryInput = page.locator(
        'input[id="wiz-binary"], input[placeholder*="binary"]',
      );
      if (await binaryInput.isVisible()) {
        break;
      }
      const nextBtn = page.locator("button:has-text('Next')");
      if (await nextBtn.isVisible()) {
        await nextBtn.click();
        await page.waitForTimeout(300);
      } else {
        break;
      }
    }

    // Enter a custom binary that doesn't match Java auto-detection
    const binaryInput = page
      .locator('input[id="wiz-binary"], input[placeholder*="binary"]')
      .first();
    await binaryInput.fill("./custom-launcher.sh");
    await page.waitForTimeout(500);

    // Java selector should NOT be visible initially
    const javaDetectBtn = page.locator(
      "button:has-text('Detect Java Runtimes')",
    );
    await expect(javaDetectBtn).not.toBeVisible();

    // Scroll to runtime helpers section and enable Java toggle
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);

    const javaToggle = page
      .locator("label:has-text('Enable Java Runtime Helper')")
      .locator("input[type='checkbox']");
    await javaToggle.check();
    await page.waitForTimeout(500);

    // Now scroll back up and verify Java selector appeared
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(300);

    // Java selector should now be visible
    await expect(javaDetectBtn).toBeVisible({ timeout: 5000 });

    // Verify the tip box is shown
    const tipBox = page.locator(".wizard-step-content div").filter({
      hasText: "💡 Tip:",
    });
    await expect(tipBox.first()).toBeVisible();
  });

  test("enabling dotnet toggle shows dotnet selector for custom binary", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Navigate to start step
    let maxClicks = 10;
    while (maxClicks-- > 0) {
      const binaryInput = page.locator(
        'input[id="wiz-binary"], input[placeholder*="binary"]',
      );
      if (await binaryInput.isVisible()) {
        break;
      }
      const nextBtn = page.locator("button:has-text('Next')");
      if (await nextBtn.isVisible()) {
        await nextBtn.click();
        await page.waitForTimeout(300);
      } else {
        break;
      }
    }

    // Enter a custom binary
    const binaryInput = page
      .locator('input[id="wiz-binary"], input[placeholder*="binary"]')
      .first();
    await binaryInput.fill("./MyCustomServer");
    await page.waitForTimeout(500);

    // .NET selector should NOT be visible initially
    const dotnetDetectBtn = page.locator(
      "button:has-text('Detect .NET Runtimes')",
    );
    await expect(dotnetDetectBtn).not.toBeVisible();

    // Enable .NET toggle
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);

    const dotnetToggle = page
      .locator("label:has-text('Enable .NET Runtime Helper')")
      .locator("input[type='checkbox']");
    await dotnetToggle.check();
    await page.waitForTimeout(500);

    // Scroll back up and verify .NET selector appeared
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(300);

    await expect(dotnetDetectBtn).toBeVisible({ timeout: 5000 });
  });

  test("both helpers can be enabled simultaneously", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Navigate to start step
    let maxClicks = 10;
    while (maxClicks-- > 0) {
      const binaryInput = page.locator(
        'input[id="wiz-binary"], input[placeholder*="binary"]',
      );
      if (await binaryInput.isVisible()) {
        break;
      }
      const nextBtn = page.locator("button:has-text('Next')");
      if (await nextBtn.isVisible()) {
        await nextBtn.click();
        await page.waitForTimeout(300);
      } else {
        break;
      }
    }

    const binaryInput = page
      .locator('input[id="wiz-binary"], input[placeholder*="binary"]')
      .first();
    await binaryInput.fill("./hybrid-server");
    await page.waitForTimeout(500);

    // Enable both toggles
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);

    const javaToggle = page
      .locator("label:has-text('Enable Java Runtime Helper')")
      .locator("input[type='checkbox']");
    const dotnetToggle = page
      .locator("label:has-text('Enable .NET Runtime Helper')")
      .locator("input[type='checkbox']");

    await javaToggle.check();
    await page.waitForTimeout(300);
    await dotnetToggle.check();
    await page.waitForTimeout(500);

    // Verify both are checked
    await expect(javaToggle).toBeChecked();
    await expect(dotnetToggle).toBeChecked();

    // Scroll up and verify both selectors are visible
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(300);

    const javaDetectBtn = page.locator(
      "button:has-text('Detect Java Runtimes')",
    );
    const dotnetDetectBtn = page.locator(
      "button:has-text('Detect .NET Runtimes')",
    );

    await expect(javaDetectBtn).toBeVisible({ timeout: 5000 });
    await expect(dotnetDetectBtn).toBeVisible({ timeout: 5000 });
  });

  test("toggle state persists when creating server", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Fill in basic info on first step (parameters step)
    const nextBtn = page.locator("button:has-text('Next')");
    await nextBtn.click();
    await page.waitForTimeout(300);

    // Server name step
    const nameInput = page.locator('input[id="wiz-name"]').first();
    await nameInput.fill("Test Helper Toggle Server");
    await nextBtn.click();
    await page.waitForTimeout(300);

    // Start step - configure binary and enable Java helper
    const binaryInput = page
      .locator('input[id="wiz-binary"], input[placeholder*="binary"]')
      .first();
    await binaryInput.fill("./my-custom-wrapper.sh");
    await page.waitForTimeout(300);

    // Enable Java helper toggle
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);

    const javaToggle = page
      .locator("label:has-text('Enable Java Runtime Helper')")
      .locator("input[type='checkbox']");
    await javaToggle.check();
    await page.waitForTimeout(500);

    // Continue through wizard to create the server
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(300);

    // Advance until review step shows the create action
    let guard = 6;
    let createBtn = page.locator("button:has-text('Create Server')");
    while (guard-- > 0 && !(await createBtn.isVisible().catch(() => false))) {
      if (!(await nextBtn.isVisible().catch(() => false))) break;
      await nextBtn.click();
      await page.waitForTimeout(300);
    }

    // On review step, click Create
    createBtn = page.locator("button:has-text('Create Server')");
    await expect(createBtn).toBeVisible({ timeout: 5000 });
    await createBtn.click();
    await page.waitForTimeout(2000);

    // Wait for redirect to dashboard or server page
    await page.waitForURL(/\/(dashboard|server)/, { timeout: 10000 });

    // Fetch the created server via API and verify the toggle is set
    const serversResp = await fetch(`${testEnv.apiUrl}/api/servers`, {
      headers: { Authorization: `Bearer ${testEnv.adminToken}` },
    });
    const serversData = await serversResp.json();

    const createdServer = serversData.servers.find(
      (s: any) => s.server.config.name === "Test Helper Toggle Server",
    );

    expect(createdServer).toBeDefined();
    expect(createdServer.server.config.enable_java_helper).toBe(true);
    expect(createdServer.server.config.enable_dotnet_helper).toBe(false);
  });

  test("toggle state persists in templates", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to templates page
    await page.goto(`${testEnv.baseUrl}/templates`);
    await page.waitForLoadState("networkidle");

    // Click "Import Template" button
    const newTemplateBtn = page.locator("button:has-text('Import Template')");
    await newTemplateBtn.click();
    await page.waitForTimeout(500);

    // Fill in template name
    const nameInput = page
      .locator(".template-create-card .form-group")
      .filter({ hasText: "Template Name" })
      .locator("input")
      .first();
    await expect(nameInput).toBeVisible({ timeout: 5000 });
    await nameInput.fill("Test Helper Template");

    // Create a minimal config JSON with helpers enabled
    const configJson = {
      name: "Helper Enabled Server",
      binary: "./custom-server",
      args: [],
      env: {},
      working_dir: null,
      auto_start: false,
      auto_restart: false,
      max_restart_attempts: 0,
      restart_delay_secs: 5,
      stop_command: null,
      stop_signal: "sigterm",
      stop_timeout_secs: 10,
      stop_steps: [],
      sftp_username: null,
      sftp_password: null,
      parameters: [],
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
      log_to_disk: true,
      max_log_size_mb: 50,
      enable_java_helper: true,
      enable_dotnet_helper: true,
    };

    // Paste JSON into the textarea
    const jsonTextarea = page.locator("textarea").first();
    await jsonTextarea.fill(JSON.stringify(configJson, null, 2));
    await page.waitForTimeout(500);

    // Click Create/Save button
    const createBtn = page.locator(
      "button:has-text('Create Template'), button:has-text('Save')",
    );
    await createBtn.click();
    await page.waitForTimeout(2000);

    // Verify template was created via API
    const templatesResp = await fetch(`${testEnv.apiUrl}/api/templates`, {
      headers: { Authorization: `Bearer ${testEnv.adminToken}` },
    });
    const templatesData = await templatesResp.json();

    const createdTemplate = templatesData.templates.find(
      (t: any) => t.name === "Test Helper Template",
    );

    expect(createdTemplate).toBeDefined();
    expect(createdTemplate.config.enable_java_helper).toBe(true);
    expect(createdTemplate.config.enable_dotnet_helper).toBe(true);
  });

  test("disabling toggle hides selector when binary doesn't match", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Navigate to start step
    let maxClicks = 10;
    while (maxClicks-- > 0) {
      const binaryInput = page.locator(
        'input[id="wiz-binary"], input[placeholder*="binary"]',
      );
      if (await binaryInput.isVisible()) {
        break;
      }
      const nextBtn = page.locator("button:has-text('Next')");
      if (await nextBtn.isVisible()) {
        await nextBtn.click();
        await page.waitForTimeout(300);
      } else {
        break;
      }
    }

    const binaryInput = page
      .locator('input[id="wiz-binary"], input[placeholder*="binary"]')
      .first();
    await binaryInput.fill("./custom-launcher");
    await page.waitForTimeout(500);

    // Enable then disable Java toggle
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);

    const javaToggle = page
      .locator("label:has-text('Enable Java Runtime Helper')")
      .locator("input[type='checkbox']");

    // Enable it
    await javaToggle.check();
    await page.waitForTimeout(500);

    // Verify selector appears
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(300);
    const javaDetectBtn = page.locator(
      "button:has-text('Detect Java Runtimes')",
    );
    await expect(javaDetectBtn).toBeVisible();

    // Now disable it
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);
    await javaToggle.uncheck();
    await page.waitForTimeout(500);

    // Verify selector disappears
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(300);
    await expect(javaDetectBtn).not.toBeVisible();
  });

  test("toggle remains off when binary matches auto-detection", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Navigate to start step
    let maxClicks = 10;
    while (maxClicks-- > 0) {
      const binaryInput = page.locator(
        'input[id="wiz-binary"], input[placeholder*="binary"]',
      );
      if (await binaryInput.isVisible()) {
        break;
      }
      const nextBtn = page.locator("button:has-text('Next')");
      if (await nextBtn.isVisible()) {
        await nextBtn.click();
        await page.waitForTimeout(300);
      } else {
        break;
      }
    }

    // Enter a Java binary that will auto-detect
    const binaryInput = page
      .locator('input[id="wiz-binary"], input[placeholder*="binary"]')
      .first();
    await binaryInput.fill("java");
    await page.waitForTimeout(500);

    // Java selector should be visible due to auto-detection
    const javaDetectBtn = page.locator(
      "button:has-text('Detect Java Runtimes')",
    );
    await expect(javaDetectBtn).toBeVisible({ timeout: 5000 });

    // Verify toggle is still unchecked (auto-detection doesn't change it)
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(300);

    const javaToggle = page
      .locator("label:has-text('Enable Java Runtime Helper')")
      .locator("input[type='checkbox']");
    await expect(javaToggle).not.toBeChecked();
  });

  test("helper toggles work in builtin templates", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to create server from template
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    // Make sure we're in template mode
    const templateBtn = page.locator("button:has-text('From Template')");
    if (await templateBtn.isVisible()) {
      await templateBtn.click();
      await page.waitForTimeout(500);
    }

    // Look for Minecraft Paper template (should have Java helper enabled)
    const minecraftCard = page
      .locator(".template-select-card:has-text('Minecraft Paper')")
      .first();
    if (await minecraftCard.isVisible()) {
      await minecraftCard.click();
      await page.waitForTimeout(1000);

      // Fill in required parameters if any
      const createBtn = page.locator(
        "button:has-text('Create Server'), button:has-text('Create')",
      );
      const inputs = page.locator("input[type='text']");
      const count = await inputs.count();

      // Fill in any empty required inputs
      for (let i = 0; i < count; i++) {
        const input = inputs.nth(i);
        const value = await input.inputValue();
        if (!value) {
          await input.fill("test-value");
        }
      }

      await createBtn.click();
      await page.waitForTimeout(2000);

      // Verify server was created
      await page.waitForURL(/\/(dashboard|server)/, { timeout: 10000 });

      // Fetch servers and verify Java helper is enabled
      const serversResp = await fetch(`${testEnv.apiUrl}/api/servers`, {
        headers: { Authorization: `Bearer ${testEnv.adminToken}` },
      });
      const serversData = await serversResp.json();

      const minecraftServer = serversData.servers.find((s: any) =>
        s.server.config.name.includes("Minecraft"),
      );

      if (minecraftServer) {
        expect(minecraftServer.server.config.enable_java_helper).toBe(true);
      }
    }
  });
});
