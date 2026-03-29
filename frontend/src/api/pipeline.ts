import type {
  RunPhaseRequest,
  RunPhaseResponse,
  PhaseStatusResponse,
  ServerRuntime,
} from "../types/bindings";
import { request } from "./core";

export function installServer(
  id: string,
  req?: RunPhaseRequest | null,
): Promise<RunPhaseResponse> {
  return request<RunPhaseResponse>(
    "POST",
    `/servers/${encodeURIComponent(id)}/install`,
    req ?? null,
  );
}

export function updateServerPipeline(
  id: string,
  req?: RunPhaseRequest | null,
): Promise<RunPhaseResponse> {
  return request<RunPhaseResponse>(
    "POST",
    `/servers/${encodeURIComponent(id)}/update`,
    req ?? null,
  );
}

export function getPhaseStatus(id: string): Promise<PhaseStatusResponse> {
  return request<PhaseStatusResponse>(
    "GET",
    `/servers/${encodeURIComponent(id)}/phase-status`,
  );
}

export function cancelPhase(
  id: string,
): Promise<{ cancelled: boolean; server_id: string }> {
  return request<{ cancelled: boolean; server_id: string }>(
    "POST",
    `/servers/${encodeURIComponent(id)}/cancel-phase`,
  );
}

export function uninstallServer(
  id: string,
  req?: RunPhaseRequest | null,
): Promise<RunPhaseResponse> {
  return request<RunPhaseResponse>(
    "POST",
    `/servers/${encodeURIComponent(id)}/uninstall`,
    req ?? null,
  );
}

export function killServer(id: string): Promise<ServerRuntime> {
  return request<ServerRuntime>(
    "POST",
    `/servers/${encodeURIComponent(id)}/kill`,
  );
}
