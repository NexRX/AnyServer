import type { IntegrationStatus } from "../types/bindings";
import { request } from "./core";

/**
 * Fetch the unified integration/feature availability status.
 *
 * Accessible to any authenticated user (not just admins). Returns which
 * integrations have been configured by an admin so the frontend can
 * proactively disable or annotate features that aren't set up yet.
 *
 * Never exposes secrets — only boolean availability flags.
 */
export function getIntegrationStatus(): Promise<IntegrationStatus> {
  return request<IntegrationStatus>("GET", "/integrations/status");
}
