/**
 * API helper functions for E2E tests.
 * Provides typed wrappers around common API operations.
 */

import type {
  CreateServerRequest,
  ServerWithStatus,
  PaginatedServerListResponse,
  SendCommandRequest,
  ServerRuntime,
  RegisterRequest,
  AuthResponse,
  AppSettings,
  UpdateSettingsRequest,
  RunPhaseResponse,
  PhaseStatusResponse,
  MarkInstalledResponse,
  CancelPhaseResponse,
} from "../../src/types/bindings";

export interface ApiClient {
  baseUrl: string;
  token: string;
}

/**
 * Create an API client instance.
 */
export function createApiClient(baseUrl: string, token: string): ApiClient {
  return { baseUrl, token };
}

/**
 * Make an authenticated API request.
 */
async function apiRequest<T>(
  client: ApiClient,
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const url = `${client.baseUrl}${path}`;
  const headers: Record<string, string> = {
    Authorization: `Bearer ${client.token}`,
  };

  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }

  const response = await fetch(url, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(
      `API request failed: ${method} ${path} => ${response.status} ${text}`,
    );
  }

  // Handle empty responses
  const contentType = response.headers.get("content-type");
  if (!contentType?.includes("application/json")) {
    return {} as T;
  }

  return response.json();
}

function normalizeCreateServerRequest(
  input: CreateServerRequest | Record<string, unknown>,
): CreateServerRequest {
  const source = input as Record<string, any>;
  const nestedConfig =
    source.config && typeof source.config === "object"
      ? (source.config as Record<string, any>)
      : {};

  const config = { ...nestedConfig };
  const configAliasKeys = [
    "name",
    "binary",
    "args",
    "env",
    "working_dir",
    "auto_start",
    "auto_restart",
    "max_restart_attempts",
    "restart_delay_secs",
    "stop_command",
    "stop_signal",
    "stop_timeout_secs",
    "sftp_username",
    "sftp_password",
    "parameters",
    "stop_steps",
    "start_steps",
    "install_steps",
    "update_steps",
    "uninstall_steps",
    "isolation",
    "update_check",
    "log_to_disk",
    "max_log_size_mb",
    "enable_java_helper",
    "enable_dotnet_helper",
    "steam_app_id",
  ];

  for (const key of configAliasKeys) {
    // Canonical nested config takes precedence over legacy top-level aliases.
    if (config[key] === undefined && source[key] !== undefined) {
      config[key] = source[key];
    }
  }

  return {
    config: config as CreateServerRequest["config"],
    parameter_values: (source.parameter_values ??
      source.parameterValues ??
      {}) as CreateServerRequest["parameter_values"],
    source_template_id: (source.source_template_id ??
      source.sourceTemplateId ??
      null) as CreateServerRequest["source_template_id"],
  };
}

/**
 * Create a server with the given configuration.
 * Supports both canonical payloads and legacy alias fields used by older tests.
 */
export async function createServer(
  client: ApiClient,
  config: CreateServerRequest | Record<string, unknown>,
): Promise<ServerWithStatus> {
  const normalized = normalizeCreateServerRequest(config);
  return apiRequest<ServerWithStatus>(
    client,
    "POST",
    "/api/servers",
    normalized,
  );
}

/**
 * List all servers.
 */
export async function listServers(
  client: ApiClient,
): Promise<PaginatedServerListResponse> {
  return apiRequest<PaginatedServerListResponse>(client, "GET", "/api/servers");
}

/**
 * Get a specific server by ID.
 */
export async function getServer(
  client: ApiClient,
  serverId: string,
): Promise<ServerWithStatus> {
  return apiRequest<ServerWithStatus>(
    client,
    "GET",
    `/api/servers/${serverId}`,
  );
}

/**
 * Delete a server.
 */
export async function deleteServer(
  client: ApiClient,
  serverId: string,
): Promise<void> {
  await apiRequest(client, "DELETE", `/api/servers/${serverId}`);
}

/**
 * Start a server.
 */
export async function startServer(
  client: ApiClient,
  serverId: string,
): Promise<ServerRuntime> {
  return apiRequest<ServerRuntime>(
    client,
    "POST",
    `/api/servers/${serverId}/start`,
  );
}

/**
 * Stop a server.
 */
