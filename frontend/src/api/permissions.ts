import type {
  ServerPermissionsResponse,
  ServerPermissionEntry,
  SetPermissionRequest,
  RemovePermissionRequest,
} from "../types/bindings";
import { request } from "./core";

export function listPermissions(
  serverId: string,
): Promise<ServerPermissionsResponse> {
  return request<ServerPermissionsResponse>(
    "GET",
    `/servers/${encodeURIComponent(serverId)}/permissions`,
  );
}

export function setPermission(
  serverId: string,
  req: SetPermissionRequest,
): Promise<ServerPermissionEntry> {
  return request<ServerPermissionEntry>(
    "POST",
    `/servers/${encodeURIComponent(serverId)}/permissions`,
    req,
  );
}

export function removePermission(
  serverId: string,
  req: RemovePermissionRequest,
): Promise<{ removed: boolean; user_id: string; server_id: string }> {
  return request<{ removed: boolean; user_id: string; server_id: string }>(
    "POST",
    `/servers/${encodeURIComponent(serverId)}/permissions/remove`,
    req,
  );
}
