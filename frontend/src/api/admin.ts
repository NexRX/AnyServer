import type {
  UserListResponse,
  UserPublic,
  UpdateUserRoleRequest,
} from "../types/bindings";
import { request } from "./core";

export function listUsers(): Promise<UserListResponse> {
  return request<UserListResponse>("GET", "/admin/users");
}

/**
 * Search users by username substring. Accessible to any authenticated user.
 * Returns up to 20 matches, prefix matches sorted first.
 */
export function searchUsers(query: string): Promise<UserListResponse> {
  return request<UserListResponse>(
    "GET",
    `/users/search?q=${encodeURIComponent(query)}`,
  );
}

export function getUser(id: string): Promise<UserPublic> {
  return request<UserPublic>("GET", `/admin/users/${encodeURIComponent(id)}`);
}

export function updateUserRole(
  id: string,
  req: UpdateUserRoleRequest,
): Promise<UserPublic> {
  return request<UserPublic>(
    "PUT",
    `/admin/users/${encodeURIComponent(id)}/role`,
    req,
  );
}

export function deleteUser(
  id: string,
): Promise<{ deleted: boolean; id: string; username: string }> {
  return request<{ deleted: boolean; id: string; username: string }>(
    "DELETE",
    `/admin/users/${encodeURIComponent(id)}`,
  );
}