export async function stopServer(
  client: ApiClient,
  serverId: string,
): Promise<ServerRuntime> {
  return apiRequest<ServerRuntime>(
    client,
    "POST",
    `/api/servers/${serverId}/stop`,
  );
}

/**
 * Restart a server.
 */
export async function restartServer(
  client: ApiClient,
  serverId: string,
): Promise<ServerRuntime> {
  return apiRequest<ServerRuntime>(
    client,
    "POST",
    `/api/servers/${serverId}/restart`,
  );
}

/**
 * Kill a server.
 */
export async function killServer(
  client: ApiClient,
  serverId: string,
): Promise<void> {
  await apiRequest(client, "POST", `/api/servers/${serverId}/kill`);
}

/**
 * Reset a server (stop, kill orphans, delete files, mark uninstalled).
 */
export async function resetServer(
  client: ApiClient,
  serverId: string,
): Promise<{ reset: boolean; id: string; killed_processes: number }> {
  return apiRequest<{ reset: boolean; id: string; killed_processes: number }>(
    client,
    "POST",
    `/api/servers/${serverId}/reset`,
  );
}

/**
 * Trigger the install pipeline for a server.
 */
export async function installServer(
  client: ApiClient,
  serverId: string,
): Promise<RunPhaseResponse> {
  return apiRequest<RunPhaseResponse>(
    client,
    "POST",
    `/api/servers/${serverId}/install`,
    null,
  );
}

/**
 * Trigger the uninstall pipeline for a server.
 */
export async function uninstallServer(
  client: ApiClient,
  serverId: string,
): Promise<RunPhaseResponse> {
  return apiRequest<RunPhaseResponse>(
    client,
    "POST",
    `/api/servers/${serverId}/uninstall`,
    null,
  );
}

/**
 * Trigger the update pipeline for a server.
 */
export async function updateServerPipeline(
  client: ApiClient,
  serverId: string,
): Promise<RunPhaseResponse> {
  return apiRequest<RunPhaseResponse>(
    client,
    "POST",
    `/api/servers/${serverId}/update`,
    null,
  );
}

/**
 * Cancel a running pipeline phase for a server.
 */
export async function cancelPhase(
  client: ApiClient,
  serverId: string,
): Promise<CancelPhaseResponse> {
  return apiRequest<CancelPhaseResponse>(
    client,
    "POST",
    `/api/servers/${serverId}/cancel-phase`,
  );
}

/**
 * Get the current phase status for a server.
 */
export async function getPhaseStatus(
  client: ApiClient,
  serverId: string,
): Promise<PhaseStatusResponse> {
  return apiRequest<PhaseStatusResponse>(
    client,
    "GET",
    `/api/servers/${serverId}/phase-status`,
  );
}

/**
 * Wait for a pipeline phase (install/update/uninstall) to complete.
 * Polls the phase-status endpoint until the phase is no longer running.
 */
export async function waitForPhaseComplete(
  client: ApiClient,
  serverId: string,
  timeout = 30000,
  interval = 500,
): Promise<void> {
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    try {
      const status = await getPhaseStatus(client, serverId);
      if (
        status.progress === null ||
        status.progress === undefined ||
        (status.progress as any).status === "completed" ||
        (status.progress as any).status === "failed"
      ) {
        return;
      }
    } catch (err) {
      // Ignore errors during polling (phase may not exist yet)
    }

    await new Promise((resolve) => setTimeout(resolve, interval));
  }

  throw new Error(
    `Pipeline phase for server ${serverId} did not complete within ${timeout}ms`,
  );
}

/**
 * Send a command to a server's stdin.
 */
export async function sendCommand(
  client: ApiClient,
  serverId: string,
  command: string,
): Promise<void> {
  const body: SendCommandRequest = { command };
  await apiRequest(client, "POST", `/api/servers/${serverId}/command`, body);
}

/**
 * Wait for a server to reach a specific status.
 * Polls the server status endpoint until the desired status is reached.
 */
export async function waitForStatus(
  client: ApiClient,
  serverId: string,
  targetStatus: string,
  timeout = 15000,
  interval = 200,
): Promise<void> {
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    try {
      const server = await getServer(client, serverId);
      if (server.runtime?.status === targetStatus) {
        return;
      }
    } catch (err) {
      // Ignore errors during polling
    }

    await new Promise((resolve) => setTimeout(resolve, interval));
  }

  throw new Error(
    `Server ${serverId} did not reach status '${targetStatus}' within ${timeout}ms`,
  );
}

