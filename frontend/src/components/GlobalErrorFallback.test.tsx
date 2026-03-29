import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { Router, Route } from "@solidjs/router";
import GlobalErrorFallback from "./GlobalErrorFallback";

describe("GlobalErrorFallback", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders error message", () => {
    const error = new Error("Test error message");
    const reset = vi.fn();

    render(() => (
      <Router>
        <Route
          path="*"
          component={() => <GlobalErrorFallback error={error} reset={reset} />}
        />
      </Router>
    ));

    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    expect(screen.getByText("Test error message")).toBeInTheDocument();
  });

  it("renders action buttons", () => {
    const error = new Error("Test error");
    const reset = vi.fn();

    render(() => (
      <Router>
        <Route
          path="*"
          component={() => <GlobalErrorFallback error={error} reset={reset} />}
        />
      </Router>
    ));

    expect(screen.getByText("Try Again")).toBeInTheDocument();
    expect(screen.getByText("Reload Page")).toBeInTheDocument();
    expect(screen.getByText("Go to Dashboard")).toBeInTheDocument();
  });

  it("calls reset function when Try Again is clicked", () => {
    const error = new Error("Test error");
    const reset = vi.fn();

    render(() => (
      <Router>
        <Route
          path="*"
          component={() => <GlobalErrorFallback error={error} reset={reset} />}
        />
      </Router>
    ));

    const tryAgainBtn = screen.getByText("Try Again");
    tryAgainBtn.click();

    expect(reset).toHaveBeenCalledTimes(1);
  });

  it("shows technical details in expandable section", () => {
    const error = new Error("Test error");
    error.stack = "Error stack trace here";
    const reset = vi.fn();

    render(() => (
      <Router>
        <Route
          path="*"
          component={() => <GlobalErrorFallback error={error} reset={reset} />}
        />
      </Router>
    ));

    expect(screen.getByText("Technical Details")).toBeInTheDocument();
  });

  it("handles 404 errors with status property", () => {
    const error = Object.assign(new Error("Server xyz not found"), {
      status: 404,
    });
    const reset = vi.fn();

    render(() => (
      <Router>
        <Route
          path="*"
          component={() => <GlobalErrorFallback error={error} reset={reset} />}
        />
      </Router>
    ));

    expect(screen.getByText("Server xyz not found")).toBeInTheDocument();

    // Note: We can't easily test the actual navigation in unit tests
    // as it requires a full router setup. The redirect logic is tested
    // in E2E tests (server-deletion.spec.ts)
  });

  it("handles 404 errors with 'not found' in message", () => {
    const error = new Error("Resource not found");
    const reset = vi.fn();

    render(() => (
      <Router>
        <Route
          path="*"
          component={() => <GlobalErrorFallback error={error} reset={reset} />}
        />
      </Router>
    ));

    expect(screen.getByText("Resource not found")).toBeInTheDocument();
  });

  it("renders dashboard link with correct href", () => {
    const error = new Error("Test error");
    const reset = vi.fn();

    render(() => (
      <Router>
        <Route
          path="*"
          component={() => <GlobalErrorFallback error={error} reset={reset} />}
        />
      </Router>
    ));

    const dashboardLink = screen.getByText("Go to Dashboard");
    expect(dashboardLink).toHaveAttribute("href", "/");
  });
});
