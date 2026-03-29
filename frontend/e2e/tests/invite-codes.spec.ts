import { test, expect } from "../fixtures/test-environment";
import { loginViaUI, loginViaToken, injectToken } from "../helpers/auth";
import {
  createApiClient,
  createNonAdminUser,
  cleanupAllServers,
  createMinimalServerConfig,
  createServer,
  enableRegistration,
} from "../helpers/api";
import type { ApiClient } from "../helpers/api";

// ─── Helper: create invite code via API ───

interface CreateInviteCodeOpts {
  expiry?: string;
  assigned_role?: string;
  assigned_permissions?: Array<{ server_id: string; level: string }>;
  label?: string | null;
}

async function createInviteCodeViaApi(
  client: ApiClient,
  opts: CreateInviteCodeOpts = {},
): Promise<{ id: string; code: string }> {
  const body = {
    expiry: opts.expiry ?? "one_day",
    assigned_role: opts.assigned_role ?? "user",
    assigned_permissions: opts.assigned_permissions ?? [],
    label: opts.label ?? null,
  };

  const response = await fetch(`${client.baseUrl}/api/admin/invite-codes`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${client.token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Failed to create invite code: ${response.status} ${text}`);
  }

  const data = await response.json();
  return { id: data.invite.id, code: data.invite.code };
}

async function listInviteCodesViaApi(
  client: ApiClient,
): Promise<
  Array<{
    id: string;
    code: string;
    is_active: boolean;
    redeemed_by: string | null;
  }>
> {
  const response = await fetch(`${client.baseUrl}/api/admin/invite-codes`, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${client.token}`,
    },
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Failed to list invite codes: ${response.status} ${text}`);
  }

  const data = await response.json();
  return data.invites;
}

async function deleteInviteCodeViaApi(
  client: ApiClient,
  id: string,
): Promise<void> {
  const response = await fetch(
    `${client.baseUrl}/api/admin/invite-codes/${id}`,
    {
      method: "DELETE",
      headers: {
        Authorization: `Bearer ${client.token}`,
      },
    },
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Failed to delete invite code: ${response.status} ${text}`);
  }
}

