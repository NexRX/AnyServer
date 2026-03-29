import { createSignal, createEffect, onCleanup, type Accessor } from "solid-js";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface UseToastNotificationsReturn {
  // ── State ──
  /** Current error message, or null. */
  actionError: Accessor<string | null>;
  /** Current success message, or null. */
  actionSuccess: Accessor<string | null>;
  /** Whether the inline error element is visible in the viewport. */
  errorVisible: Accessor<boolean>;
  /** Whether the inline success element is visible in the viewport. */
  successVisible: Accessor<boolean>;
  /** Current animation duration for the error timer bar (ms). */
  errorTimerDuration: Accessor<number>;
  /** Current animation duration for the success timer bar (ms). */
  successTimerDuration: Accessor<number>;
  /** Key that changes each time the error timer restarts (forces CSS animation reset). */
  errorTimerKey: Accessor<number>;
  /** Key that changes each time the success timer restarts (forces CSS animation reset). */
  successTimerKey: Accessor<number>;

  // ── Ref setters ──
  /** Call with the inline error DOM element so IntersectionObserver can track it. */
  setErrorInlineRef: (el: HTMLDivElement) => void;
  /** Call with the inline success DOM element so IntersectionObserver can track it. */
  setSuccessInlineRef: (el: HTMLDivElement) => void;
  /** Get the current error inline ref (for scrollIntoView). */
  errorInlineRef: () => HTMLDivElement | undefined;
  /** Get the current success inline ref (for scrollIntoView). */
  successInlineRef: () => HTMLDivElement | undefined;

  // ── Actions ──
  /** Show an error toast with auto-dismiss. */
  showError: (msg: string) => void;
  /** Show a success toast with auto-dismiss. */
  showSuccess: (msg: string) => void;
  /** Immediately dismiss the error toast. */
  dismissError: () => void;
  /** Immediately dismiss the success toast. */
  dismissSuccess: () => void;
  /** Pause the error auto-dismiss timer (call on mouseenter). */
  pauseErrorTimer: () => void;
  /** Resume the error auto-dismiss timer (call on mouseleave). */
  resumeErrorTimer: () => void;
  /** Pause the success auto-dismiss timer (call on mouseenter). */
  pauseSuccessTimer: () => void;
  /** Resume the success auto-dismiss timer (call on mouseleave). */
  resumeSuccessTimer: () => void;
}

// ─── Default durations ──────────────────────────────────────────────────────

const DEFAULT_ERROR_DURATION = 8000;
const DEFAULT_SUCCESS_DURATION = 4000;

// ─── Hook ───────────────────────────────────────────────────────────────────

