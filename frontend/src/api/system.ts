import type {
  SystemHealth,
  JavaRuntimesResponse,
  DotnetRuntimesResponse,
  SteamCmdStatusResponse,
  ValidateAppResponse,
} from "../types/bindings";
import { request } from "./core";

export function getSystemHealth(): Promise<SystemHealth> {
  return request<SystemHealth>("GET", "/system/health");
}

export function getJavaRuntimes(): Promise<JavaRuntimesResponse> {
  return request<JavaRuntimesResponse>("GET", "/system/java-runtimes");
}

export function getDotnetRuntimes(): Promise<DotnetRuntimesResponse> {
  return request<DotnetRuntimesResponse>("GET", "/system/dotnet-runtimes");
}

export function getJavaEnv(javaHome: string): Promise<Record<string, string>> {
  const params = new URLSearchParams({ java_home: javaHome });
  return request<Record<string, string>>("GET", `/system/java-env?${params}`);
}

export function getDotnetEnv(
  installationRoot: string,
  serverDir?: string,
): Promise<Record<string, string>> {
  const params = new URLSearchParams({ installation_root: installationRoot });
  if (serverDir) {
    params.set("server_dir", serverDir);
  }
  return request<Record<string, string>>("GET", `/system/dotnet-env?${params}`);
}

export function getSteamCmdStatus(): Promise<SteamCmdStatusResponse> {
  return request<SteamCmdStatusResponse>("GET", "/system/steamcmd-status");
}

export function validateSteamApp(appId: number): Promise<ValidateAppResponse> {
  return request<ValidateAppResponse>(
    "GET",
    `/steamcmd/validate-app?app_id=${appId}`,
  );
}