async function redeemInviteCodeViaApi(
  baseUrl: string,
  code: string,
  username: string,
  password: string,
): Promise<{
  token: string;
  user: { id: string; username: string; role: string };
}> {
  const response = await fetch(`${baseUrl}/api/auth/redeem-invite`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, username, password }),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Failed to redeem invite code: ${response.status} ${text}`);
  }

  return response.json();
}

test.describe("Invite Codes", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);

    // Clean up any invite codes
    try {
      const invites = await listInviteCodesViaApi(client);
      for (const invite of invites) {
        await deleteInviteCodeViaApi(client, invite.id);
      }
    } catch {
      // Ignore cleanup errors
    }
  });

  test("invite codes tab is visible in admin panel", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const inviteTab = page.locator("button.tab:has-text('Invite Codes')");
    await expect(inviteTab).toBeVisible({ timeout: 5000 });
  });

  test("can navigate to invite codes tab and see empty state", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const inviteTab = page.locator("button.tab:has-text('Invite Codes')");
    await inviteTab.click();

    await expect(page.locator("h2:has-text('Invite Codes')")).toBeVisible({
      timeout: 5000,
    });

    // Should show empty state or the "New Invite Code" button
    const newBtn = page.locator("button:has-text('New Invite Code')");
    await expect(newBtn).toBeVisible({ timeout: 5000 });
  });

  test("can create an invite code via the UI", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    // Navigate to invite codes tab
    const inviteTab = page.locator("button.tab:has-text('Invite Codes')");
    await inviteTab.click();

    // Click the "New Invite Code" button
    const newBtn = page.locator("button:has-text('New Invite Code')");
    await newBtn.click();

    // Fill out the form — label is optional
    const labelInput = page.locator("input[placeholder*='For John']");
    await labelInput.fill("E2E Test Invite");

    // Click generate
    const generateBtn = page.locator("button:has-text('Generate Invite Code')");
    await expect(generateBtn).toBeVisible({ timeout: 5000 });
    await generateBtn.click();

    // Should see a success message with the code
    await expect(page.locator("text=Invite code created")).toBeVisible({
      timeout: 10000,
    });

    // The invite code should now appear in the list
    await expect(page.locator("text=E2E Test Invite")).toBeVisible({
      timeout: 5000,
    });

    // Should show "Active" badge
    await expect(page.locator("text=Active").first()).toBeVisible({
      timeout: 5000,
    });
  });

  test("can delete an invite code via the UI", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create an invite code via API first
    await createInviteCodeViaApi(client, { label: "Delete Me" });

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    // Navigate to invite codes tab
    const inviteTab = page.locator("button.tab:has-text('Invite Codes')");
    await inviteTab.click();

    // Wait for the invite to appear
    await expect(page.locator("text=Delete Me")).toBeVisible({
      timeout: 5000,
    });

    // Accept the confirm dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Click the delete button
    const deleteBtn = page.locator("button:has-text('Delete')").first();
    await deleteBtn.click();

    // Should see success message
    await expect(page.locator("text=Invite code deleted")).toBeVisible({
      timeout: 10000,
    });
  });

  test("invite code can be redeemed via the login page", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create an invite code via API
    const invite = await createInviteCodeViaApi(client, {
      label: "Redeem Test",
    });

    // Navigate to login page (not logged in)
    await page.goto(`${testEnv.baseUrl}/login`);
    await page.waitForLoadState("networkidle");

    // Click "Have an invite code?" button
    const inviteBtn = page.locator("button:has-text('invite code')");
    await expect(inviteBtn).toBeVisible({ timeout: 5000 });
    await inviteBtn.click();

    // Fill in the invite code
    const codeInput = page.locator("input#invite-code");
    await expect(codeInput).toBeVisible({ timeout: 5000 });
    await codeInput.fill(invite.code);

    // Fill in username and password
    const usernameInput = page.locator("input#invite-username");
    await usernameInput.fill("inviteduser");

    const passwordInput = page.locator("input#invite-password");
    await passwordInput.fill("InvitedUser1");

    // Submit the redeem form
    const redeemBtn = page.locator("button:has-text('Redeem')");
    await redeemBtn.click();

    // Should redirect to dashboard after successful redemption
    await page.waitForURL(/.*\//, { timeout: 15000 });

    // Should see the navbar (indicating logged in)
    await expect(page.locator("nav.navbar")).toBeVisible({ timeout: 10000 });
  });

  test("redeemed invite code shows as redeemed in admin panel", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create and redeem an invite code via API
    const invite = await createInviteCodeViaApi(client, {
      label: "Already Redeemed",
    });
    await redeemInviteCodeViaApi(
      testEnv.apiUrl,
      invite.code,
      "redeemeduser",
      "RedeemedUser1",
    );

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const inviteTab = page.locator("button.tab:has-text('Invite Codes')");
    await inviteTab.click();

    // Wait for content to load
    await expect(page.locator("text=Already Redeemed")).toBeVisible({
      timeout: 5000,
    });

    // Should show "Redeemed" badge
    await expect(page.locator("text=Redeemed").first()).toBeVisible({
      timeout: 5000,
    });
  });

  test("invite code with admin role creates admin user", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create an invite code with admin role
    const invite = await createInviteCodeViaApi(client, {
      assigned_role: "admin",
      label: "Admin Invite",
    });

    // Redeem it
    const result = await redeemInviteCodeViaApi(
      testEnv.apiUrl,
      invite.code,
      "newadmin",
      "NewAdmin1Pass",
    );

    expect(result.user.role).toBe("admin");
    expect(result.user.username).toBe("newadmin");
  });

  test("invite code with server permissions grants access", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server first
    const server = await createServer(
      client,
      createMinimalServerConfig("Permission Test Server"),
    );

    // Create invite code with viewer permission on the server
    const invite = await createInviteCodeViaApi(client, {
      assigned_permissions: [{ server_id: server.server.id, level: "viewer" }],
      label: "Permission Invite",
    });

    // Redeem it
    const result = await redeemInviteCodeViaApi(
      testEnv.apiUrl,
      invite.code,
      "permuser",
      "PermUser1Pass",
    );

    // The new user should be able to get the server
    const userClient = createApiClient(testEnv.apiUrl, result.token);
    const response = await fetch(
      `${testEnv.apiUrl}/api/servers/${server.server.id}`,
      {
        headers: { Authorization: `Bearer ${result.token}` },
      },
    );
    expect(response.ok).toBe(true);
  });

  test("cannot redeem an already-used invite code", async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const invite = await createInviteCodeViaApi(client, { label: "One Time" });

    // Redeem it once
    await redeemInviteCodeViaApi(
      testEnv.apiUrl,
      invite.code,
      "firstuser",
      "FirstUser1Pass",
    );

    // Try to redeem it again
    try {
      await redeemInviteCodeViaApi(
        testEnv.apiUrl,
        invite.code,
        "seconduser",
        "SecondUser1Pass",
      );
      // Should not reach here
      expect(true).toBe(false);
    } catch (err: unknown) {
      expect((err as Error).message).toContain(
        "Invalid or expired invite code",
      );
    }
  });

  test("cannot redeem an invalid invite code", async ({ testEnv }) => {
    try {
      await redeemInviteCodeViaApi(
        testEnv.apiUrl,
        "999999",
        "fakeuser",
        "FakeUser1Pass",
      );
      expect(true).toBe(false);
    } catch (err: unknown) {
      expect((err as Error).message).toContain(
        "Invalid or expired invite code",
      );
    }
  });

  test("can update permissions on an active invite code", async ({
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const server = await createServer(
      client,
      createMinimalServerConfig("Update Perms Server"),
    );

    // Create an invite code with no permissions
    const invite = await createInviteCodeViaApi(client, { label: "Update Me" });

    // Update its permissions
    const response = await fetch(
      `${testEnv.apiUrl}/api/admin/invite-codes/${invite.id}/permissions`,
      {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${client.token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          assigned_role: "user",
          assigned_permissions: [
            { server_id: server.server.id, level: "operator" },
          ],
        }),
      },
    );

    expect(response.ok).toBe(true);

    const data = await response.json();
    expect(data.assigned_permissions.length).toBe(1);
    expect(data.assigned_permissions[0].level).toBe("operator");
  });

  test("permissions tab shows user permissions overview", async ({
    page,
    testEnv,
  }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const permTab = page.locator("button.tab:has-text('Permissions')");
    await expect(permTab).toBeVisible({ timeout: 5000 });
    await permTab.click();

    // Should show the permissions heading
    await expect(
      page.locator("h2:has-text('User Permissions Overview')"),
    ).toBeVisible({ timeout: 5000 });

    // Should show the admin user
    await expect(page.locator("text=admin").first()).toBeVisible({
      timeout: 5000,
    });
  });

  test("permissions tab shows server access for users", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const server = await createServer(
      client,
      createMinimalServerConfig("Perms Overview Server"),
    );

    // Create a user via invite
    const invite = await createInviteCodeViaApi(client, {
      assigned_permissions: [{ server_id: server.server.id, level: "manager" }],
    });
    await redeemInviteCodeViaApi(
      testEnv.apiUrl,
      invite.code,
      "permviewuser",
      "PermViewUser1",
    );

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const permTab = page.locator("button.tab:has-text('Permissions')");
    await permTab.click();

    // Wait for the content to load
    await expect(
      page.locator("h2:has-text('User Permissions Overview')"),
    ).toBeVisible({ timeout: 5000 });

    // The user should be listed
    await expect(page.locator("text=permviewuser")).toBeVisible({
      timeout: 10000,
    });

    // Their server permission should be visible
    await expect(page.locator("text=manager").first()).toBeVisible({
      timeout: 5000,
    });
  });
});
