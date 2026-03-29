import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright configuration for AnyServer E2E tests.
 *
 * Features:
 * - Parallel execution with isolated test environments
 * - Each worker gets its own backend + frontend instance
 * - Fast execution with smart retries
 * - Comprehensive reporting and debugging
 */
export default defineConfig({
  testDir: "./e2e/tests",

  // Test timeouts
  timeout: 60_000, // 60 seconds per test
  expect: {
    timeout: 10_000, // 10 seconds for assertions
  },

  // Parallelization
  fullyParallel: true,
  workers: process.env.CI ? 2 : undefined, // Auto workers locally, 2 in CI

  // Retries
  retries: process.env.CI ? 2 : 1, // 1 retry locally to handle flaky infrastructure

  // Reporting
  reporter: process.env.CI
    ? [["html"], ["github"]]
    : [["list"], ["html", { open: "never" }]],

  // Global setup/teardown
  globalTimeout: 300_000, // 5 minutes total for all tests

  // Shared settings for all projects
  use: {
    // Base URL is set per-test via fixture
    baseURL: undefined,

    // Tracing and debugging
    trace: process.env.CI ? "retain-on-failure" : "on-first-retry",
    screenshot: "only-on-failure",
    video: process.env.CI ? "retain-on-failure" : "off",

    // Navigation
    actionTimeout: 10_000,
    navigationTimeout: 30_000,

    // Viewport
    viewport: { width: 1280, height: 720 },

    // Ignore HTTPS errors in test environment
    ignoreHTTPSErrors: true,
  },

  // Test projects
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // Use headless mode by default, can override with --headed
        headless: !process.env.HEADED,
      },
    },

    // Uncomment to test in Firefox and WebKit
    // {
    //   name: "firefox",
    //   use: { ...devices["Desktop Firefox"] },
    // },
    // {
    //   name: "webkit",
    //   use: { ...devices["Desktop Safari"] },
    // },
  ],

  // Output folders
  outputDir: "./test-results",
  snapshotDir: "./e2e/snapshots",

  // Fail fast in CI
  maxFailures: process.env.CI ? 10 : undefined,
});
