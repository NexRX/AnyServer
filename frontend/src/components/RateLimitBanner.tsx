import {
  type Component,
  createSignal,
  createEffect,
  onCleanup,
  Show,
} from "solid-js";
import { rateLimitRetryAt } from "../api/core";

interface Props {
  belowNavbar?: boolean;
}

const RateLimitBanner: Component<Props> = (props) => {
  const [secondsLeft, setSecondsLeft] = createSignal<number>(0);
  const [visible, setVisible] = createSignal(false);
  const [totalSecs, setTotalSecs] = createSignal<number>(0);

  let interval: ReturnType<typeof setInterval> | null = null;

  const clearTimer = () => {
    if (interval !== null) {
      clearInterval(interval);
      interval = null;
    }
  };

  createEffect(() => {
    const retryAt = rateLimitRetryAt();

    if (retryAt === null) {
      clearTimer();
      setVisible(false);
      return;
    }

    const remaining = Math.max(0, retryAt - Date.now());
    const secs = Math.ceil(remaining / 1000);
    setTotalSecs(secs);
    setVisible(true);

    const tick = () => {
      const rem = Math.max(0, retryAt - Date.now());
      const s = Math.ceil(rem / 1000);
      setSecondsLeft(s);

      if (rem <= 0) {
        clearTimer();
        // Keep the banner visible briefly with a "Resuming…" message
        setTimeout(() => setVisible(false), 800);
      }
    };

    tick();
    clearTimer();
    // Tick every 100ms for a smooth countdown (the displayed value
    // only changes once per second, but sub-second precision avoids
    // a visible "stuck on 1" lag).
    interval = setInterval(tick, 100);
  });

  onCleanup(clearTimer);

  const progress = () => {
    const total = totalSecs();
    if (total <= 0) return 0;
    const retryAt = rateLimitRetryAt();
    if (!retryAt) return 0;
    const remaining = Math.max(0, retryAt - Date.now());
    return (remaining / (total * 1000)) * 100;
  };

  return (
    <Show when={visible()}>
      <div
        class="rate-limit-banner"
        classList={{ "rate-limit-banner--below-nav": !!props.belowNavbar }}
        role="alert"
        aria-live="polite"
      >
        <div class="rate-limit-banner-content">
          <span class="rate-limit-icon" aria-hidden="true">
            ⏳
          </span>
          <Show
            when={secondsLeft() > 0}
            fallback={<span class="rate-limit-text">Resuming…</span>}
          >
            <span class="rate-limit-text">
              Too many requests — retrying in{" "}
              <strong class="rate-limit-countdown">{secondsLeft()}s</strong>
            </span>
          </Show>
        </div>
        {/* Animated shrinking progress bar */}
        <div
          class="rate-limit-progress-track"
          role="progressbar"
          aria-valuenow={secondsLeft()}
          aria-valuemin={0}
        >
          <div
            class="rate-limit-progress-bar"
            style={{ width: `${progress()}%` }}
          />
        </div>
      </div>
    </Show>
  );
};

export default RateLimitBanner;
