import type {
  AppSettings,
  AuthResponse,
  LoginRequest,
  RegisterRequest,
  SetupRequest,
  MeResponse,
  ChangePasswordRequest,
  ChangePasswordResponse,
  UpdateSettingsRequest,
  LogoutEverywhereResponse,
  SessionListResponse,
  RevokeSessionRequest,
  RevokeSessionResponse,
  WsTicketRequest,
  WsTicketResponse,
} from "../types/bindings";
import { request, setToken, clearToken } from "./core";

export function getAuthStatus(): Promise<AppSettings> {
  return request<AppSettings>("GET", "/auth/status", undefined, {
    noAuth: true,
  });
}

export async function setup(req: SetupRequest): Promise<AuthResponse> {
  const resp = await request<AuthResponse>("POST", "/auth/setup", req, {
    noAuth: true,
  });
  setToken(resp.token);
  return resp;
}

export async function login(req: LoginRequest): Promise<AuthResponse> {
  const resp = await request<AuthResponse>("POST", "/auth/login", req, {
    noAuth: true,
  });
  setToken(resp.token);
  return resp;
}

export async function register(req: RegisterRequest): Promise<AuthResponse> {
  const resp = await request<AuthResponse>("POST", "/auth/register", req, {
    noAuth: true,
  });
  setToken(resp.token);
  return resp;
}

export async function logout(): Promise<void> {
  try {
    await request("POST", "/auth/logout", undefined, { noAuth: true });
  } catch (error) {
    console.warn("Logout request failed:", error);
  } finally {
    clearToken();
  }
}

export function getMe(): Promise<MeResponse> {
  return request<MeResponse>("GET", "/auth/me");
}

export async function changePassword(
  req: ChangePasswordRequest,
): Promise<ChangePasswordResponse> {
  const resp = await request<ChangePasswordResponse>(
    "POST",
    "/auth/change-password",
    req,
  );
  setToken(resp.token);
  return resp;
}

export async function logoutEverywhere(): Promise<LogoutEverywhereResponse> {
  const resp = await request<LogoutEverywhereResponse>(
    "POST",
    "/auth/logout-everywhere",
  );
  setToken(resp.token);
  return resp;
}

export function updateSettings(
  req: UpdateSettingsRequest,
): Promise<AppSettings> {
  return request<AppSettings>("PUT", "/auth/settings", req);
}

export function listSessions(): Promise<SessionListResponse> {
  return request<SessionListResponse>("GET", "/auth/sessions");
}

export function revokeSession(
  req: RevokeSessionRequest,
): Promise<RevokeSessionResponse> {
  return request<RevokeSessionResponse>("POST", "/auth/sessions/revoke", req);
}

export function getWsTicket(scope?: string): Promise<WsTicketResponse> {
  const req: WsTicketRequest = { scope: scope ?? null };
  return request<WsTicketResponse>("POST", "/auth/ws-ticket", req);
}
