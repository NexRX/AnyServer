import { createSignal, type Accessor } from "solid-js";
import { checkForUpdate } from "../api/client";
import type { UpdateCheckResult } from "../types/bindings";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface UseUpdateCheckReturn {
  /** The most recent update check result, or null if not yet checked. */
  updateCheckResult: Accessor<UpdateCheckResult | null>;
  /** Whether an update check is currently in progress. */
  updateChecking: Accessor<boolean>;
  /** Trigger an update check. Pass `true` to force a fresh check (bypass cache). */
  handleCheckForUpdate: (force?: boolean) => Promise<void>;
}

// ─── Hook ───────────────────────────────────────────────────────────────────

/**
 * Manages update-check state for a single server.
 *
 * @param serverId - Accessor returning the current server ID.
 * @param onError  - Callback invoked with an error message when a check fails.
 */
export function useUpdateCheck(
  serverId: Accessor<string>,
  onError: (msg: string) => void,
): UseUpdateCheckReturn {
  const [updateCheckResult, setUpdateCheckResult] =
    createSignal<UpdateCheckResult | null>(null);
  const [updateChecking, setUpdateChecking] = createSignal(false);

  const handleCheckForUpdate = async (force?: boolean) => {
    setUpdateChecking(true);
    try {
      const result = await checkForUpdate(serverId(), force);
      setUpdateCheckResult(result);
    } catch (e: unknown) {
      if (e instanceof Error) {
        onError(`Update check failed: ${e.message}`);
      }
    } finally {
      setUpdateChecking(false);
    }
  };

  return {
    updateCheckResult,
    updateChecking,
    handleCheckForUpdate,
  };
}
