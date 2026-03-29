import type {
  AlertConfig,
  SaveAlertConfigRequest,
  SmtpConfigPublic,
  SaveSmtpConfigRequest,
  DeleteSmtpConfigResponse,
  TestEmailRequest,
  TestEmailResponse,
  ServerAlertConfig,
  UpdateServerAlertRequest,
} from "../types/bindings";
import { request } from "./core";

export function getSmtpConfig(): Promise<SmtpConfigPublic | null> {
  return request<SmtpConfigPublic | null>("GET", "/admin/smtp");
}

export function saveSmtpConfig(
  req: SaveSmtpConfigRequest,
): Promise<SmtpConfigPublic> {
  return request<SmtpConfigPublic>("PUT", "/admin/smtp", req);
}

export function deleteSmtpConfig(): Promise<DeleteSmtpConfigResponse> {
  return request<DeleteSmtpConfigResponse>("DELETE", "/admin/smtp");
}

export function sendTestEmail(
  req: TestEmailRequest,
): Promise<TestEmailResponse> {
  return request<TestEmailResponse>("POST", "/admin/smtp/test", req);
}

export function getAlertConfig(): Promise<AlertConfig> {
  return request<AlertConfig>("GET", "/admin/alerts");
}

export function saveAlertConfig(
  req: SaveAlertConfigRequest,
): Promise<AlertConfig> {
  return request<AlertConfig>("PUT", "/admin/alerts", req);
}

export function getServerAlerts(serverId: string): Promise<ServerAlertConfig> {
  return request<ServerAlertConfig>(
    "GET",
    `/servers/${encodeURIComponent(serverId)}/alerts`,
  );
}

export function updateServerAlerts(
  serverId: string,
  req: UpdateServerAlertRequest,
): Promise<ServerAlertConfig> {
  return request<ServerAlertConfig>(
    "PUT",
    `/servers/${encodeURIComponent(serverId)}/alerts`,
    req,
  );
}
