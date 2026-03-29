import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  cleanupAllServers,
  createMinimalServerConfig,
  createServer,
} from "../helpers/api";
import type { ApiClient } from "../helpers/api";

// ─── Helper: toggle sandbox feature via API ───

async function toggleSandboxFeature(
  client: ApiClient,
  enabled: boolean,
): Promise<void> {
  const response = await fetch(
    `${client.baseUrl}/api/admin/sandbox/feature`,
    {
      method: "PUT",
      headers: {
        Authorization: `Bearer ${client.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ enabled }),
    },
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(
      `Failed to toggle sandbox feature: ${response.status} ${text}`,
    );
  }
}

async function getSandboxCapabilities(
  client: ApiClient,
): Promise<{ feature_enabled: boolean; landlock_available: boolean; namespaces_available: boolean }> {
  const response = await fetch(
    `${client.baseUrl}/api/admin/sandbox/capabilities`,
    {
      method: "GET",
      headers: {
        Authorization: `Bearer ${client.token}`,
      },
    },
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(
      `Failed to get sandbox capabilities: ${response.status} ${text}`,
    );
  }

  return response.json();
}

async function getSandboxProfile(
  client: ApiClient,
  serverId: string,
): Promise<{ profile: Record<string, unknown>; capabilities: Record<string, unknown> }> {
  const response = await fetch(
    `${client.baseUrl}/api/servers/${serverId}/sandbox`,
    {
      method: "GET",
      headers: {
        Authorization: `Bearer ${client.token}`,
      },
    },
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(
      `Failed to get sandbox profile: ${response.status} ${text}`,
    );
  }

  return response.json();
}

async function updateSandboxProfile(
  client: ApiClient,
  serverId: string,
  profile: Record<string, unknown>,
): Promise<{ profile: Record<string, unknown>; capabilities: Record<string, unknown> }> {
  const response = await fetch(
    `${client.baseUrl}/api/servers/${serverId}/sandbox`,
    {
      method: "PUT",
      headers: {
        Authorization: `Bearer ${client.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(profile),
    },
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(
      `Failed to update sandbox profile: ${response.status} ${text}`,
    );
  }

  return response.json();
}

async function resetSandboxProfile(
  client: ApiClient,
  serverId: string,
): Promise<void> {
  const response = await fetch(
    `${client.baseUrl}/api/servers/${serverId}/sandbox`,
    {
      method: "DELETE",
      headers: {
        Authorization: `Bearer ${client.token}`,
      },
    },
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(
      `Failed to reset sandbox profile: ${response.status} ${text}`,
    );
  }
}

test.describe("Sandbox Configuration", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);

    // Disable sandbox feature after each test to clean up
    try {
      await toggleSandboxFeature(client, false);
    } catch {
      // Ignore cleanup errors
    }
  });

  // ─── Admin Panel: Sandbox Feature Tab ───

  test("sandbox tab is visible in admin panel", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await expect(sandboxTab).toBeVisible({ timeout: 5000 });
  });

  test("sandbox tab shows capabilities and feature toggle", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await sandboxTab.click();

    // Should show the heading
    await expect(
      page.locator("h2:has-text('Sandbox Management')"),
    ).toBeVisible({ timeout: 5000 });

    // Should show the feature flag status (defaults to disabled)
    await expect(page.locator("text=Disabled").first()).toBeVisible({
      timeout: 5000,
    });

    // Should show an Enable button
    const enableBtn = page.locator("button:has-text('Enable')");
    await expect(enableBtn).toBeVisible({ timeout: 5000 });

    // Should show host capabilities section
    await expect(
      page.locator("text=Host Capabilities").first(),
    ).toBeVisible({ timeout: 5000 });
  });

  test("can enable sandbox feature via admin panel", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await sandboxTab.click();

    // Wait for content to load
    await expect(
      page.locator("h2:has-text('Sandbox Management')"),
    ).toBeVisible({ timeout: 5000 });

    // Accept the confirm dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Click Enable
    const enableBtn = page.locator("button:has-text('Enable')");
    await expect(enableBtn).toBeVisible({ timeout: 5000 });
    await enableBtn.click();

    // Should show success message
    await expect(
      page.locator("text=Sandbox management enabled"),
    ).toBeVisible({ timeout: 10000 });

    // Should now show Enabled status
    await expect(page.locator("text=Enabled").first()).toBeVisible({
      timeout: 5000,
    });

    // Should show a Disable button
    await expect(
      page.locator("button:has-text('Disable')"),
    ).toBeVisible({ timeout: 5000 });
  });

  test("can disable sandbox feature via admin panel", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable first via API
    await toggleSandboxFeature(client, true);

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await sandboxTab.click();

    await expect(
      page.locator("h2:has-text('Sandbox Management')"),
    ).toBeVisible({ timeout: 5000 });

    // Accept the confirm dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Click Disable
    const disableBtn = page.locator("button:has-text('Disable')");
    await expect(disableBtn).toBeVisible({ timeout: 5000 });
    await disableBtn.click();

    // Should show success message
    await expect(
      page.locator("text=Sandbox management disabled"),
    ).toBeVisible({ timeout: 10000 });
  });

  // ─── API: Sandbox Feature Toggle ───

  test("API: can toggle sandbox feature on and off", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Initially disabled
    let caps = await getSandboxCapabilities(client);
    expect(caps.feature_enabled).toBe(false);

    // Enable it
    await toggleSandboxFeature(client, true);
    caps = await getSandboxCapabilities(client);
    expect(caps.feature_enabled).toBe(true);

    // Disable it
    await toggleSandboxFeature(client, false);
    caps = await getSandboxCapabilities(client);
    expect(caps.feature_enabled).toBe(false);
  });

  test("API: capabilities endpoint returns sandbox availability info", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const caps = await getSandboxCapabilities(client);

    // These fields must always be present
    expect(typeof caps.feature_enabled).toBe("boolean");
    expect(typeof caps.landlock_available).toBe("boolean");
    expect(typeof caps.namespaces_available).toBe("boolean");
  });

  // ─── API: Per-Server Sandbox Profile ───

  test("API: get default sandbox profile for a server", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Default Test"),
    );

    const result = await getSandboxProfile(client, server.server.id);

    expect(result.profile).toBeDefined();
    expect(result.capabilities).toBeDefined();

    // Default profile values
    const profile = result.profile as any;
    expect(profile.enabled).toBe(true);
    expect(profile.landlock_enabled).toBe(true);
    expect(profile.no_new_privs).toBe(true);
    expect(profile.fd_cleanup).toBe(true);
    expect(profile.non_dumpable).toBe(true);
    expect(profile.namespace_isolation).toBe(true);
    expect(profile.pids_max).toBe(0);
    expect(profile.extra_read_paths).toEqual([]);
    expect(profile.extra_rw_paths).toEqual([]);
    expect(profile.network_isolation).toBe(false);
    expect(profile.seccomp_mode).toBe("off");
  });

  test("API: update sandbox profile requires feature to be enabled", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Gated Test"),
    );

    // Feature is disabled by default, so updating should fail
    const response = await fetch(
      `${testEnv.apiUrl}/api/servers/${server.server.id}/sandbox`,
      {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${client.token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          enabled: false,
          landlock_enabled: false,
          no_new_privs: false,
          fd_cleanup: false,
          non_dumpable: false,
          namespace_isolation: false,
          pids_max: 0,
          extra_read_paths: [],
          extra_rw_paths: [],
          network_isolation: false,
          seccomp_mode: "off",
        }),
      },
    );

    expect(response.status).toBe(403);
    const body = await response.json();
    expect(body.error).toContain("not enabled");
  });

  test("API: update sandbox profile when feature is enabled", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable the feature
    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Update Test"),
    );

    const result = await updateSandboxProfile(client, server.server.id, {
      enabled: true,
      landlock_enabled: false,
      no_new_privs: true,
      fd_cleanup: true,
      non_dumpable: false,
      namespace_isolation: false,
      pids_max: 1024,
      extra_read_paths: ["/opt/custom-runtime"],
      extra_rw_paths: ["/tmp/server-scratch"],
      network_isolation: false,
      seccomp_mode: "off",
    });

    const profile = result.profile as any;
    expect(profile.enabled).toBe(true);
    expect(profile.landlock_enabled).toBe(false);
    expect(profile.no_new_privs).toBe(true);
    expect(profile.fd_cleanup).toBe(true);
    expect(profile.non_dumpable).toBe(false);
    expect(profile.namespace_isolation).toBe(false);
    expect(profile.pids_max).toBe(1024);
    expect(profile.extra_read_paths).toEqual(["/opt/custom-runtime"]);
    expect(profile.extra_rw_paths).toEqual(["/tmp/server-scratch"]);
    expect(profile.network_isolation).toBe(false);
    expect(profile.seccomp_mode).toBe("off");
  });

  test("API: reset sandbox profile to defaults", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable the feature
    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Reset Test"),
    );

    // Customize the profile
    await updateSandboxProfile(client, server.server.id, {
      enabled: false,
      landlock_enabled: false,
      no_new_privs: false,
      fd_cleanup: false,
      non_dumpable: false,
      namespace_isolation: false,
      pids_max: 512,
      extra_read_paths: ["/custom/path"],
      extra_rw_paths: ["/custom/rw"],
      network_isolation: true,
      seccomp_mode: "basic",
    });

    // Verify it was customized
    let result = await getSandboxProfile(client, server.server.id);
    expect((result.profile as any).enabled).toBe(false);
    expect((result.profile as any).pids_max).toBe(512);

    // Reset it
    await resetSandboxProfile(client, server.server.id);

    // Verify it's back to defaults
    result = await getSandboxProfile(client, server.server.id);
    const profile = result.profile as any;
    expect(profile.enabled).toBe(true);
    expect(profile.landlock_enabled).toBe(true);
    expect(profile.pids_max).toBe(0);
    expect(profile.extra_read_paths).toEqual([]);
    expect(profile.extra_rw_paths).toEqual([]);
    expect(profile.seccomp_mode).toBe("off");
  });

  test("API: validates seccomp_mode values", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Validation Test"),
    );

    // Invalid seccomp mode should be rejected
    const response = await fetch(
      `${testEnv.apiUrl}/api/servers/${server.server.id}/sandbox`,
      {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${client.token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          enabled: true,
          landlock_enabled: true,
          no_new_privs: true,
          fd_cleanup: true,
          non_dumpable: true,
          namespace_isolation: true,
          pids_max: 0,
          extra_read_paths: [],
          extra_rw_paths: [],
          network_isolation: false,
          seccomp_mode: "invalid_mode",
        }),
      },
    );

    expect(response.status).toBe(400);
    const body = await response.json();
    expect(body.error).toContain("seccomp_mode");
  });

  test("API: validates extra paths must be absolute", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Path Validation Test"),
    );

    // Relative path should be rejected
    const response = await fetch(
      `${testEnv.apiUrl}/api/servers/${server.server.id}/sandbox`,
      {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${client.token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          enabled: true,
          landlock_enabled: true,
          no_new_privs: true,
          fd_cleanup: true,
          non_dumpable: true,
          namespace_isolation: true,
          pids_max: 0,
          extra_read_paths: ["relative/path"],
          extra_rw_paths: [],
          network_isolation: false,
          seccomp_mode: "off",
        }),
      },
    );

    expect(response.status).toBe(400);
    const body = await response.json();
    expect(body.error).toContain("absolute path");
  });

  // ─── Frontend: Per-Server Sandbox Tab ───

  test("server detail page has sandbox tab", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Tab Server"),
    );

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Should see the sandbox tab
    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await expect(sandboxTab).toBeVisible({ timeout: 10000 });
  });

  test("sandbox tab shows feature disabled warning when feature is off", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Warning Server"),
    );

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click the sandbox tab
    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await sandboxTab.click();

    // Should show a warning that the feature is disabled
    await expect(
      page.locator("text=Sandbox management is disabled site-wide"),
    ).toBeVisible({ timeout: 10000 });
  });

  test("sandbox tab shows toggle controls when feature is enabled", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable the sandbox feature
    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Toggles Server"),
    );

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click the sandbox tab
    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await sandboxTab.click();

    // Should show the security sandbox heading
    await expect(
      page.locator("text=Security Sandbox").first(),
    ).toBeVisible({ timeout: 10000 });

    // Should show individual toggle labels
    await expect(
      page.locator("text=Isolation Enabled").first(),
    ).toBeVisible({ timeout: 5000 });
    await expect(
      page.locator("text=Landlock Filesystem").first(),
    ).toBeVisible({ timeout: 5000 });
    await expect(
      page.locator("text=NO_NEW_PRIVS").first(),
    ).toBeVisible({ timeout: 5000 });
    await expect(
      page.locator("text=FD Cleanup").first(),
    ).toBeVisible({ timeout: 5000 });
    await expect(
      page.locator("text=Non-Dumpable").first(),
    ).toBeVisible({ timeout: 5000 });

    // Should show Save Changes button
    await expect(
      page.locator("button:has-text('Save')").first(),
    ).toBeVisible({ timeout: 5000 });

    // Should show Reset to Defaults button
    await expect(
      page.locator("button:has-text('Reset')").first(),
    ).toBeVisible({ timeout: 5000 });
  });

  test("sandbox tab save button persists changes", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable the sandbox feature
    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Save Server"),
    );

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click the sandbox tab
    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await sandboxTab.click();

    await expect(
      page.locator("text=Security Sandbox").first(),
    ).toBeVisible({ timeout: 10000 });

    // Click the save button (saving defaults is fine — just verifies the round-trip)
    const saveBtn = page.locator("button:has-text('Save Sandbox Profile')");
    await expect(saveBtn).toBeVisible({ timeout: 5000 });
    await saveBtn.click();

    // Should show a success message
    await expect(
      page.locator("text=Sandbox profile saved"),
    ).toBeVisible({ timeout: 10000 });
  });

  test("sandbox tab reset button works", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable the sandbox feature
    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Reset UI Server"),
    );

    // Customize the profile first via API
    await updateSandboxProfile(client, server.server.id, {
      enabled: true,
      landlock_enabled: false,
      no_new_privs: true,
      fd_cleanup: true,
      non_dumpable: true,
      namespace_isolation: true,
      pids_max: 256,
      extra_read_paths: [],
      extra_rw_paths: [],
      network_isolation: false,
      seccomp_mode: "off",
    });

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/server/${server.server.id}`);
    await page.waitForLoadState("networkidle");

    // Click the sandbox tab
    const sandboxTab = page.locator("button.tab:has-text('Sandbox')");
    await sandboxTab.click();

    await expect(
      page.locator("text=Security Sandbox").first(),
    ).toBeVisible({ timeout: 10000 });

    // Accept the confirm dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Click the reset button
    const resetBtn = page
      .locator("button:has-text('Reset to Defaults')")
      .first();
    await expect(resetBtn).toBeVisible({ timeout: 5000 });
    await resetBtn.click();

    // Should show a success message
    await expect(
      page.locator("text=Sandbox profile reset to defaults"),
    ).toBeVisible({ timeout: 10000 });

    // Verify via API that it was reset
    const result = await getSandboxProfile(client, server.server.id);
    const profile = result.profile as any;
    expect(profile.landlock_enabled).toBe(true);
    expect(profile.pids_max).toBe(0);
  });

  // ─── Non-admin access ───

  test("API: non-admin cannot toggle sandbox feature", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Enable registration and create a non-admin user
    const meResponse = await fetch(`${testEnv.apiUrl}/api/auth/me`, {
      headers: { Authorization: `Bearer ${client.token}` },
    });
    const meData = await meResponse.json();

    await fetch(`${testEnv.apiUrl}/api/auth/settings`, {
      method: "PUT",
      headers: {
        Authorization: `Bearer ${client.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        registration_enabled: true,
        allow_run_commands: meData.settings.allow_run_commands,
        run_command_sandbox: meData.settings.run_command_sandbox,
        run_command_default_timeout_secs:
          meData.settings.run_command_default_timeout_secs,
        run_command_use_namespaces: meData.settings.run_command_use_namespaces,
      }),
    });

    // Register a normal user
    const registerResponse = await fetch(
      `${testEnv.apiUrl}/api/auth/register`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: "sandboxuser",
          password: "SandboxUser1",
        }),
      },
    );
    const registerData = await registerResponse.json();

    // Attempt to toggle sandbox feature as non-admin
    const toggleResponse = await fetch(
      `${testEnv.apiUrl}/api/admin/sandbox/feature`,
      {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${registerData.token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ enabled: true }),
      },
    );

    expect(toggleResponse.status).toBe(403);

    // Disable registration again
    await fetch(`${testEnv.apiUrl}/api/auth/settings`, {
      method: "PUT",
      headers: {
        Authorization: `Bearer ${client.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        registration_enabled: false,
        allow_run_commands: meData.settings.allow_run_commands,
        run_command_sandbox: meData.settings.run_command_sandbox,
        run_command_default_timeout_secs:
          meData.settings.run_command_default_timeout_secs,
        run_command_use_namespaces: meData.settings.run_command_use_namespaces,
      }),
    });
  });

  test("sandbox profile is deleted when server is deleted", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    await toggleSandboxFeature(client, true);

    const server = await createServer(
      client,
      createMinimalServerConfig("Sandbox Cascade Delete"),
    );

    // Set a custom sandbox profile
    await updateSandboxProfile(client, server.server.id, {
      enabled: false,
      landlock_enabled: false,
      no_new_privs: false,
      fd_cleanup: false,
      non_dumpable: false,
      namespace_isolation: false,
      pids_max: 100,
      extra_read_paths: [],
      extra_rw_paths: [],
      network_isolation: false,
      seccomp_mode: "off",
    });

    // Delete the server
    await fetch(`${testEnv.apiUrl}/api/servers/${server.server.id}`, {
      method: "DELETE",
      headers: { Authorization: `Bearer ${client.token}` },
    });

    // Trying to get the sandbox profile should now 404
    const response = await fetch(
      `${testEnv.apiUrl}/api/servers/${server.server.id}/sandbox`,
      {
        method: "GET",
        headers: { Authorization: `Bearer ${client.token}` },
      },
    );

    expect(response.status).toBe(404);
  });
});
