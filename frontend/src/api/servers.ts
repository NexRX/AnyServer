import type {
  ServerListResponse,
  PaginatedServerListResponse,
  ServerWithStatus,
  CreateServerRequest,
  UpdateServerRequest,
  ServerRuntime,
  SendCommandRequest,
  ServerResourceStats,
  UpdateCheckResult,
  UpdateCheckStatusResponse,
  MarkInstalledResponse,
} from "../types/bindings";
import { request } from "./core";

export interface ListServersParams {
  page?: number;
  per_page?: number;
  search?: string;
  status?: string;
  sort?: string;
  order?: string;
}

export function listServers(
  params?: ListServersParams,
): Promise<PaginatedServerListResponse> {
  const query = new URLSearchParams();
  if (params) {
    if (params.page !== undefined) query.set("page", params.page.toString());
    if (params.per_page !== undefined)
      query.set("per_page", params.per_page.toString());
    if (params.search) query.set("search", params.search);
    if (params.status) query.set("status", params.status);
    if (params.sort) query.set("sort", params.sort);
    if (params.order) query.set("order", params.order);
  }
  const queryString = query.toString();
  const endpoint = queryString ? `/servers?${queryString}` : "/servers";
  return request<PaginatedServerListResponse>("GET", endpoint);
}

export function getServer(id: string): Promise<ServerWithStatus> {
  return request<ServerWithStatus>("GET", `/servers/${encodeURIComponent(id)}`);
}

export function createServer(
  req: CreateServerRequest,
): Promise<ServerWithStatus> {
  return request<ServerWithStatus>("POST", "/servers", req);
}

export function updateServer(
  id: string,
  req: UpdateServerRequest,
): Promise<ServerWithStatus> {
  return request<ServerWithStatus>(
    "PUT",
    `/servers/${encodeURIComponent(id)}`,
    req,
  );
}

export function deleteServer(
  id: string,
): Promise<{ deleted: boolean; id: string }> {
  return request<{ deleted: boolean; id: string }>(
    "DELETE",
    `/servers/${encodeURIComponent(id)}`,
  );
}

export function startServer(id: string): Promise<ServerRuntime> {
  return request<ServerRuntime>(
    "POST",
    `/servers/${encodeURIComponent(id)}/start`,
  );
}

export function stopServer(id: string): Promise<ServerRuntime> {
  return request<ServerRuntime>(
    "POST",
    `/servers/${encodeURIComponent(id)}/stop`,
  );
}

export function restartServer(id: string): Promise<ServerRuntime> {
  return request<ServerRuntime>(
    "POST",
    `/servers/${encodeURIComponent(id)}/restart`,
  );
}

export function cancelRestart(id: string): Promise<ServerRuntime> {
  return request<ServerRuntime>(
    "POST",
    `/servers/${encodeURIComponent(id)}/cancel-restart`,
  );
}

export function sendCommand(
  id: string,
  command: string,
): Promise<{ sent: boolean; command: string }> {
  const body: SendCommandRequest = { command };
  return request<{ sent: boolean; command: string }>(
    "POST",
    `/servers/${encodeURIComponent(id)}/command`,
    body,
  );
}

export function cancelStop(
  id: string,
): Promise<{ cancelled: boolean; server_id: string }> {
  return request<{ cancelled: boolean; server_id: string }>(
    "POST",
    `/servers/${encodeURIComponent(id)}/cancel-stop`,
  );
}

export function sendSigint(
  id: string,
): Promise<{ sent: boolean; signal: string; pid: number }> {
  return request<{ sent: boolean; signal: string; pid: number }>(
    "POST",
    `/servers/${encodeURIComponent(id)}/sigint`,
  );
}

export function resetServer(
  id: string,
): Promise<{ reset: boolean; id: string; killed_processes: number }> {
  return request<{ reset: boolean; id: string; killed_processes: number }>(
    "POST",
    `/servers/${encodeURIComponent(id)}/reset`,
  );
}

export function listDirectoryProcesses(id: string): Promise<{
  count: number;
  processes: Array<{ pid: number; command: string; args: string[] }>;
}> {
  return request<{
    count: number;
    processes: Array<{ pid: number; command: string; args: string[] }>;
  }>("GET", `/servers/${encodeURIComponent(id)}/directory-processes`);
}

export function getServerStats(id: string): Promise<ServerResourceStats> {
  return request<ServerResourceStats>(
    "GET",
    `/servers/${encodeURIComponent(id)}/stats`,
  );
}

export function killDirectoryProcesses(id: string): Promise<{
  killed: number;
  failed: number;
  processes: Array<{ pid: number; command: string; success: boolean }>;
}> {
  return request<{
    killed: number;
    failed: number;
    processes: Array<{ pid: number; command: string; success: boolean }>;
  }>("POST", `/servers/${encodeURIComponent(id)}/kill-directory-processes`);
}

export function checkForUpdate(
  id: string,
  force?: boolean,
): Promise<UpdateCheckResult> {
  const query = force ? "?force=true" : "";
  return request<UpdateCheckResult>(
    "GET",
    `/servers/${encodeURIComponent(id)}/check-update${query}`,
  );
}

export function getUpdateStatus(): Promise<UpdateCheckStatusResponse> {
  return request<UpdateCheckStatusResponse>("GET", "/servers/update-status");
}

export function markInstalled(id: string): Promise<MarkInstalledResponse> {
  return request<MarkInstalledResponse>(
    "POST",
    `/servers/${encodeURIComponent(id)}/mark-installed`,
  );
}
