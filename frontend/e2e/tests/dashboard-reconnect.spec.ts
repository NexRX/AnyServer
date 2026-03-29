/**
 * Dashboard Reconnection Banner Test Suite
 *
 * Tests the behavior of the "Connection lost — reconnecting…" banner on the
 * Dashboard (homepage) when the backend server goes down and comes back up.
 *
 * Bug reproduced:
 * 1. Start server, navigate to dashboard
 * 2. Kill the backend
 * 3. The banner appears briefly, then disappears, then reappears — "flapping"
 *    — because each reconnect attempt transitions through "connecting" state
 *    which resets the ConnectionBanner's debounce timer.
 * 4. Restart the backend
 * 5. The banner continues to show "reconnecting" for an extended period even
 *    though the server is already up, because the exponential backoff delay
 *    hasn't elapsed yet.
 *
 * Root cause:
 * The `ConnectionBanner` component treats the "connecting" state (which occurs
 * during every reconnect attempt) as a "good" state, clearing the debounce
 * timer and hiding the banner. This creates a cycle where the banner flaps
 * in and out of visibility with each reconnect attempt.
 *
 * Expected (fixed) behavior:
 * - After the backend goes down, the banner should appear once (after the
 *   debounce period) and STAY visible until the connection is truly
 *   re-established ("connected" state).
 * - After the backend comes back, the banner should disappear promptly
 *   once the WebSocket reconnects.
 * - The banner should never flap (appear/disappear/appear) during a
 *   sustained outage.
 */

import { test, expect } from "../fixtures/test-environment";
import { loginViaUI } from "../helpers/auth";
import {
  createApiClient,
  createServer,
  createMinimalServerConfig,
  cleanupAllServers,
} from "../helpers/api";

/**
 * The ConnectionBanner's default debounce period.
 * The banner should not appear before this time has elapsed.
 */
const BANNER_DEBOUNCE_MS = 3000;

/**
 * Selector for the disconnect/reconnecting banner on the dashboard.
 * Matches the CSS class used by ConnectionBanner.
 */
const BANNER_SELECTOR = ".ws-disconnect-banner";

