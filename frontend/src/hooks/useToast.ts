import { createSignal, onCleanup } from "solid-js";

export interface ToastState {
  error: string | null;
  success: string | null;
}

export interface ToastActions {
  showError: (msg: string) => void;
  showSuccess: (msg: string) => void;
  dismissError: () => void;
  dismissSuccess: () => void;
}

export function useToast(
  errorDuration = 8000,
  successDuration = 4000,
): [() => ToastState, ToastActions] {
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);

  let errorTimer: ReturnType<typeof setTimeout> | null = null;
  let successTimer: ReturnType<typeof setTimeout> | null = null;

  const showError = (msg: string) => {
    if (errorTimer) clearTimeout(errorTimer);
    setError(msg);
    errorTimer = setTimeout(() => setError(null), errorDuration);
  };

  const showSuccess = (msg: string) => {
    if (successTimer) clearTimeout(successTimer);
    setSuccess(msg);
    successTimer = setTimeout(() => setSuccess(null), successDuration);
  };

  const dismissError = () => {
    if (errorTimer) clearTimeout(errorTimer);
    setError(null);
  };

  const dismissSuccess = () => {
    if (successTimer) clearTimeout(successTimer);
    setSuccess(null);
  };

  onCleanup(() => {
    if (errorTimer) clearTimeout(errorTimer);
    if (successTimer) clearTimeout(successTimer);
  });

  const getState = () => ({
    error: error(),
    success: success(),
  });

  return [
    getState,
    {
      showError,
      showSuccess,
      dismissError,
      dismissSuccess,
    },
  ];
}
