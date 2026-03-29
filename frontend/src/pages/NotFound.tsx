import { type Component } from "solid-js";
import { A } from "@solidjs/router";

const NotFound: Component = () => {
  return (
    <div class="not-found-page">
      <div class="not-found-card">
        <div class="not-found-icon">404</div>
        <h1 class="not-found" data-testid="not-found-heading">
          Page Not Found
        </h1>
        <p class="not-found-message">
          The page you're looking for doesn't exist or has been moved.
        </p>
        <div class="not-found-actions">
          <A href="/" class="btn btn-primary">
            Go to Dashboard
          </A>
        </div>
      </div>
    </div>
  );
};

export default NotFound;
