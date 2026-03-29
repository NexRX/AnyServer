import type {
  SandboxProfileResponse,
  UpdateSandboxProfileRequest,
  SandboxCapabilities,
  ToggleSandboxFeatureRequest,
  ToggleSandboxFeatureResponse,
} from "../types/bindings";
import { request } from "./core";

// ─── Per-Server Sandbox Profiles ───

export function getSandboxProfile(
  serverId: string,
): Promise<SandboxProfileResponse> {
  return request<SandboxProfileResponse>(
    "GET",
    `/servers/${encodeURIComponent(serverId)}/sandbox`,
  );
}

export function updateSandboxProfile(
  serverId: string,
  req: UpdateSandboxProfileRequest,
): Promise<SandboxProfileResponse> {
  return request<SandboxProfileResponse>(
    "PUT",
    `/servers/${encodeURIComponent(serverId)}/sandbox`,
    req,
  );
}

export function resetSandboxProfile(
  serverId: string,
): Promise<SandboxProfileResponse> {
  return request<SandboxProfileResponse>(
    "DELETE",
    `/servers/${encodeURIComponent(serverId)}/sandbox`,
  );
}

// ─── Admin: Sandbox Feature Flag & Capabilities ───

export function getSandboxCapabilities(): Promise<SandboxCapabilities> {
  return request<SandboxCapabilities>("GET", "/admin/sandbox/capabilities");
}

export function toggleSandboxFeature(
  req: ToggleSandboxFeatureRequest,
): Promise<ToggleSandboxFeatureResponse> {
  return request<ToggleSandboxFeatureResponse>(
    "PUT",
    "/admin/sandbox/feature",
    req,
  );
}
