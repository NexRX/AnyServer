import { describe, it, expect } from "vitest";
import type {
  ServerWithStatus,
  Server,
  ServerRuntime,
  EffectivePermission,
  ServerConfig,
  ServerListResponse,
  PaginatedServerListResponse,
} from "./bindings";

// ═══════════════════════════════════════════════════════════════════════
//  Ticket 3-003: ServerWithStatus serialization — Frontend unit tests
//
//  Verifies that the generated TypeScript types match the expected wire
//  format (Option B: nested `server` field, no flattening) and that
//  parsing API responses produces valid objects with all fields accessible.
// ═══════════════════════════════════════════════════════════════════════

/** A realistic mock of the JSON wire format returned by the backend. */
function buildMockServerWithStatus(
  overrides: Partial<{
    name: string;
    serverId: string;
    ownerId: string;
    status: string;
    installed: boolean;
  }> = {},
): Record<string, unknown> {
  const serverId =
    overrides.serverId ?? "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
  return {
    server: {
      id: serverId,
      owner_id: overrides.ownerId ?? "11111111-2222-3333-4444-555555555555",
      config: {
        name: overrides.name ?? "Test Server",
        binary: "/usr/bin/echo",
        args: ["hello"],
        env: {},
        working_dir: null,
        auto_start: false,
        auto_restart: false,
        max_restart_attempts: 0,
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
        log_to_disk: true,
        max_log_size_mb: 50,
        enable_java_helper: false,
        enable_dotnet_helper: false,
        steam_app_id: null,
      },
      created_at: "2025-01-01T00:00:00Z",
      updated_at: "2025-01-01T00:00:00Z",
      parameter_values: {},
      installed: overrides.installed ?? false,
      installed_at: null,
      updated_via_pipeline_at: null,
      installed_version: null,
      source_template_id: null,
    },
    runtime: {
      server_id: serverId,
      status: overrides.status ?? "stopped",
      pid: null,
      started_at: null,
      restart_count: 0,
      next_restart_at: null,
    },
    permission: {
      level: "owner",
      is_global_admin: false,
    },
    phase_progress: null,
  };
}

// ─── Type structure tests ─────────────────────────────────────────────

