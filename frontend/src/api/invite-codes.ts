import type {
  CreateInviteCodeRequest,
  CreateInviteCodeResponse,
  InviteCodeListResponse,
  InviteCodePublic,
  UpdateInvitePermissionsRequest,
  RedeemInviteCodeRequest,
  RedeemInviteCodeResponse,
  DeleteInviteCodeResponse,
  UserPermissionListResponse,
} from "../types/bindings";
import { request, setToken } from "./core";

// ─── Admin: Invite Code Management ───

export function createInviteCode(
  req: CreateInviteCodeRequest,
): Promise<CreateInviteCodeResponse> {
  return request<CreateInviteCodeResponse>(
    "POST",
    "/admin/invite-codes",
    req,
  );
}

export function listInviteCodes(): Promise<InviteCodeListResponse> {
  return request<InviteCodeListResponse>("GET", "/admin/invite-codes");
}

export function getInviteCode(id: string): Promise<InviteCodePublic> {
  return request<InviteCodePublic>(
    "GET",
    `/admin/invite-codes/${encodeURIComponent(id)}`,
  );
}

export function updateInvitePermissions(
  id: string,
  req: UpdateInvitePermissionsRequest,
): Promise<InviteCodePublic> {
  return request<InviteCodePublic>(
    "PUT",
    `/admin/invite-codes/${encodeURIComponent(id)}/permissions`,
    req,
  );
}

export function deleteInviteCode(id: string): Promise<DeleteInviteCodeResponse> {
  return request<DeleteInviteCodeResponse>(
    "DELETE",
    `/admin/invite-codes/${encodeURIComponent(id)}`,
  );
}

// ─── Admin: User Permission Management ───

export function listUserPermissions(): Promise<UserPermissionListResponse> {
  return request<UserPermissionListResponse>("GET", "/admin/user-permissions");
}

// ─── Public: Redeem Invite Code ───

export async function redeemInviteCode(
  req: RedeemInviteCodeRequest,
): Promise<RedeemInviteCodeResponse> {
  const resp = await request<RedeemInviteCodeResponse>(
    "POST",
    "/auth/redeem-invite",
    req,
    { noAuth: true },
  );
  setToken(resp.token);
  return resp;
}
