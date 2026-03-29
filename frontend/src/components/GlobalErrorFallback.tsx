import { Component, createEffect } from "solid-js";
import { useNavigate } from "@solidjs/router";

interface Props {
  error: Error;
  reset: () => void;
}

const GlobalErrorFallback: Component<Props> = (props) => {
  const navigate = useNavigate();

  createEffect(() => {
    const err = props.error as any;
    const status = err.status;
    const message = err.message || "";

    // Session expired — redirect to login immediately
    if (status === 401) {
      navigate("/login?reason=session_expired", { replace: true });
      return;
    }

    if (status === 404 || message.toLowerCase().includes("not found")) {
      setTimeout(() => {
        navigate("/", { replace: true });
      }, 2000);
    }
  });

  return (
    <div class="global-error-fallback">
      <div class="global-error-content">
        <div class="global-error-icon">⚠️</div>
        <h1>Something went wrong</h1>
        <p class="error-message">{props.error.message}</p>
        <div class="error-actions">
          <button onClick={props.reset} class="btn btn-primary">
            Try Again
          </button>
          <button
            onClick={() => window.location.reload()}
            class="btn btn-secondary"
          >
            Reload Page
          </button>
          <a href="/" class="btn btn-secondary">
            Go to Dashboard
          </a>
        </div>
        <details class="error-details">
          <summary>Technical Details</summary>
          <pre>{props.error.stack}</pre>
        </details>
      </div>
    </div>
  );
};

export default GlobalErrorFallback;
