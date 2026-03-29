import { type Component } from "solid-js";

export interface FloatingToastProps {
  message: string;
  type: "error" | "success";
  timerDuration: number;
  timerKey: number;
  onDismiss: () => void;
  onPause: () => void;
  onResume: () => void;
  onScrollTo: () => void;
}

const FloatingToast: Component<FloatingToastProps> = (props) => {
  return (
    <div
      class={`error-toast-floating error-toast-floating--${props.type}`}
      role="alert"
      aria-live="assertive"
      onMouseEnter={props.onPause}
      onMouseLeave={props.onResume}
    >
      <div class="error-toast-content">
        <span class="error-toast-icon" aria-hidden="true">
          {props.type === "error" ? "⚠️" : "✓"}
        </span>
        <span class="error-toast-text">{props.message}</span>
        <button
          class="error-toast-dismiss"
          onClick={props.onDismiss}
          aria-label={`Dismiss ${props.type}`}
        >
          ✕
        </button>
      </div>
      <button class="error-toast-scroll-btn" onClick={props.onScrollTo}>
        ↑ Scroll to {props.type === "error" ? "error" : "message"}
      </button>
      <div
        class={`toast-timer-bar toast-timer-bar--${props.type}`}
        style={{ "animation-duration": `${props.timerDuration}ms` }}
        data-key={props.timerKey}
      />
    </div>
  );
};

export default FloatingToast;
