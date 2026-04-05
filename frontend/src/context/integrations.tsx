import {
  type Component,
  type ParentProps,
  createContext,
  useContext,
  createSignal,
  createEffect,
} from "solid-js";
import type { IntegrationStatus } from "../types/bindings";
import { getIntegrationStatus } from "../api/integrations";
import { useAuth } from "./auth";

/**
 * Default status when integrations haven't been loaded yet.
 * Everything reports as unavailable until we hear otherwise — this
 * prevents briefly flashing features that turn out to be unconfigured.
 */
const DEFAULT_STATUS: IntegrationStatus = {
  curseforge_configured: false,
  github_configured: false,
  steamcmd_available: false,
  smtp_configured: false,
};

export interface IntegrationState {
  /** Current integration availability flags (reactive). */
  status: () => IntegrationStatus;
  /** Whether the initial fetch has completed at least once. */
  loaded: () => boolean;
  /** Whether a fetch is currently in flight. */
  loading: () => boolean;
  /** Force a re-fetch (e.g. after an admin saves new settings). */
  refresh: () => Promise<void>;
}

const IntegrationContext = createContext<IntegrationState>();

export const IntegrationStatusProvider: Component<ParentProps> = (props) => {
  const auth = useAuth();

  const [status, setStatus] = createSignal<IntegrationStatus>(DEFAULT_STATUS);
  const [loaded, setLoaded] = createSignal(false);
  const [loading, setLoading] = createSignal(false);

  const fetchStatus = async () => {
    setLoading(true);
    try {
      const result = await getIntegrationStatus();
      setStatus(result);
      setLoaded(true);
    } catch (err: unknown) {
      // If the request fails (e.g. not logged in, network error),
      // keep whatever we had before.  On first load this means
      // everything stays disabled — which is the safe default.
      console.warn(
        "[Integrations] Failed to fetch integration status:",
        err,
      );
    } finally {
      setLoading(false);
    }
  };

  // Reactively fetch whenever the user logs in (or auth state changes).
  // When the user logs out, reset to defaults.
  createEffect(() => {
    if (auth.isLoggedIn()) {
      fetchStatus();
    } else {
      setStatus(DEFAULT_STATUS);
      setLoaded(false);
    }
  });

  const state: IntegrationState = {
    status,
    loaded,
    loading,
    refresh: fetchStatus,
  };

  return (
    <IntegrationContext.Provider value={state}>
      {props.children}
    </IntegrationContext.Provider>
  );
};

/**
 * Access the global integration status from any component.
 *
 * Must be used within an `<IntegrationStatusProvider>`.
 */
export function useIntegrationStatus(): IntegrationState {
  const ctx = useContext(IntegrationContext);
  if (!ctx) {
    throw new Error(
      "useIntegrationStatus() must be used within an <IntegrationStatusProvider>",
    );
  }
  return ctx;
}
