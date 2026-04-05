import { request } from "./core";
import type { CurseForgeFilesResponse } from "../types/generated/CurseForgeFilesResponse";
import type { CurseForgeSettingsResponse } from "../types/generated/CurseForgeSettingsResponse";
import type { SaveCurseForgeSettingsRequest } from "../types/generated/SaveCurseForgeSettingsRequest";

/**
 * Fetch available file versions for a CurseForge project.
 *
 * Returns value/label pairs suitable for dropdown population.
 * Requires the CurseForge API key to be configured by an admin.
 *
 * @param projectId - CurseForge project (mod) ID
 * @returns List of file versions with display names
 */
export async function fetchCurseForgeFiles(
  projectId: number,
): Promise<CurseForgeFilesResponse> {
  return request<CurseForgeFilesResponse>(
    "GET",
    `/curseforge/files?project_id=${encodeURIComponent(projectId)}`,
  );
}

/**
 * Get CurseForge settings (admin only).
 * Returns whether an API key is configured without revealing the actual key.
 *
 * @returns CurseForge settings response with has_key flag
 */
export async function getCurseForgeSettings(): Promise<CurseForgeSettingsResponse> {
  return request<CurseForgeSettingsResponse>(
    "GET",
    "/admin/settings/curseforge",
  );
}

/**
 * Save CurseForge settings (admin only).
 *
 * @param settings - The settings to save (api_key)
 */
export async function saveCurseForgeSettings(
  settings: SaveCurseForgeSettingsRequest,
): Promise<void> {
  await request<void>("PUT", "/admin/settings/curseforge", settings);
}
