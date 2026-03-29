import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "../test-utils/loadStylesheet";

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

function loadFromSrc(relativePath: string): string {
  return readFileSync(resolve(__dirname, "..", relativePath), "utf-8");
}

function loadStylesheet(): string {
  return loadProjectStylesheet(__dirname);
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

// ─── Generated TypeScript type ──────────────────────────────────────

describe("ServerResourceStats generated type", () => {
  it("exports a type with all expected fields using number (not bigint)", () => {
    const source = loadComponent("../types/generated/ServerResourceStats.ts");
    expect(source).toMatch(/export type ServerResourceStats/);
    for (const field of [
      /server_id: string/,
      /cpu_percent: number \| null/,
      /memory_rss_bytes: number \| null/,
      /memory_swap_bytes: number \| null/,
      /disk_usage_bytes: number/,
      /timestamp: string/,
    ]) {
      expect(source, `missing ${field.source}`).toMatch(field);
    }
    expect(source).not.toMatch(/bigint/);
  });

  it("is re-exported from type bindings", () => {
    expect(loadComponent("../types/bindings.ts")).toMatch(
      /export type \{ ServerResourceStats \} from "\.\/generated\/ServerResourceStats"/,
    );
  });
});

// ─── API client ─────────────────────────────────────────────────────

describe("ServerResourceStats API client", () => {
  it("api/servers.ts exports getServerStats calling /stats endpoint", () => {
    const apiSource = loadComponent("../api/servers.ts");
    expect(apiSource).toMatch(
      /export function getServerStats\(id: string\): Promise<ServerResourceStats>/,
    );
    expect(apiSource).toMatch(/\/stats/);
    expect(apiSource).toMatch(/ServerResourceStats/);
  });

  it("api/client.ts re-exports getServerStats", () => {
    expect(loadComponent("../api/client.ts")).toMatch(/getServerStats/);
  });
});

// ─── ServerDetail Resources tab ─────────────────────────────────────

describe("ServerDetail Resources tab", () => {
  const source = loadComponent("ServerDetail.tsx");
  // ResourcesTab was extracted into its own component and the polling hook
  // was extracted into useResourceStats.
  const resourcesTab = loadFromSrc("components/ResourcesTab.tsx");
  const resourcesHook = loadFromSrc("hooks/useResourceStats.ts");

  it("imports and uses getServerStats with ServerResourceStats type", () => {
    // getServerStats is now used via the useResourceStats hook
    const combined = source + resourcesHook;
    expect(combined).toMatch(/getServerStats/);
    expect(combined).toMatch(/ServerResourceStats/);
  });

  it("includes 'resources' in the Tab union and renders a Resources tab button", () => {
    expect(source).toMatch(/\| "resources"/);
    expect(source).toMatch(/Resources\s*<\/button>/);
  });

  it("renders resource cards (CPU, Memory/RSS, Disk) with loading and N/A states", () => {
    expect(resourcesTab).toMatch(/class="resources-tab"/);
    expect(resourcesTab).toMatch(/Resource Usage/);
    expect(resourcesTab).toMatch(/Loading resource stats/);
    expect(resourcesTab).toMatch(/Process CPU/);
    expect(resourcesTab).toMatch(/RSS/);
    expect(resourcesTab).toMatch(/Server Directory/);
    expect(resourcesTab).toMatch(/N\/A — server not running/);
  });

  it("uses health-card and health-bar-fill classes for consistent styling", () => {
    expect(resourcesTab).toMatch(/class="health-card"/);
    expect(resourcesTab).toMatch(/health-bar-fill/);
    expect(resourcesTab).toMatch(/thresholdClass/);
  });

  it("uses resources-grid layout and displays swap when available", () => {
    expect(resourcesTab).toMatch(/class="resources-grid"/);
    expect(resourcesTab).toMatch(/Swap/);
  });

  it("sets up a polling interval for resource stats and cleans up on unmount", () => {
    // Polling is now in the useResourceStats hook
    expect(resourcesHook).toMatch(/setInterval\(fetchResourceStats/);
    expect(resourcesHook).toMatch(/clearInterval/);
    expect(resourcesTab).toMatch(/formatBytes/);
  });
});

// ─── ServerCard resource badges ─────────────────────────────────────

describe("ServerCard resource badges", () => {
  const source = loadComponent("../components/ServerCard.tsx");

  it("imports and fetches resource stats with polling and cleanup", () => {
    expect(source).toMatch(/getServerStats/);
    expect(source).toMatch(/ServerResourceStats/);
    expect(source).toMatch(/fetchStats/);
    expect(source).toMatch(/setInterval\(fetchStats/);
    expect(source).toMatch(/onCleanup/);
    expect(source).toMatch(/clearInterval\(statsInterval\)/);
  });

  it("shows memory (RSS), CPU, and disk resource indicators", () => {
    expect(source).toMatch(/Memory \(RSS\)/);
    expect(source).toMatch(/CPU/);
    expect(source).toMatch(/Disk usage/);
    expect(source).toMatch(/resource-mini/);
  });

  it("imports formatBytes from shared utility and conditionally renders resource stats", () => {
    expect(source).toMatch(
      /import\s*\{[^}]*formatBytes[^}]*\}\s*from\s*["']\.\.\/utils\/format["']/,
    );
    expect(source).toMatch(/cpu_percent/);
    expect(source).toMatch(/memory_rss_bytes/);
  });
});

// ─── CSS styles for resources ───────────────────────────────────────

describe("Resources CSS styles", () => {
  const stripped = stripComments(loadStylesheet());

  it("defines resources layout and heading classes", () => {
    expect(stripped).toMatch(/\.resources-tab\s*\{/);
    expect(stripped).toMatch(/\.resources-heading\s*\{/);
  });

  it("defines .resources-grid with 3-column grid layout", () => {
    expect(stripped).toMatch(/\.resources-grid\s*\{[^}]*display:\s*grid/);
    expect(stripped).toMatch(
      /\.resources-grid\s*\{[^}]*grid-template-columns:\s*repeat\(3,\s*1fr\)/,
    );
  });

  it("defines .resource-na with dim italic text and .resource-tag with monospace font", () => {
    const naBlock = stripped.match(/\.resource-na\s*\{([^}]*)/)?.[1] ?? "";
    expect(naBlock).toMatch(/color:\s*var\(--text-dim\)/);
    expect(naBlock).toMatch(/font-style:\s*italic/);

    expect(stripped).toMatch(
      /\.resource-tag\s*\{[^}]*font-family:\s*var\(--mono\)/,
    );
  });

  it("has a responsive breakpoint that collapses resources-grid to 1 column", () => {
    expect(stripped).toMatch(
      /\.resources-grid\s*\{[^}]*grid-template-columns:\s*1fr/,
    );
  });
});
