import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

// ─── Ticket 009: Remove "New Server" from Navbar ────────────────────

describe("Navbar no longer contains 'New Server' link (ticket 009)", () => {
  const source = loadComponent("App.tsx");
  const navbarMatch = source.match(
    /<nav\s+class="navbar"[^>]*>([\s\S]*?)<\/nav>/,
  );
  const navbarSource = navbarMatch?.[1] ?? "";

  it("has a navbar section in the source", () => {
    expect(navbarSource.length).toBeGreaterThan(0);
  });

  it("does not contain 'New Server' link or /create route in the navbar", () => {
    expect(navbarSource).not.toContain("New Server");
    expect(navbarSource).not.toMatch(/<A\s[^>]*href=["']\/create["']/);
  });

  it("still contains Dashboard, Templates, Health, and Admin links", () => {
    for (const [label, href] of [
      ["Dashboard", "/"],
      ["Templates", "/templates"],
      ["Health", "/health"],
      ["Admin", "/admin"],
    ]) {
      expect(navbarSource, `missing ${label} link`).toContain(label);
      expect(navbarSource, `missing href=${href}`).toMatch(
        new RegExp(`<A\\s[^>]*href=["']${href.replace("/", "\\/")}["']`),
      );
    }
  });

  it("has at least 3 nav links for a balanced layout", () => {
    const navLinksMatch = navbarSource.match(
      /<div\s+class="nav-links[^"]*">([\s\S]*?)<\/div>/,
    );
    const linkCount = (navLinksMatch?.[1]?.match(/<A\s/g) || []).length;
    expect(linkCount).toBeGreaterThanOrEqual(3);
  });
});

// ─── Dashboard still has CTA for server creation (ticket 009) ───────

describe("Dashboard has a 'New Server' CTA button (ticket 009)", () => {
  const source = loadComponent("pages/Dashboard.tsx");

  it("has a prominent '+ New Server' CTA linking to /create with btn-primary", () => {
    // The page-header contains nested divs (e.g. refresh-btn-group), so
    // instead of a fragile non-greedy match up to the first </div>, just
    // verify the source file as a whole contains the expected elements.
    expect(source).toContain("+ New Server");
    expect(source).toMatch(/<A\s[^>]*href=["']\/create["']/);

    const ctaMatch = source.match(/<A\s[^>]*href=["']\/create["'][^>]*>/g);
    expect(ctaMatch).not.toBeNull();
    expect(ctaMatch!.some((tag) => tag.includes("btn-primary"))).toBe(true);

    // Ensure the CTA lives inside the page-header section.
    const headerStart = source.indexOf('class="page-header"');
    const ctaPos = source.indexOf('href="/create"');
    expect(headerStart).toBeGreaterThan(-1);
    expect(ctaPos).toBeGreaterThan(headerStart);
  });

  it("has an empty-state fallback linking to /create with 'Create your first server'", () => {
    const emptyStateMatch = source.match(
      /<div\s+class="empty-state">([\s\S]*?)<\/div>/,
    );
    const emptyStateSource = emptyStateMatch?.[1] ?? "";
    expect(emptyStateSource).toMatch(/<A\s[^>]*href=["']\/create["']/);
    expect(source).toContain("Create your first server");
  });
});

// ─── /create route still exists (ticket 009) ────────────────────────

describe("/create route is still registered (ticket 009)", () => {
  const source = loadComponent("index.tsx");

  it("defines a /create route mapped to the CreateServer component", () => {
    expect(source).toMatch(/<Route\s[^>]*path=["']\/create["']/);
    expect(source).toMatch(
      /<Route\s[^>]*path=["']\/create["'][^>]*component=\{CreateServer\}/,
    );
    expect(source).toMatch(
      /import\s+CreateServer\s+from\s+["'][^"']*\/CreateServer["']/,
    );
  });
});