/**
 * Ensure a server is stopped before proceeding.
 * If it's running, stop it and wait for it to stop.
 */
export async function ensureStopped(
  client: ApiClient,
  serverId: string,
  timeout = 15000,
): Promise<void> {
  const server = await getServer(client, serverId);
  const status = server.runtime?.status;

  if (status === "stopped" || status === "crashed") {
    return; // Already stopped
  }

  if (status === "running" || status === "starting") {
    await stopServer(client, serverId);
    await waitForStatus(client, serverId, "stopped", timeout);
  } else if (status === "stopping") {
    await waitForStatus(client, serverId, "stopped", timeout);
  }
}

/**
 * Clean up all servers for a given API client.
 * Useful for test cleanup.
 */
export async function cleanupAllServers(client: ApiClient): Promise<void> {
  const { servers } = await listServers(client);

  for (const server of servers) {
    try {
      // Try to stop it first if running
      if (
        server.runtime?.status === "running" ||
        server.runtime?.status === "starting"
      ) {
        await stopServer(client, server.server.id);
        await waitForStatus(client, server.server.id, "stopped", 5000);
      }
    } catch (err) {
      // Ignore stop errors, proceed to delete
    }

    try {
      await deleteServer(client, server.server.id);
    } catch (err) {
      console.error(`Failed to delete server ${server.server.id}:`, err);
    }
  }
}

/**
 * Create a minimal test server configuration.
 */
export function createMinimalServerConfig(
  name: string,
  binaryPath = "/run/current-system/sw/bin/sleep",
): CreateServerRequest {
  const request: CreateServerRequest & Record<string, unknown> = {
    config: {
      name,
      binary: binaryPath,
      args: ["infinity"],
      env: {},
      working_dir: null,
      auto_start: false,
      auto_restart: false,
      max_restart_attempts: 5,
      restart_delay_secs: 5,
      stop_command: null,
      stop_signal: "sigterm" as const,
      stop_timeout_secs: 5,
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
      enable_java_helper: false,
      enable_dotnet_helper: false,
      steam_app_id: null,
    },
    parameter_values: {},
    source_template_id: null,
  };

  // Backward-compatible aliases for older tests:
  // - top-level config field aliases (e.g. request.binary = "...")
  // - camelCase request fields
  const requestAny = request as Record<string, any>;

  for (const key of Object.keys(request.config as Record<string, unknown>)) {
    Object.defineProperty(requestAny, key, {
      enumerable: true,
      configurable: true,
      get() {
        return (request.config as Record<string, unknown>)[key];
      },
      set(value: unknown) {
        (request.config as Record<string, unknown>)[key] = value;
      },
    });
  }

  Object.defineProperty(requestAny, "parameterValues", {
    enumerable: true,
    configurable: true,
    get() {
      return request.parameter_values;
    },
    set(value: unknown) {
      request.parameter_values = (value ??
        {}) as CreateServerRequest["parameter_values"];
    },
  });

  Object.defineProperty(requestAny, "sourceTemplateId", {
    enumerable: true,
    configurable: true,
    get() {
      return request.source_template_id;
    },
    set(value: unknown) {
      request.source_template_id = (value ??
        null) as CreateServerRequest["source_template_id"];
    },
  });

  return request;
}

/**
 * Create a server that exits immediately (for crash testing).
 */
export function createCrashingServerConfig(name: string): CreateServerRequest {
  return {
    config: {
      name,
      binary: "/bin/false",
      args: [],
      env: {},
      working_dir: null,
      auto_start: false,
      auto_restart: false,
      max_restart_attempts: 5,
      restart_delay_secs: 5,
      stop_command: null,
      stop_signal: "sigterm" as const,
      stop_timeout_secs: 5,
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
      enable_java_helper: false,
      enable_dotnet_helper: false,
      steam_app_id: null,
    },
    parameter_values: {},
    source_template_id: null,
  };
}

/**
 * Create a server that outputs to stdout/stderr (for console testing).
 */
