import type {
  FileListResponse,
  FileContentResponse,
  FilePermissionsResponse,
  WriteFileRequest,
  CreateDirRequest,
  DeleteRequest,
  ChmodRequest,
} from "../types/bindings";
import { request } from "./core";

export function listFiles(
  serverId: string,
  path?: string,
): Promise<FileListResponse> {
  const params = new URLSearchParams();
  if (path !== undefined && path !== "") {
    params.set("path", path);
  }
  const qs = params.toString();
  return request<FileListResponse>(
    "GET",
    `/servers/${encodeURIComponent(serverId)}/files${qs ? `?${qs}` : ""}`,
  );
}

export function readFile(
  serverId: string,
  path: string,
): Promise<FileContentResponse> {
  const params = new URLSearchParams({ path });
  return request<FileContentResponse>(
    "GET",
    `/servers/${encodeURIComponent(serverId)}/files/read?${params.toString()}`,
  );
}

export function writeFile(
  serverId: string,
  req: WriteFileRequest,
): Promise<{ written: boolean; path: string; size: number }> {
  return request<{ written: boolean; path: string; size: number }>(
    "POST",
    `/servers/${encodeURIComponent(serverId)}/files/write`,
    req,
  );
}

export function createDir(
  serverId: string,
  req: CreateDirRequest,
): Promise<{ created: boolean; path: string }> {
  return request<{ created: boolean; path: string }>(
    "POST",
    `/servers/${encodeURIComponent(serverId)}/files/mkdir`,
    req,
  );
}

export function deletePath(
  serverId: string,
  req: DeleteRequest,
): Promise<{ deleted: boolean; path: string }> {
  return request<{ deleted: boolean; path: string }>(
    "POST",
    `/servers/${encodeURIComponent(serverId)}/files/delete`,
    req,
  );
}

export function getFilePermissions(
  serverId: string,
  path: string,
): Promise<FilePermissionsResponse> {
  const params = new URLSearchParams({ path });
  return request<FilePermissionsResponse>(
    "GET",
    `/servers/${encodeURIComponent(serverId)}/files/permissions?${params.toString()}`,
  );
}

export function chmodFile(
  serverId: string,
  req: ChmodRequest,
): Promise<{ path: string; mode: string; mode_display: string }> {
  return request<{ path: string; mode: string; mode_display: string }>(
    "POST",
    `/servers/${encodeURIComponent(serverId)}/files/chmod`,
    req,
  );
}