test.describe("Dashboard reconnection banner", () => {
  test.afterEach(async ({ testEnv }) => {
    // Make sure backend is running for cleanup
    try {
      await testEnv.restartBackend();
    } catch {
      // already running — fine
    }
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);
    await cleanupAllServers(client);
  });

  test("banner appears and stays visible when backend goes down (no flapping)", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server so the dashboard has something to display
    const config = createMinimalServerConfig("reconnect-test-server");
    await createServer(client, config);

    // Log in and navigate to the dashboard (homepage)
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    // Verify the dashboard loaded and we see the server
    await expect(page.locator("h1")).toContainText("Servers", {
      timeout: 10000,
    });
    await expect(
      page.locator(".server-card, .server-grid").first(),
    ).toBeVisible({
      timeout: 10000,
    });

    // Confirm the banner is NOT visible initially
    const banner = page.locator(BANNER_SELECTOR);
    await expect(banner).not.toBeVisible();

    // --- Kill the backend ---
    await testEnv.killBackend();

    // The banner should NOT appear immediately (debounce protects against that).
    // Wait a bit less than the debounce period and confirm it's still hidden.
    await page.waitForTimeout(Math.max(BANNER_DEBOUNCE_MS - 1500, 500));
    // It may or may not be visible yet depending on timing, but we'll check
    // for flapping below which is the real assertion.

    // Wait for the banner to appear (debounce period + reconnect cycle time).
    // Give extra headroom for the debounce + first reconnect attempt.
    await expect(banner).toBeVisible({
      timeout: BANNER_DEBOUNCE_MS + 10000,
    });

    // --- Core assertion: banner must STAY visible (no flapping) ---
    // Sample the banner visibility over a sustained window.
    // If the bug is present, the banner will flap (appear/disappear/appear)
    // as each reconnect attempt briefly transitions through "connecting" state.
    const MONITOR_DURATION_MS = 12000;
    const SAMPLE_INTERVAL_MS = 200;
    const samples = Math.ceil(MONITOR_DURATION_MS / SAMPLE_INTERVAL_MS);
    let invisibleCount = 0;
    const invisibleTimestamps: number[] = [];

    for (let i = 0; i < samples; i++) {
      const isVisible = await banner.isVisible().catch(() => false);
      if (!isVisible) {
        invisibleCount++;
        invisibleTimestamps.push(i * SAMPLE_INTERVAL_MS);
      }
      await page.waitForTimeout(SAMPLE_INTERVAL_MS);
    }

    // Allow at most 1-2 invisible samples due to timing jitter, but definitely
    // not the sustained flapping pattern (which would show dozens).
    expect(
      invisibleCount,
      `Banner flapped! It was invisible for ${invisibleCount}/${samples} samples ` +
        `over ${MONITOR_DURATION_MS}ms while backend was down. ` +
        `Invisible at offsets (ms): [${invisibleTimestamps.join(", ")}]. ` +
        `Expected the banner to stay visible once it appeared.`,
    ).toBeLessThanOrEqual(2);
  });

  test("banner disappears promptly after backend comes back", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    // Create a server
    const config = createMinimalServerConfig("reconnect-recovery-test");
    await createServer(client, config);

    // Log in and navigate to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    await expect(page.locator("h1")).toContainText("Servers", {
      timeout: 10000,
    });

    const banner = page.locator(BANNER_SELECTOR);
    await expect(banner).not.toBeVisible();

    // Kill the backend and wait for the banner to appear
    await testEnv.killBackend();
    await expect(banner).toBeVisible({
      timeout: BANNER_DEBOUNCE_MS + 10000,
    });

    // Let the banner be visible for a bit to confirm it's stable
    await page.waitForTimeout(3000);
    await expect(banner).toBeVisible();

    // --- Restart the backend ---
    await testEnv.restartBackend();

    // The banner should disappear once the WebSocket reconnects.
    // The ReconnectingWebSocket uses exponential backoff, so in the worst
    // case we might need to wait for the current backoff delay to elapse
    // before the reconnect attempt succeeds. Max backoff is 10s.
    await expect(banner).not.toBeVisible({
      timeout: 30000,
    });

    // Confirm the banner stays gone (no false re-appearances)
    const POST_RECONNECT_MONITOR_MS = 6000;
    const POST_SAMPLE_MS = 200;
    const postSamples = Math.ceil(POST_RECONNECT_MONITOR_MS / POST_SAMPLE_MS);
    let postVisibleCount = 0;

    for (let i = 0; i < postSamples; i++) {
      const visible = await banner.isVisible().catch(() => false);
      if (visible) {
        postVisibleCount++;
      }
      await page.waitForTimeout(POST_SAMPLE_MS);
    }

    expect(
      postVisibleCount,
      `Banner reappeared ${postVisibleCount} times after reconnection. ` +
        `Expected it to stay hidden once the connection was restored.`,
    ).toBe(0);
  });

  test("full lifecycle: connected → backend down → banner appears → backend up → banner disappears", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const config = createMinimalServerConfig("full-lifecycle-test");
    await createServer(client, config);

    // Log in and go to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    await expect(page.locator("h1")).toContainText("Servers", {
      timeout: 10000,
    });

    const banner = page.locator(BANNER_SELECTOR);

    // Phase 1: Healthy — no banner
    await expect(banner).not.toBeVisible();

    // Phase 2: Kill backend — banner should appear and stay
    await testEnv.killBackend();

    await expect(banner).toBeVisible({
      timeout: BANNER_DEBOUNCE_MS + 10000,
    });

    // Verify banner content
    const bannerText = await banner.textContent();
    expect(bannerText).toContain("reconnecting");

    // Phase 3: Restart backend — banner should disappear
    await testEnv.restartBackend();

    await expect(banner).not.toBeVisible({
      timeout: 30000,
    });

    // Phase 4: Dashboard should be functional again — data should reload
    // The useGlobalEvents hook calls refetch() on reconnect, so the server
    // list should still be visible.
    await expect(
      page.locator(".server-card, .server-grid").first(),
    ).toBeVisible({ timeout: 15000 });
  });

  test("banner does not appear for brief backend restart (shorter than debounce)", async ({
    page,
    testEnv,
  }) => {
    const client = createApiClient(testEnv.apiUrl, testEnv.adminToken);

    const config = createMinimalServerConfig("brief-restart-test");
    await createServer(client, config);

    // Log in and go to dashboard
    await loginViaUI(page, testEnv.baseUrl, "admin", "Admin123");
    await page.goto(`${testEnv.baseUrl}/`);
    await page.waitForLoadState("networkidle");

    await expect(page.locator("h1")).toContainText("Servers", {
      timeout: 10000,
    });

    const banner = page.locator(BANNER_SELECTOR);
    await expect(banner).not.toBeVisible();

    // Kill and immediately restart the backend.
    // If the WebSocket reconnects before the debounce period (3s), the
    // banner should never appear at all.
    await testEnv.killBackend();
    // Wait just a moment to let the disconnect register
    await page.waitForTimeout(500);
    await testEnv.restartBackend();

    // Monitor for banner — it should NOT appear since the outage was brief.
    // We wait longer than the debounce to be sure.
    const MONITOR_MS = BANNER_DEBOUNCE_MS + 5000;
    const SAMPLE_MS = 200;
    const totalSamples = Math.ceil(MONITOR_MS / SAMPLE_MS);
    let visibleCount = 0;

    for (let i = 0; i < totalSamples; i++) {
      const visible = await banner.isVisible().catch(() => false);
      if (visible) {
        visibleCount++;
      }
      await page.waitForTimeout(SAMPLE_MS);
    }

    // Allow a small number of visible samples due to timing, but the banner
    // should not have been persistently visible.
    expect(
      visibleCount,
      `Banner appeared ${visibleCount}/${totalSamples} times during a brief restart. ` +
        `A restart shorter than the debounce window should not trigger the banner.`,
    ).toBeLessThanOrEqual(3);
  });
});
