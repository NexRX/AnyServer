import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "../test-utils/loadStylesheet";

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

function loadStylesheet(): string {
  return loadProjectStylesheet(__dirname);
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

// ─── Loader component source-level tests (ticket 010) ───────────────

describe("Loader component (ticket 010)", () => {
  const source = loadComponent("Loader.tsx");

  it("exports a default Loader component with LoaderProps interface", () => {
    expect(source).toMatch(/export\s+default\s+Loader/);
    expect(source).toMatch(/export\s+interface\s+LoaderProps/);
    expect(source).toMatch(
      /import\s*\{[^}]*Component[^}]*\}\s*from\s*["']solid-js["']/,
    );
  });

  it("accepts optional message and compact props", () => {
    expect(source).toMatch(/message\?:\s*string/);
    expect(source).toMatch(/compact\?:\s*boolean/);
  });

  it("renders the loading CSS class with compact variant support", () => {
    expect(source).toMatch(/class=\{?[`"']?.*\bloading\b/);
    expect(source).toContain("loading-compact");
    expect(source).toMatch(/props\.compact/);
  });

  it("defaults to 'Loading' message and contains no static ellipsis", () => {
    expect(source).toMatch(/props\.message\s*\?\?\s*["']Loading["']/);
    const returnBlock = source.match(/return\s*\(([\s\S]*?)\);/)?.[1] ?? "";
    expect(returnBlock).not.toMatch(/\.{3}/);
    expect(returnBlock).not.toContain("\u2026");
  });
});

// ─── CSS support for the Loader (ticket 010) ────────────────────────

describe("CSS classes used by Loader (ticket 010)", () => {
  const stripped = stripComments(loadStylesheet());

  it("defines .loading with center alignment, padding, and muted color", () => {
    const block = stripped.match(/\.loading\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/text-align:\s*center/);
    expect(block).toMatch(/padding:\s*3rem/);
    expect(block).toMatch(/color:\s*var\(--text-muted\)/);
  });

  it("defines .loading-compact and loader-spinner with SVG animation", () => {
    expect(stripped).toMatch(/\.loading-compact\s*\{/);
    expect(stripped).toMatch(/\.loader-spinner\s*\{/);
    expect(stripped).toMatch(/\.loader-arc\s*\{[^}]*animation:\s*loader-dash/);
  });
});

// ─── All consumers use the shared Loader (ticket 010) ───────────────

describe("all loading states use the shared Loader component (ticket 010)", () => {
  const consumers = [
    { path: "../App.tsx", label: "App" },
    { path: "FileManager.tsx", label: "FileManager" },
    { path: "../pages/AdminPanel.tsx", label: "AdminPanel" },
    { path: "../pages/CreateServer.tsx", label: "CreateServer" },
    { path: "../pages/Dashboard.tsx", label: "Dashboard" },
    { path: "../pages/ServerDetail.tsx", label: "ServerDetail" },
    { path: "../pages/SystemHealth.tsx", label: "SystemHealth" },
    { path: "../pages/Templates.tsx", label: "Templates" },
  ];

  for (const { path, label } of consumers) {
    it(`${label} imports and uses <Loader /> without ad-hoc loading divs`, () => {
      const source = loadComponent(path);
      expect(source).toMatch(/import\s+Loader\s+from\s+["'][^"']*\/Loader["']/);
      expect(source).toMatch(/<Loader\b/);
      expect(source).not.toContain('class="loading"');
    });
  }
});

// ─── FileManager chmod dialog loading (ticket 010) ──────────────────

describe("FileManager chmod dialog loading state (ticket 010)", () => {
  const source = loadComponent("FileManager.tsx");

  it("uses compact <Loader /> for chmod permissions loading without hardcoded color", () => {
    const chmodArea =
      source.match(
        /when=\{!dialog\.loading\}[\s\S]*?fallback=\{([\s\S]*?)\}/,
      )?.[1] ?? "";
    expect(chmodArea).toContain("<Loader");
    expect(chmodArea).toContain("compact");

    const chmodSection =
      source.match(/when=\{!dialog\.loading\}[\s\S]{0,500}/)?.[0] ?? "";
    expect(chmodSection).not.toContain("#9ca3af");
  });
});

// ─── Loader messages are contextual (ticket 010) ────────────────────

describe("Loader messages are contextual (ticket 010)", () => {
  const expectations: [string, string, RegExp][] = [
    [
      "../pages/Dashboard.tsx",
      "Dashboard",
      /<Loader\s[^>]*message=["']Loading servers["']/,
    ],
    [
      "../pages/ServerDetail.tsx",
      "ServerDetail",
      /<Loader\s[^>]*message=["']Loading server["']/,
    ],
    [
      "ResourcesTab.tsx",
      "ServerDetail resources",
      /<Loader\s[^>]*message=["']Loading resource stats["']/,
    ],
    [
      "admin/UsersTab.tsx",
      "AdminPanel",
      /<Loader\s[^>]*message=["']Loading users["']/,
    ],
    [
      "../pages/Templates.tsx",
      "Templates",
      /<Loader\s[^>]*message=["']Loading templates["']/,
    ],
    [
      "../pages/CreateServer.tsx",
      "CreateServer",
      /<Loader\s[^>]*message=["']Loading templates["']/,
    ],
    [
      "../pages/SystemHealth.tsx",
      "SystemHealth",
      /<Loader\s[^>]*message=["']Loading system metrics["']/,
    ],
    [
      "FileManager.tsx",
      "FileManager files",
      /<Loader\s[^>]*message=["']Loading files["']/,
    ],
    [
      "FileManager.tsx",
      "FileManager permissions",
      /<Loader\s[^>]*message=["']Loading permissions["']/,
    ],
  ];

  for (const [path, label, pattern] of expectations) {
    it(`${label} uses a contextual Loader message`, () => {
      expect(loadComponent(path)).toMatch(pattern);
    });
  }

  it("App uses default message for auth loading and 'Redirecting' for redirect", () => {
    const source = loadComponent("../App.tsx");
    expect(source).toMatch(/<Loader\s*\/>/);
    expect(source).toMatch(/<Loader\s[^>]*message=["']Redirecting["']/);
  });
});
