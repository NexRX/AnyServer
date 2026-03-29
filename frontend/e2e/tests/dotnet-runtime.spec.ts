/**
 * .NET Runtime Selector E2E Test Suite
 *
 * Tests the .NET runtime detection and selector UI:
 * - Detecting .NET runtimes on the system
 * - Selecting a runtime to auto-configure environment variables
 * - Integration with server creation/editing
 * - Clearing .NET environment variables
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

test.describe(".NET Runtime Selector", () => {
  test.afterEach(async ({ testEnv }) => {
    // Clean up servers after each test
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("detects .NET runtimes via API", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Call the .NET detection API
    const response = await fetch(
      `${testEnv.apiUrl}/api/system/dotnet-runtimes`,
      {
        headers: {
          Authorization: `Bearer ${testEnv.adminToken}`,
        },
      },
    );

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(
        `API call failed with status ${response.status}: ${errorText}`,
      );
    }
    const data = await response.json();

    // Should return a runtimes array (may be empty if .NET not installed)
    expect(data).toHaveProperty("runtimes");
    expect(Array.isArray(data.runtimes)).toBeTruthy();

    // If runtimes exist, verify their structure
    if (data.runtimes.length > 0) {
      const runtime = data.runtimes[0];
      expect(runtime).toHaveProperty("runtime_name");
      expect(runtime).toHaveProperty("version");
      expect(runtime).toHaveProperty("major_version");
      expect(runtime).toHaveProperty("installation_root");
      expect(runtime).toHaveProperty("is_default");
    }
  });

  test("shows .NET selector for .NET binaries in wizard", async ({
    page,
    testEnv,
  }) => {
    // Log in
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to create server page
    await page.goto(`${testEnv.baseUrl}/create`);
    await page.waitForLoadState("networkidle");

    // Switch to wizard mode
    const wizardBtn = page.locator("button:has-text('Wizard')");
    if (await wizardBtn.isVisible()) {
      await wizardBtn.click();
      await page.waitForTimeout(500);
    }

    // Navigate to the "Start" step (where binary is configured)
    // Click through steps until we get to the binary input
    let maxClicks = 5;
    while (maxClicks-- > 0) {
      const nextBtn = page.locator("button:has-text('Next')");
      if (await nextBtn.isVisible()) {
        await nextBtn.click();
        await page.waitForTimeout(300);
        // Check if we're on the start step by looking for binary input
        const binaryInput = page.locator('input[placeholder*="binary"]');
        if (await binaryInput.isVisible()) {
          break;
        }
      } else {
        break;
      }
    }

    // Enter a .NET binary name
    const binaryInput = page
      .locator('input[id="wiz-binary"], input[placeholder*="binary"]')
      .first();
    await binaryInput.fill("TShock.Server");

    // Wait a moment for detection logic to trigger
    await page.waitForTimeout(500);

    // Look for the .NET runtime detector button
    const detectBtn = page.locator("button:has-text('Detect .NET Runtimes')");
    await expect(detectBtn).toBeVisible({ timeout: 5000 });
  });

  test("shows .NET selector for .NET binaries in server edit", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server with a .NET binary
    const config = createMinimalServerConfig("test-dotnet-server");
    config.config.binary = "TShock.Server";
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Open the pipeline/config section
    const pipelineTab = page.locator("button:has-text('Pipeline')");
    if (await pipelineTab.isVisible()) {
      await pipelineTab.click();
      await page.waitForTimeout(500);
    }

    // Look for the .NET runtime detector button
    const detectBtn = page.locator("button:has-text('Detect .NET Runtimes')");
    await expect(detectBtn).toBeVisible({ timeout: 5000 });
  });

  test("selecting a runtime adds environment variables", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // First check if any .NET runtimes are available
    const runtimesResp = await fetch(
      `${testEnv.apiUrl}/api/system/dotnet-runtimes`,
      {
        headers: { Authorization: `Bearer ${testEnv.adminToken}` },
      },
    );
    const runtimesData = await runtimesResp.json();

    // Skip test if no .NET runtimes found
    test.skip(
      runtimesData.runtimes.length === 0,
      "No .NET runtimes installed on test system",
    );

    // Create a server with a .NET binary
    const config = createMinimalServerConfig("test-dotnet-env");
    config.config.binary = "./TShock.Server";
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Open the pipeline/config section
    const pipelineTab = page.locator("button:has-text('Pipeline')");
    if (await pipelineTab.isVisible()) {
      await pipelineTab.click();
      await page.waitForTimeout(500);
    }

    // Click detect button
    const detectBtn = page.locator("button:has-text('Detect .NET Runtimes')");
    await detectBtn.click();
    await page.waitForTimeout(1000);

    // Should show runtimes
    const runtimeList = page.locator("button:has-text('.NET')").first();
    await expect(runtimeList).toBeVisible({ timeout: 5000 });

    // Click on the first runtime
    await runtimeList.click();
    await page.waitForTimeout(500);

    // Environment variables textarea should now contain DOTNET_ROOT
    const envTextarea = page
      .locator("textarea[placeholder*='JAVA_HOME'], textarea")
      .filter({ hasText: /DOTNET_ROOT|Environment/ })
      .first();

    // Look for the applied .NET env row specifically (avoid broad text matches)
    const envInfo = page.locator("div", { hasText: "DOTNET_ROOT=" }).first();
    await expect(envInfo).toBeVisible({ timeout: 5000 });

    // Save the configuration
    const saveBtn = page.locator("button:has-text('Save Pipeline')");
    if (await saveBtn.isVisible()) {
      await saveBtn.click();
      await page.waitForTimeout(1000);
    }

    // Verify environment variables were saved via API
    const updatedServer = await getServer(client, server.server.id);
    expect(updatedServer.server.config.env).toHaveProperty("DOTNET_ROOT");
    expect(updatedServer.server.config.env).toHaveProperty(
      "DOTNET_BUNDLE_EXTRACT_BASE_DIR",
    );
  });

  test("clear button removes .NET environment variables", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server with .NET env vars already set
    const config = createMinimalServerConfig("test-dotnet-clear");
    config.config.binary = "./TShock.Server";
    config.config.env = {
      DOTNET_ROOT: "/usr/share/dotnet",
      DOTNET_BUNDLE_EXTRACT_BASE_DIR: "./.dotnet_bundle_cache",
      OTHER_VAR: "keep-me",
    };
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Open the pipeline/config section
    const pipelineTab = page.locator("button:has-text('Pipeline')");
    if (await pipelineTab.isVisible()) {
      await pipelineTab.click();
      await page.waitForTimeout(500);
    }

    // Click detect to expand the selector
    const detectBtn = page.locator(
      "button:has-text('Detect .NET Runtimes'), button:has-text('Show .NET Runtimes')",
    );
    await detectBtn.click();
    await page.waitForTimeout(500);

    // Click the .NET clear button specifically (avoid strict-mode collisions)
    const clearBtn = page.getByRole("button", { name: "✕ Clear" });
    await expect(clearBtn).toBeVisible({ timeout: 5000 });
    await clearBtn.click();
    await page.waitForTimeout(500);

    // Save the configuration
    const saveBtn = page.locator("button:has-text('Save Pipeline')");
    if (await saveBtn.isVisible()) {
      await saveBtn.click();
      await page.waitForTimeout(1000);
    }

    // Verify .NET env vars were removed but others remain
    const updatedServer = await getServer(client, server.server.id);
    expect(updatedServer.server.config.env).not.toHaveProperty("DOTNET_ROOT");
    expect(updatedServer.server.config.env).not.toHaveProperty(
      "DOTNET_BUNDLE_EXTRACT_BASE_DIR",
    );
    expect(updatedServer.server.config.env).toHaveProperty("OTHER_VAR");
    expect(updatedServer.server.config.env.OTHER_VAR).toBe("keep-me");
  });

  test("generates correct env vars via API", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Get available runtimes
    const runtimesResp = await fetch(
      `${testEnv.apiUrl}/api/system/dotnet-runtimes`,
      {
        headers: { Authorization: `Bearer ${testEnv.adminToken}` },
      },
    );
    const runtimesData = await runtimesResp.json();

    test.skip(
      runtimesData.runtimes.length === 0,
      "No .NET runtimes installed on test system",
    );

    const runtime = runtimesData.runtimes[0];

    // Call env generation API
    const envResp = await fetch(
      `${testEnv.apiUrl}/api/system/dotnet-env?installation_root=${encodeURIComponent(runtime.installation_root)}&server_dir=/test/server`,
      {
        headers: { Authorization: `Bearer ${testEnv.adminToken}` },
      },
    );

    expect(envResp.ok).toBeTruthy();
    const envVars = await envResp.json();

    // Should return DOTNET_ROOT and DOTNET_BUNDLE_EXTRACT_BASE_DIR
    expect(envVars).toHaveProperty("DOTNET_ROOT");
    expect(envVars).toHaveProperty("DOTNET_BUNDLE_EXTRACT_BASE_DIR");
    expect(envVars.DOTNET_ROOT).toBe(runtime.installation_root);
    expect(envVars.DOTNET_BUNDLE_EXTRACT_BASE_DIR).toContain(
      ".dotnet_bundle_cache",
    );
  });

  test("does not show .NET selector for non-.NET binaries", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server with a Java binary
    const config = createMinimalServerConfig("test-java-server");
    config.config.binary = "java";
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Open the pipeline/config section
    const pipelineTab = page.locator("button:has-text('Pipeline')");
    if (await pipelineTab.isVisible()) {
      await pipelineTab.click();
      await page.waitForTimeout(500);
    }

    // .NET detector should NOT be visible
    const dotnetDetectBtn = page.locator(
      "button:has-text('Detect .NET Runtimes')",
    );
    await expect(dotnetDetectBtn).not.toBeVisible();

    // But Java detector should be visible
    const javaDetectBtn = page.locator(
      "button:has-text('Detect Java Runtimes')",
    );
    await expect(javaDetectBtn).toBeVisible({ timeout: 5000 });
  });

  test("shows multiple runtime versions grouped by installation", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Check if multiple runtimes are available
    const runtimesResp = await fetch(
      `${testEnv.apiUrl}/api/system/dotnet-runtimes`,
      {
        headers: { Authorization: `Bearer ${testEnv.adminToken}` },
      },
    );
    const runtimesData = await runtimesResp.json();

    test.skip(
      runtimesData.runtimes.length < 2,
      "Test requires multiple .NET runtimes installed",
    );

    // Create a server
    const config = createMinimalServerConfig("test-dotnet-multi");
    config.config.binary = "dotnet";
    const server = await createServer(client, config);

    // Log in and navigate to server detail page
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Open the pipeline/config section
    const pipelineTab = page.locator("button:has-text('Pipeline')");
    if (await pipelineTab.isVisible()) {
      await pipelineTab.click();
      await page.waitForTimeout(500);
    }

    // Click detect button
    const detectBtn = page.locator("button:has-text('Detect .NET Runtimes')");
    await detectBtn.click();
    await page.waitForTimeout(1000);

    // Should show count of detected runtimes
    const runtimeCount = page.locator(`text=/\\d+ runtimes? found/`);
    await expect(runtimeCount).toBeVisible({ timeout: 5000 });

    // Should show runtime buttons
    const runtimeButtons = page.locator("button:has-text('.NET')");
    const count = await runtimeButtons.count();
    expect(count).toBeGreaterThan(0);
  });
});