export function createVerboseServerConfig(name: string): CreateServerRequest {
  return {
    config: {
      name,
      binary: "/run/current-system/sw/bin/sh",
      args: [
        "-c",
        'echo "Server starting..."; while true; do echo "Tick: $(date +%s)"; sleep 1; done',
      ],
      env: {},
      working_dir: null,
      auto_start: false,
      auto_restart: false,
      max_restart_attempts: 5,
      restart_delay_secs: 5,
      stop_command: null,
      stop_signal: "sigterm" as const,
      stop_timeout_secs: 5,
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
      enable_java_helper: false,
      enable_dotnet_helper: false,
      steam_app_id: null,
    },
    parameter_values: {},
    source_template_id: null,
  };
}

/**
 * Enable or disable RunCommand pipeline steps.
 * Required for install/update pipelines that use run_command steps.
 */
export async function enableRunCommands(
  client: ApiClient,
  enabled: boolean = true,
): Promise<void> {
  // First, get current settings
  const meResponse = await apiRequest<{ user: any; settings: AppSettings }>(
    client,
    "GET",
    "/api/auth/me",
  );
  const currentSettings = meResponse.settings;

  const updateBody: UpdateSettingsRequest = {
    registration_enabled: currentSettings.registration_enabled,
    allow_run_commands: enabled,
    run_command_sandbox: currentSettings.run_command_sandbox,
    run_command_default_timeout_secs:
      currentSettings.run_command_default_timeout_secs,
    run_command_use_namespaces: currentSettings.run_command_use_namespaces,
  };

  await apiRequest(client, "PUT", "/api/auth/settings", updateBody);
}

/**
 * Mark a server as installed without running the install pipeline.
 * Requires admin-level permission on the server.
 */
export async function markServerInstalled(
  client: ApiClient,
  serverId: string,
): Promise<MarkInstalledResponse> {
  return apiRequest<MarkInstalledResponse>(
    client,
    "POST",
    `/api/servers/${serverId}/mark-installed`,
  );
}

/**
 * Register a new user (requires registration to be enabled).
 */
export async function registerUser(
  baseUrl: string,
  username: string,
  password: string,
): Promise<{ token: string; userId: string }> {
  const body: RegisterRequest = { username, password };
  const response = await fetch(`${baseUrl}/api/auth/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Registration failed: ${response.status} ${text}`);
  }

  const data: AuthResponse = await response.json();
  return { token: data.token, userId: data.user.id };
}

/**
 * Enable user registration (admin only).
 */
export async function enableRegistration(
  client: ApiClient,
  enabled: boolean = true,
): Promise<void> {
  // First, get current settings from /me endpoint
  const meResponse = await apiRequest<{ user: any; settings: AppSettings }>(
    client,
    "GET",
    "/api/auth/me",
  );
  const currentSettings = meResponse.settings;

  // Update with registration enabled
  const updateBody: UpdateSettingsRequest = {
    registration_enabled: enabled,
    allow_run_commands: currentSettings.allow_run_commands,
    run_command_sandbox: currentSettings.run_command_sandbox,
    run_command_default_timeout_secs:
      currentSettings.run_command_default_timeout_secs,
    run_command_use_namespaces: currentSettings.run_command_use_namespaces,
  };

  await apiRequest(client, "PUT", "/api/auth/settings", updateBody);
}

/**
 * Create a non-admin user for testing.
 * Enables registration, creates the user, then optionally disables registration again.
 */
export async function createNonAdminUser(
  adminClient: ApiClient,
  username: string,
  password: string,
  disableRegistrationAfter: boolean = true,
): Promise<{ token: string; userId: string }> {
  // Enable registration
  await enableRegistration(adminClient, true);

  // Register the user
  const result = await registerUser(adminClient.baseUrl, username, password);

  // Optionally disable registration again
  if (disableRegistrationAfter) {
    await enableRegistration(adminClient, false);
  }

  return result;
}

/**
 * Grant a user permission to access a server.
 */
export async function grantServerPermission(
  adminClient: ApiClient,
  serverId: string,
  userId: string,
  level: "viewer" | "operator" | "manager" | "admin" | "owner",
): Promise<void> {
  const body = { user_id: userId, level };
  await apiRequest(
    adminClient,
    "POST",
    `/api/servers/${serverId}/permissions`,
    body,
  );
}
