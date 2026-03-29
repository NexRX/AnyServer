/**
 * Parallel Execution Isolation Tests (Ticket #054)
 *
 * These tests verify that multiple test workers can run in parallel without
 * interfering with each other. Each worker should have completely isolated:
 * - Backend server (unique port)
 * - Frontend server (unique port)
 * - Database (unique data directory)
 * - Admin user (independent setup)
 */

import { test, expect } from "../fixtures/test-environment";

test.describe("Parallel Execution Isolation", () => {
  test("each worker gets unique backend port", async ({ testEnv }) => {
    // The backend port should be in the range 4000-4899 (for up to 10 workers with 100-port offset)
    expect(testEnv.backendPort).toBeGreaterThanOrEqual(4000);
    expect(testEnv.backendPort).toBeLessThan(5000);

    // Verify backend is accessible on its unique port
    const response = await fetch(`${testEnv.apiUrl}/api/auth/status`);
    expect(response.ok).toBe(true);
  });

  test("each worker gets unique frontend port", async ({ testEnv }) => {
    // The frontend port should be in the range 5000-5899 (for up to 10 workers with 100-port offset)
    expect(testEnv.frontendPort).toBeGreaterThanOrEqual(5000);
    expect(testEnv.frontendPort).toBeLessThan(6000);

    // Verify frontend is accessible on its unique port
    const response = await fetch(testEnv.baseUrl);
    expect(response.ok).toBe(true);
  });

  test("each worker gets isolated data directory", async ({ testEnv }) => {
    // Data directory should be in /tmp (or nix-shell temp) and contain worker ID
    // In nix-shell: /tmp/nix-shell-xxx-xxx/anyserver-e2e-1-xxx
    // Outside nix: /tmp/anyserver-e2e-1-xxx
    expect(testEnv.dataDir).toMatch(/anyserver-e2e-\d+-[a-zA-Z0-9]+$/);

    // Directory should exist
    const fs = await import("fs");
    expect(fs.existsSync(testEnv.dataDir)).toBe(true);
  });

  test("each worker has independent admin user", async ({ testEnv }) => {
    // Verify the admin token works for this worker's backend
    const response = await fetch(`${testEnv.apiUrl}/api/auth/me`, {
      headers: {
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
    });

    expect(response.ok).toBe(true);
    const data = await response.json();
    expect(data.user.username).toBe("admin");
    expect(data.user.role).toBe("admin");
  });

  test("each worker can create and manage servers independently", async ({
    testEnv,
  }) => {
    const serverName = `Test Server ${Math.random().toString(36).substring(7)}`;

    // Create a test server using the API
    const createResponse = await fetch(`${testEnv.apiUrl}/api/servers`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
      body: JSON.stringify({
        config: {
          name: serverName,
          description: "Test server for parallel isolation",
          working_directory: "/tmp/test",
          binary: "/bin/echo",
          command: "/bin/echo",
          args: ["hello"],
          auto_start: false,
          stop_signal: "sigterm",
          stop_timeout_secs: 10,
        },
      }),
    });

    if (!createResponse.ok) {
      const errorText = await createResponse.text();
      throw new Error(
        `Failed to create server: ${createResponse.status} ${errorText}`,
      );
    }

    const server = await createResponse.json();
    expect(server.server.id).toBeDefined();

    // Verify we can retrieve the server
    const getResponse = await fetch(
      `${testEnv.apiUrl}/api/servers/${server.server.id}`,
      {
        headers: {
          Authorization: `Bearer ${testEnv.adminToken}`,
        },
      },
    );
    expect(getResponse.ok).toBe(true);

    // Clean up
    const deleteResponse = await fetch(
      `${testEnv.apiUrl}/api/servers/${server.server.id}`,
      {
        method: "DELETE",
        headers: {
          Authorization: `Bearer ${testEnv.adminToken}`,
        },
      },
    );
    expect(deleteResponse.ok).toBe(true);
  });

  test("parallel workers don't cause database constraint violations", async ({
    testEnv,
  }) => {
    // This test verifies that the admin user was created successfully
    // without any UNIQUE constraint violations from other workers
    const response = await fetch(`${testEnv.apiUrl}/api/auth/me`, {
      headers: {
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
    });

    expect(response.ok).toBe(true);
    const data = await response.json();
    expect(data.user.username).toBe("admin");
    expect(data.user.role).toBe("admin");
  });

  test("parallel workers handle API requests correctly", async ({
    testEnv,
  }) => {
    // Verify basic API access works without triggering rate limits
    const response = await fetch(`${testEnv.apiUrl}/api/auth/status`);

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Request failed: ${response.status} ${errorText}`);
    }

    const data = await response.json();
    expect(data).toHaveProperty("setup_complete");
    expect(data.setup_complete).toBe(true);
  });

  test("worker data directory is unique across test runs", async ({
    testEnv,
  }) => {
    // The data directory should include a random component
    // to prevent conflicts even if the same worker ID is reused
    // Pattern works for both nix-shell and regular /tmp
    const match = testEnv.dataDir.match(/anyserver-e2e-(\d+)-([a-zA-Z0-9]+)$/);
    expect(match).not.toBeNull();

    const [, workerId, randomId] = match!;
    expect(workerId).toMatch(/^\d+$/);
    expect(randomId.length).toBeGreaterThan(4); // Should be reasonably random
  });

  test("JWT tokens are isolated per worker", async ({ testEnv }) => {
    // Each worker should have its own JWT secret, so tokens from one
    // worker should not be valid for another worker's backend

    // This test just verifies our token works for our backend
    const response = await fetch(`${testEnv.apiUrl}/api/auth/me`, {
      headers: {
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
    });

    expect(response.ok).toBe(true);

    // If we try an invalid token, it should be rejected
    const badResponse = await fetch(`${testEnv.apiUrl}/api/auth/me`, {
      headers: {
        Authorization: "Bearer invalid-token-12345",
      },
    });

    expect(badResponse.ok).toBe(false);
    expect(badResponse.status).toBe(401);
  });

  test("worker cleanup doesn't affect other workers", async ({ testEnv }) => {
    // This test verifies that the test environment is stable
    // even when other workers are starting up or cleaning up

    // Make a series of requests with delays
    for (let i = 0; i < 5; i++) {
      const response = await fetch(`${testEnv.apiUrl}/api/auth/status`);
      expect(response.ok).toBe(true);
      const data = await response.json();
      expect(data.setup_complete).toBe(true);

      // Small delay to allow other workers to churn
      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    // Verify backend is still responsive
    const finalResponse = await fetch(`${testEnv.apiUrl}/api/auth/me`, {
      headers: {
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
    });
    expect(finalResponse.ok).toBe(true);
  });
});

test.describe("Parallel Execution Performance", () => {
  test("backend responds quickly under parallel load", async ({ testEnv }) => {
    const start = Date.now();

    const response = await fetch(`${testEnv.apiUrl}/api/auth/status`);
    expect(response.ok).toBe(true);

    const duration = Date.now() - start;

    // Backend should respond in less than 1 second even under parallel load
    expect(duration).toBeLessThan(1000);
  });

  test("database operations complete successfully", async ({ testEnv }) => {
    // Verify we can read from the database
    const listResponse = await fetch(`${testEnv.apiUrl}/api/servers`, {
      headers: {
        Authorization: `Bearer ${testEnv.adminToken}`,
      },
    });

    expect(listResponse.ok).toBe(true);
    const data = await listResponse.json();
    expect(Array.isArray(data.servers)).toBe(true);
  });
});
