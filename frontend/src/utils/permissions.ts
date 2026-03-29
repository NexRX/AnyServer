/**
 * Shared permission helpers for the AnyServer frontend.
 *
 * These helpers encapsulate the role-based access control logic so that
 * UI components can gate buttons, inputs, and edit controls based on the
 * user's effective permission level on a given server.
 *
 * The canonical permission hierarchy (lowest → highest):
 *
 *   viewer < operator < manager < admin < owner
 *
 * A **global admin** (`is_global_admin: true`) always has full access
 * regardless of the server-level permission.
 */

import type { EffectivePermission, PermissionLevel } from "../types/bindings";

// ─── Level Ranking ────────────────────────────────────────────────────

export const LEVEL_RANK: Record<PermissionLevel, number> = {
  viewer: 0,
  operator: 1,
  manager: 2,
  admin: 3,
  owner: 4,
};

// ─── Level Descriptions ───────────────────────────────────────────────

export const LEVEL_DESCRIPTIONS: Record<PermissionLevel, string> = {
  viewer:
    "View server status, logs, files, and configuration. Read-only access — cannot start, stop, or modify anything.",
  operator:
    "Everything a Viewer can do, plus start, stop, restart the server and send console commands.",
  manager:
    "Everything an Operator can do, plus edit configuration, manage files (create, edit, delete), and run install/update pipelines.",
  admin:
    "Everything a Manager can do, plus edit pipeline definitions, manage server access permissions, and delete the server.",
  owner:
    "Full control over the server. Same as Admin but also owns the server record.",
};

// ─── Level Colors ─────────────────────────────────────────────────────

export const LEVEL_COLORS: Record<PermissionLevel, string> = {
  viewer: "#9ca3af",
  operator: "#60a5fa",
  manager: "#a78bfa",
  admin: "#f59e0b",
  owner: "#22c55e",
};

// ─── Permission Check Helpers ─────────────────────────────────────────

function hasMinLevel(
  permission: EffectivePermission,
  minLevel: PermissionLevel,
): boolean {
  if (permission.is_global_admin) return true;
  return (LEVEL_RANK[permission.level] ?? 0) >= LEVEL_RANK[minLevel];
}

/**
 * Can the user start, stop, restart, kill the server and send console
 * commands?
 *
 * Requires **Operator** or higher.
 */
export function canControl(permission: EffectivePermission): boolean {
  return hasMinLevel(permission, "operator");
}

/**
 * Can the user run install, update, and uninstall pipelines?
 *
 * Requires **Manager** or higher.
 */
export function canRunPipelines(permission: EffectivePermission): boolean {
  return hasMinLevel(permission, "manager");
}

/**
 * Can the user edit server configuration, pipeline definitions, and
 * manage server access permissions?
 *
 * Requires **Admin** or higher.
 */
export function canEditConfig(permission: EffectivePermission): boolean {
  return hasMinLevel(permission, "admin");
}

/**
 * Can the user delete the server?
 *
 * Requires **Admin** or higher.
 */
export function canDeleteServer(permission: EffectivePermission): boolean {
  return hasMinLevel(permission, "admin");
}
