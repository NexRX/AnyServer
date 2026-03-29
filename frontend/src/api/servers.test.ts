import { describe, it, expect, beforeEach, vi } from "vitest";
import { getServer } from "./servers";
import { ApiClientError } from "./core";

// Mock fetch globally
const mockFetch = vi.fn();
global.fetch = mockFetch as any;

describe("getServer API", () => {
  beforeEach(() => {
    mockFetch.mockClear();
    // Clear any stored token
    vi.resetModules();
  });

  it("should throw ApiClientError with status 404 when server not found", async () => {
    // Mock a 404 response
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 404,
      json: async () => ({
        error: "Server 00000000-0000-0000-0000-000000000000 not found",
        details: null,
      }),
    });

    // Attempt to get a non-existent server
    const fakeId = "00000000-0000-0000-0000-000000000000";

    try {
      await getServer(fakeId);
      // Should not reach here
      expect.fail("Expected ApiClientError to be thrown");
    } catch (error) {
      expect(error).toBeInstanceOf(ApiClientError);
      expect((error as ApiClientError).status).toBe(404);
      expect((error as ApiClientError).message).toContain("not found");
    }
  });

  it("should successfully return server data when found", async () => {
    const mockServer = {
      id: "123e4567-e89b-12d3-a456-426614174000",
      config: {
        name: "test-server",
        binary: "/usr/bin/test",
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
      runtime: {
        status: "stopped",
        pid: null,
        started_at: null,
        restart_count: 0,
      },
      installed: false,
      phase_progress: null,
      parameter_values: {},
      source_template_id: null,
      permission: {
        level: "owner",
        is_global_admin: false,
      },
    };

    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => mockServer,
    });

    const result = await getServer("123e4567-e89b-12d3-a456-426614174000");
    expect(result).toEqual(mockServer);
  });
});