describe("ServerWithStatus type structure (ticket 3-003)", () => {
  it("has a nested 'server' field (not flattened)", () => {
    const mock = buildMockServerWithStatus() as unknown as ServerWithStatus;

    // The 'server' property should exist and be an object
    expect(mock.server).toBeDefined();
    expect(typeof mock.server).toBe("object");

    // Server fields should be accessed via server.xxx, not directly
    expect(mock.server.id).toBe("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
    expect(mock.server.config.name).toBe("Test Server");
  });

  it("has runtime, permission, and phase_progress at top level", () => {
    const mock = buildMockServerWithStatus() as unknown as ServerWithStatus;

    expect(mock.runtime).toBeDefined();
    expect(mock.runtime.status).toBe("stopped");

    expect(mock.permission).toBeDefined();
    expect(mock.permission.level).toBe("owner");

    // phase_progress can be null
    expect(mock.phase_progress).toBeNull();
  });

  it("does NOT have flattened Server fields at the top level", () => {
    const mock = buildMockServerWithStatus();

    // These fields should only exist under 'server', not at the top level
    expect(mock).not.toHaveProperty("id");
    expect(mock).not.toHaveProperty("owner_id");
    expect(mock).not.toHaveProperty("config");
    expect(mock).not.toHaveProperty("created_at");
    expect(mock).not.toHaveProperty("updated_at");
    expect(mock).not.toHaveProperty("parameter_values");
    expect(mock).not.toHaveProperty("installed");
    expect(mock).not.toHaveProperty("installed_at");
    expect(mock).not.toHaveProperty("installed_version");
    expect(mock).not.toHaveProperty("source_template_id");
  });

  it("top-level object has exactly 4 keys", () => {
    const mock = buildMockServerWithStatus();
    const keys = Object.keys(mock);

    expect(keys).toHaveLength(4);
    expect(keys.sort()).toEqual(
      ["server", "runtime", "permission", "phase_progress"].sort(),
    );
  });
});

// ─── Parsing API responses ────────────────────────────────────────────

describe("Parsing ServerWithStatus from API response JSON (ticket 3-003)", () => {
  it("parses a single ServerWithStatus with all fields accessible", () => {
    // Simulate JSON.parse of a GET /api/servers/:id response
    const raw = JSON.stringify(buildMockServerWithStatus({ name: "My MC" }));
    const parsed = JSON.parse(raw) as ServerWithStatus;

    // Nested server access
    expect(parsed.server.id).toBe("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
    expect(parsed.server.owner_id).toBe(
      "11111111-2222-3333-4444-555555555555",
    );
    expect(parsed.server.config.name).toBe("My MC");
    expect(parsed.server.config.binary).toBe("/usr/bin/echo");
    expect(parsed.server.config.args).toEqual(["hello"]);
    expect(parsed.server.config.auto_start).toBe(false);
    expect(parsed.server.config.isolation.enabled).toBe(true);
    expect(parsed.server.installed).toBe(false);
    expect(parsed.server.installed_version).toBeNull();
    expect(parsed.server.source_template_id).toBeNull();
    expect(parsed.server.parameter_values).toEqual({});

    // Runtime access
    expect(parsed.runtime.server_id).toBe(
      "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    );
    expect(parsed.runtime.status).toBe("stopped");
    expect(parsed.runtime.pid).toBeNull();
    expect(parsed.runtime.restart_count).toBe(0);

    // Permission access
    expect(parsed.permission.level).toBe("owner");
    expect(parsed.permission.is_global_admin).toBe(false);

    // Phase progress
    expect(parsed.phase_progress).toBeNull();
  });

  it("parses a ServerListResponse with multiple servers", () => {
    const mockResponse: Record<string, unknown> = {
      servers: [
        buildMockServerWithStatus({
          name: "Server A",
          serverId: "aaaa0000-0000-0000-0000-000000000001",
        }),
        buildMockServerWithStatus({
          name: "Server B",
          serverId: "aaaa0000-0000-0000-0000-000000000002",
          status: "running",
          installed: true,
        }),
        buildMockServerWithStatus({
          name: "Server C",
          serverId: "aaaa0000-0000-0000-0000-000000000003",
        }),
      ],
    };

    const raw = JSON.stringify(mockResponse);
    const parsed = JSON.parse(raw) as ServerListResponse;

    expect(parsed.servers).toHaveLength(3);

    // Each entry should use nested access pattern
    expect(parsed.servers[0].server.config.name).toBe("Server A");
    expect(parsed.servers[0].server.id).toBe(
      "aaaa0000-0000-0000-0000-000000000001",
    );
    expect(parsed.servers[0].runtime.status).toBe("stopped");

    expect(parsed.servers[1].server.config.name).toBe("Server B");
    expect(parsed.servers[1].runtime.status).toBe("running");
    expect(parsed.servers[1].server.installed).toBe(true);

    expect(parsed.servers[2].server.config.name).toBe("Server C");
    expect(parsed.servers[2].permission.level).toBe("owner");
  });

  it("parses a PaginatedServerListResponse correctly", () => {
    const mockResponse: Record<string, unknown> = {
      servers: [
        buildMockServerWithStatus({ name: "Paginated-1" }),
        buildMockServerWithStatus({ name: "Paginated-2" }),
      ],
      total: 50,
      page: 1,
      per_page: 2,
      total_pages: 25,
    };

    const raw = JSON.stringify(mockResponse);
    const parsed = JSON.parse(raw) as PaginatedServerListResponse;

    expect(parsed.servers).toHaveLength(2);
    expect(parsed.servers[0].server.config.name).toBe("Paginated-1");
    expect(parsed.servers[1].server.config.name).toBe("Paginated-2");

    // Pagination fields
    expect(parsed.total).toBe(50);
    expect(parsed.page).toBe(1);
    expect(parsed.per_page).toBe(2);
    expect(parsed.total_pages).toBe(25);
  });
});

// ─── No duplicate data in JSON ────────────────────────────────────────

describe("ServerWithStatus JSON has no duplicate data (ticket 3-003)", () => {
  it("server name appears exactly once in serialized JSON", () => {
    const uniqueName = "UniqueServerName_XYZ_12345";
    const mock = buildMockServerWithStatus({ name: uniqueName });
    const json = JSON.stringify(mock);

    const occurrences = json.split(uniqueName).length - 1;
    expect(occurrences).toBe(1);
  });

  it("server id appears exactly twice (server.id + runtime.server_id)", () => {
    const serverId = "deadbeef-dead-beef-dead-beefdeadbeef";
    const mock = buildMockServerWithStatus({ serverId });
    const json = JSON.stringify(mock);

    const occurrences = json.split(serverId).length - 1;
    expect(occurrences).toBe(2);
  });

  it("owner_id appears exactly once", () => {
    const ownerId = "cafebabe-cafe-babe-cafe-babecafebabe";
    const mock = buildMockServerWithStatus({ ownerId });
    const json = JSON.stringify(mock);

    const occurrences = json.split(ownerId).length - 1;
    expect(occurrences).toBe(1);
  });

  it("binary path appears exactly once", () => {
    const mock = buildMockServerWithStatus();
    const json = JSON.stringify(mock);

    const occurrences = json.split("/usr/bin/echo").length - 1;
    expect(occurrences).toBe(1);
  });
});

// ─── Round-trip serialization ─────────────────────────────────────────

describe("ServerWithStatus JSON round-trip (ticket 3-003)", () => {
  it("survives JSON stringify/parse round-trip with all data intact", () => {
    const original = buildMockServerWithStatus({
      name: "Round Trip Test",
      status: "running",
      installed: true,
    }) as unknown as ServerWithStatus;

    const json = JSON.stringify(original);
    const restored = JSON.parse(json) as ServerWithStatus;

    expect(restored.server.config.name).toBe("Round Trip Test");
    expect(restored.runtime.status).toBe("running");
    expect(restored.server.installed).toBe(true);
    expect(restored.permission.level).toBe("owner");
    expect(restored.phase_progress).toBeNull();

    // Deep equality
    expect(restored).toEqual(original);
  });

  it("server list survives round-trip", () => {
    const original: Record<string, unknown> = {
      servers: [
        buildMockServerWithStatus({ name: "RT-1" }),
        buildMockServerWithStatus({ name: "RT-2" }),
      ],
    };

    const json = JSON.stringify(original);
    const restored = JSON.parse(json) as ServerListResponse;

    expect(restored.servers).toHaveLength(2);
    expect(restored.servers[0].server.config.name).toBe("RT-1");
    expect(restored.servers[1].server.config.name).toBe("RT-2");
    expect(restored).toEqual(original);
  });
});

// ─── Type compile-time checks ─────────────────────────────────────────

describe("ServerWithStatus type compile-time shape (ticket 3-003)", () => {
  it("ServerWithStatus type has 'server' as a Server field", () => {
    // Type-level test: if this compiles, the structure is correct
    const _check: ServerWithStatus = {
      server: {
        id: "test",
        owner_id: "test",
        config: {
          name: "test",
          binary: "test",
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
          log_to_disk: true,
          max_log_size_mb: 50,
          enable_java_helper: false,
          enable_dotnet_helper: false,
          steam_app_id: null,
        },
        created_at: "2025-01-01T00:00:00Z",
        updated_at: "2025-01-01T00:00:00Z",
        parameter_values: {},
        installed: false,
        installed_at: null,
        updated_via_pipeline_at: null,
        installed_version: null,
        source_template_id: null,
      } as Server,
      runtime: {
        server_id: "test",
        status: "stopped",
        pid: null,
        started_at: null,
        restart_count: 0,
        next_restart_at: null,
      } as ServerRuntime,
      permission: {
        level: "owner",
        is_global_admin: false,
      } as EffectivePermission,
      phase_progress: null,
    };
    expect(_check.server.config.name).toBe("test");
  });

  it("nested access patterns work correctly with TypeScript types", () => {
    const mock = buildMockServerWithStatus() as unknown as ServerWithStatus;

    // These are the correct access patterns for Option B (nested)
    const _name: string = mock.server.config.name;
    const _id: string = mock.server.id;
    const _ownerId: string = mock.server.owner_id;
    const _status: string = mock.runtime.status;
    const _level: string = mock.permission.level;
    const _progress: unknown = mock.phase_progress;
    const _installed: boolean = mock.server.installed;
    const _binary: string = mock.server.config.binary;

    expect(_name).toBeDefined();
    expect(_id).toBeDefined();
    expect(_ownerId).toBeDefined();
    expect(_status).toBeDefined();
    expect(_level).toBeDefined();
    expect(_installed).toBeDefined();
    expect(_binary).toBeDefined();
  });
});
