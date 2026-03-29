import { describe, it, expect } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { Router, Route } from "@solidjs/router";
import NotFound from "./NotFound";

describe("NotFound", () => {
  it("renders 404 message", () => {
    render(() => (
      <Router>
        <Route path="*" component={NotFound} />
      </Router>
    ));

    expect(screen.getByText("Page Not Found")).toBeInTheDocument();
  });

  it("displays 404 icon", () => {
    render(() => (
      <Router>
        <Route path="*" component={NotFound} />
      </Router>
    ));

    expect(screen.getByText("404")).toBeInTheDocument();
  });

  it("shows helpful message", () => {
    render(() => (
      <Router>
        <Route path="*" component={NotFound} />
      </Router>
    ));

    expect(
      screen.getByText(
        "The page you're looking for doesn't exist or has been moved.",
      ),
    ).toBeInTheDocument();
  });

  it("has link to dashboard", () => {
    render(() => (
      <Router>
        <Route path="*" component={NotFound} />
      </Router>
    ));

    const dashboardLink = screen.getByRole("link", {
      name: /go to dashboard/i,
    });
    expect(dashboardLink).toBeInTheDocument();
    expect(dashboardLink).toHaveAttribute("href", "/");
  });

  it("has correct CSS classes for styling", () => {
    render(() => (
      <Router>
        <Route path="*" component={NotFound} />
      </Router>
    ));

    const heading = screen.getByText("Page Not Found");
    expect(heading).toHaveClass("not-found");
    expect(heading.tagName).toBe("H1");
  });
});
