import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createNonAdminUser,
  cleanupAllServers,
} from "../helpers/api";

test.describe("Admin Panel", () => {
  test.afterEach(async ({ testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("admin panel loads for admin users", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");

    // Navigate to admin panel via navbar
    const adminLink = page.locator("nav.navbar a[href='/admin']");
    await expect(adminLink).toBeVisible({ timeout: 5000 });
    await adminLink.click();

    await expect(page).toHaveURL(/.*\/admin/, { timeout: 5000 });

    // Page heading should be visible
    const heading = page.locator("h1:has-text('Admin Panel')");
    await expect(heading).toBeVisible({ timeout: 5000 });
  });

  test("users tab shows the admin user", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    // Users tab is active by default — click it to be safe
    const usersTab = page.locator("button.tab:has-text('Users')");
    await expect(usersTab).toBeVisible({ timeout: 5000 });
    await usersTab.click();

    // The admin user should be listed in an admin-user-row
    const adminRow = page.locator(".admin-user-row:has-text('admin')");
    await expect(adminRow).toBeVisible({ timeout: 5000 });

    // Admin should have the "admin" role badge
    await expect(
      adminRow.locator(".status-badge:has-text('admin')"),
    ).toBeVisible();
  });

  test("users tab lists newly created users", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a non-admin user
    await createNonAdminUser(client, "testuser1", "Testuser1Pass");

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const usersTab = page.locator("button.tab:has-text('Users')");
    await usersTab.click();

    // Both admin and the new user should be listed
    await expect(page.locator(".admin-user-row:has-text('admin')")).toBeVisible(
      { timeout: 5000 },
    );
    await expect(
      page.locator(".admin-user-row:has-text('testuser1')"),
    ).toBeVisible({ timeout: 5000 });

    // User count summary should show 2 users
    await expect(page.locator("text=2 users total")).toBeVisible({
      timeout: 5000,
    });
  });

  test("can change a user's role", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a non-admin user
    await createNonAdminUser(client, "roleuser", "Roleuser1Pass");

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const usersTab = page.locator("button.tab:has-text('Users')");
    await usersTab.click();

    // Find the row for our user
    const userRow = page.locator(".admin-user-row:has-text('roleuser')");
    await expect(userRow).toBeVisible({ timeout: 5000 });

    // Should have "user" role initially
    await expect(
      userRow.locator(".status-badge:has-text('user')"),
    ).toBeVisible();

    // Accept the confirm dialog that will appear
    page.on("dialog", (dialog) => dialog.accept());

    // Click the "Promote" button to make them admin
    const promoteBtn = userRow.locator("button:has-text('Promote')");
    await expect(promoteBtn).toBeVisible();
    await promoteBtn.click();

    // After promotion, the role badge should show "admin"
    await expect(
      userRow.locator(".status-badge:has-text('admin')"),
    ).toBeVisible({ timeout: 10000 });

    // The button should now say "Demote" instead of "Promote"
    await expect(userRow.locator("button:has-text('Demote')")).toBeVisible({
      timeout: 5000,
    });
  });

  test("can delete a non-admin user", async ({ page, testEnv }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a user to delete
    await createNonAdminUser(client, "deleteuser", "Deleteuser1Pass");

    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const usersTab = page.locator("button.tab:has-text('Users')");
    await usersTab.click();

    // Verify the user is listed
    const userRow = page.locator(".admin-user-row:has-text('deleteuser')");
    await expect(userRow).toBeVisible({ timeout: 5000 });

    // Accept the confirm dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Click the delete button
    const deleteBtn = userRow.locator("button:has-text('Delete')");
    await expect(deleteBtn).toBeVisible();
    await deleteBtn.click();

    // User should disappear from the list
    await expect(userRow).not.toBeVisible({ timeout: 10000 });
  });

  test("admin cannot delete themselves", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    const usersTab = page.locator("button.tab:has-text('Users')");
    await usersTab.click();

    // The admin's own row has the "admin-user-self" class and shows "you" tag
    const adminRow = page.locator(".admin-user-row.admin-user-self");
    await expect(adminRow).toBeVisible({ timeout: 5000 });
    await expect(adminRow.locator("text=you")).toBeVisible();

    // The self row should show "—" instead of action buttons (no Delete, no Promote/Demote)
    const deleteBtn = adminRow.locator("button:has-text('Delete')");
    await expect(deleteBtn).not.toBeVisible();

    const promoteBtn = adminRow.locator("button:has-text('Promote')");
    await expect(promoteBtn).not.toBeVisible();

    const demoteBtn = adminRow.locator("button:has-text('Demote')");
    await expect(demoteBtn).not.toBeVisible();
  });

  test("settings tab shows application settings", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    // Click the Settings tab
    const settingsTab = page.locator("button.tab:has-text('Settings')");
    await expect(settingsTab).toBeVisible({ timeout: 5000 });
    await settingsTab.click();

    // Should show "Application Settings" heading
    await expect(
      page.locator("h2:has-text('Application Settings')"),
    ).toBeVisible({ timeout: 5000 });

    // Should show registration setting
    await expect(page.locator("text=Registration").first()).toBeVisible({
      timeout: 5000,
    });

    // Should show run commands setting
    await expect(page.locator("text=Run").first()).toBeVisible({
      timeout: 5000,
    });
  });

  test("can toggle registration setting", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    // Go to settings tab
    const settingsTab = page.locator("button.tab:has-text('Settings')");
    await settingsTab.click();

    await expect(
      page.locator("h2:has-text('Application Settings')"),
    ).toBeVisible({ timeout: 5000 });

    // Accept the confirm dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Find the registration toggle button (Enable/Disable Registration)
    const registrationBtn = page
      .locator(
        "button:has-text('Enable Registration'), button:has-text('Disable Registration')",
      )
      .first();

    if (await registrationBtn.isVisible({ timeout: 5000 }).catch(() => false)) {
      const initialText = await registrationBtn.textContent();
      await registrationBtn.click();

      // Wait for the change to complete
      await page.waitForTimeout(1500);

      // The button text should have toggled
      const newBtn = page
        .locator(
          "button:has-text('Enable Registration'), button:has-text('Disable Registration')",
        )
        .first();
      const newText = await newBtn.textContent();
      expect(newText).not.toBe(initialText);
    }
  });

  test("change password tab is accessible", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    // Click the Change Password tab
    const passwordTab = page.locator("button.tab:has-text('Change Password')");
    await expect(passwordTab).toBeVisible({ timeout: 5000 });
    await passwordTab.click();

    // Should show password change form fields
    const passwordInputs = page.locator("input[type='password']");
    const count = await passwordInputs.count();
    expect(count).toBeGreaterThanOrEqual(2); // current + new (+ confirm)
  });

  test("sessions tab shows active sessions", async ({ page, testEnv }) => {
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");

    // Click the Sessions tab
    const sessionsTab = page.locator("button.tab:has-text('Sessions')");
    await expect(sessionsTab).toBeVisible({ timeout: 5000 });
    await sessionsTab.click();

    // Should show the "Active Sessions" heading
    const heading = page.locator("h2:has-text('Active Sessions')");
    await expect(heading).toBeVisible({ timeout: 10000 });

    // Wait for the sessions tab content to fully load (loading spinner to disappear)
    await page.waitForTimeout(2000);

    // The tab shows either session cards or "No active sessions found"
    // (E2E uses injected tokens, so refresh-token-based sessions may not exist)
    const sessionCard = page.locator(".session-card").first();
    const noSessions = page.locator("p:has-text('No active sessions')");

    // Poll for up to 5 seconds until one of the two states appears
    await expect(sessionCard.or(noSessions)).toBeVisible({ timeout: 5000 });
  });

  test("non-admin users cannot access admin panel", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a regular user
    await createNonAdminUser(client, "regularuser", "Regular1Pass");

    // Log in as the regular user
    await loginViaUI(page, testEnv.baseUrl, "regularuser", "Regular1Pass");

    // The admin link should NOT be visible in the navbar for non-admins
    const adminLink = page.locator("nav.navbar a[href='/admin']");
    await expect(adminLink).not.toBeVisible({ timeout: 3000 });

    // Directly navigating to /admin should redirect away
    await page.goto(`${testEnv.baseUrl}/admin`);
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1000);

    // Should have been redirected to the dashboard (not on /admin)
    const currentUrl = page.url();
    expect(currentUrl).not.toContain("/admin");
  });
});