export function useToastNotifications(): UseToastNotificationsReturn {
  // ── Core message signals ──
  const [actionError, setActionError] = createSignal<string | null>(null);
  const [actionSuccess, setActionSuccess] = createSignal<string | null>(null);

  // ── IntersectionObserver visibility ──
  const [errorVisible, setErrorVisible] = createSignal(true);
  const [successVisible, setSuccessVisible] = createSignal(true);

  // ── Timer animation state ──
  const [errorTimerDuration, setErrorTimerDuration] = createSignal(DEFAULT_ERROR_DURATION);
  const [successTimerDuration, setSuccessTimerDuration] = createSignal(DEFAULT_SUCCESS_DURATION);
  const [errorTimerKey, setErrorTimerKey] = createSignal(0);
  const [successTimerKey, setSuccessTimerKey] = createSignal(0);

  // ── Mutable tracking for pause/resume ──
  let errorStartTime = 0;
  let errorRemainingMs = DEFAULT_ERROR_DURATION;
  let successStartTime = 0;
  let successRemainingMs = DEFAULT_SUCCESS_DURATION;

  // ── Auto-dismiss timers ──
  let errorDismissTimer: ReturnType<typeof setTimeout> | null = null;
  let successDismissTimer: ReturnType<typeof setTimeout> | null = null;

  // ── DOM refs for IntersectionObserver ──
  let _errorInlineRef: HTMLDivElement | undefined;
  let _successInlineRef: HTMLDivElement | undefined;

  const setErrorInlineRef = (el: HTMLDivElement) => {
    _errorInlineRef = el;
  };

  const setSuccessInlineRef = (el: HTMLDivElement) => {
    _successInlineRef = el;
  };

  const errorInlineRef = () => _errorInlineRef;
  const successInlineRef = () => _successInlineRef;

  // ── IntersectionObserver for error inline ref ──
  createEffect(() => {
    if (!actionError()) {
      setErrorVisible(true);
      return;
    }
    // Wait a tick for the ref to mount
    requestAnimationFrame(() => {
      if (!_errorInlineRef) return;
      const observer = new IntersectionObserver(
        ([entry]) => setErrorVisible(entry.isIntersecting),
        { threshold: 0.1 },
      );
      observer.observe(_errorInlineRef);
      onCleanup(() => observer.disconnect());
    });
  });

  // ── IntersectionObserver for success inline ref ──
  createEffect(() => {
    if (!actionSuccess()) {
      setSuccessVisible(true);
      return;
    }
    requestAnimationFrame(() => {
      if (!_successInlineRef) return;
      const observer = new IntersectionObserver(
        ([entry]) => setSuccessVisible(entry.isIntersecting),
        { threshold: 0.1 },
      );
      observer.observe(_successInlineRef);
      onCleanup(() => observer.disconnect());
    });
  });

  // ── Actions ──

  const showError = (msg: string) => {
    if (errorDismissTimer) clearTimeout(errorDismissTimer);
    setActionError(msg);
    errorStartTime = Date.now();
    errorRemainingMs = DEFAULT_ERROR_DURATION;
    setErrorTimerDuration(DEFAULT_ERROR_DURATION);
    setErrorTimerKey((k) => k + 1);
    errorDismissTimer = setTimeout(() => setActionError(null), DEFAULT_ERROR_DURATION);
  };

  const showSuccess = (msg: string) => {
    if (successDismissTimer) clearTimeout(successDismissTimer);
    setActionSuccess(msg);
    successStartTime = Date.now();
    successRemainingMs = DEFAULT_SUCCESS_DURATION;
    setSuccessTimerDuration(DEFAULT_SUCCESS_DURATION);
    setSuccessTimerKey((k) => k + 1);
    successDismissTimer = setTimeout(() => setActionSuccess(null), DEFAULT_SUCCESS_DURATION);
  };

  const dismissError = () => {
    if (errorDismissTimer) clearTimeout(errorDismissTimer);
    setActionError(null);
  };

  const dismissSuccess = () => {
    if (successDismissTimer) clearTimeout(successDismissTimer);
    setActionSuccess(null);
  };

  const pauseErrorTimer = () => {
    if (errorDismissTimer) clearTimeout(errorDismissTimer);
    const elapsed = Date.now() - errorStartTime;
    errorRemainingMs = Math.max(0, errorRemainingMs - elapsed);
  };

  const resumeErrorTimer = () => {
    if (errorRemainingMs > 0 && actionError()) {
      errorStartTime = Date.now();
      setErrorTimerDuration(errorRemainingMs);
      setErrorTimerKey((k) => k + 1);
      errorDismissTimer = setTimeout(() => setActionError(null), errorRemainingMs);
    }
  };

  const pauseSuccessTimer = () => {
    if (successDismissTimer) clearTimeout(successDismissTimer);
    const elapsed = Date.now() - successStartTime;
    successRemainingMs = Math.max(0, successRemainingMs - elapsed);
  };

  const resumeSuccessTimer = () => {
    if (successRemainingMs > 0 && actionSuccess()) {
      successStartTime = Date.now();
      setSuccessTimerDuration(successRemainingMs);
      setSuccessTimerKey((k) => k + 1);
      successDismissTimer = setTimeout(() => setActionSuccess(null), successRemainingMs);
    }
  };

  // ── Cleanup ──

  onCleanup(() => {
    if (errorDismissTimer) clearTimeout(errorDismissTimer);
    if (successDismissTimer) clearTimeout(successDismissTimer);
  });

  return {
    actionError,
    actionSuccess,
    errorVisible,
    successVisible,
    errorTimerDuration,
    successTimerDuration,
    errorTimerKey,
    successTimerKey,
    setErrorInlineRef,
    setSuccessInlineRef,
    errorInlineRef,
    successInlineRef,
    showError,
    showSuccess,
    dismissError,
    dismissSuccess,
    pauseErrorTimer,
    resumeErrorTimer,
    pauseSuccessTimer,
    resumeSuccessTimer,
  };
}
