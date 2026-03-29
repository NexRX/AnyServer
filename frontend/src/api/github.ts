import { request } from "./core";
import type { GithubReleasesResponse } from "../types/generated/GithubReleasesResponse";
import type { GithubSettingsResponse } from "../types/generated/GithubSettingsResponse";
import type { SaveGithubSettingsRequest } from "../types/generated/SaveGithubSettingsRequest";

/**
 * Fetch GitHub release tags for a repository.
 *
 * @param repo - GitHub repository in "owner/repo" format
 * @returns List of release tags sorted by published date (oldest first)
 */
export async function fetchGithubReleases(
  repo: string,
): Promise<GithubReleasesResponse> {
  return request<GithubReleasesResponse>(
    "GET",
    `/github/releases?repo=${encodeURIComponent(repo)}`,
  );
}

/**
 * Get GitHub settings (admin only).
 * Returns whether a token is configured without revealing the actual token.
 *
 * @returns GitHub settings response with has_token flag
 */
export async function getGithubSettings(): Promise<GithubSettingsResponse> {
  return request<GithubSettingsResponse>("GET", "/admin/settings/github");
}

/**
 * Save GitHub settings (admin only).
 *
 * @param settings - The settings to save (api_token)
 */
export async function saveGithubSettings(
  settings: SaveGithubSettingsRequest,
): Promise<void> {
  await request<void>("PUT", "/admin/settings/github", settings);
}
